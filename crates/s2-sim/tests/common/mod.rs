//! What the scenario replays share: the map and replay files beside them, and a planner that
//! places buildings and the roads between them, recording the commands it gives.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use s2_formats::map::Map;
use s2_sim::replay::{MapSource, Replay, content_hash};
use s2_sim::{BuildingKind, Command, Direction, Game, MapObject, Node, Player, Setup, Stamped};

/// The longest road a planner draws.
const LONGEST_ROAD: usize = 24;

/// Where the maps and replays live.
pub fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replays")
}

pub fn player(index: u8) -> Player {
    Player::new(index).unwrap()
}

/// A new game on the map file `map` beside the replays, and the replay header for it.
pub fn new_game(map: &str, setup: Setup, waypoints: Vec<u8>, end: u32) -> (Game, Replay) {
    let bytes = std::fs::read(folder().join(map)).unwrap();
    let parsed = Map::parse_wld(&bytes).unwrap();
    let game = Game::start(&parsed, &setup).unwrap();
    let mut replay = Replay::new(MapSource::File(map.into()), content_hash(&bytes), None, setup);
    replay.waypoints = waypoints;
    replay.end = end;
    (game, replay)
}

/// The game replay `name` starts with, and the replay itself.
pub fn recorded(map: &str, name: &str, setup: Setup, waypoints: Vec<u8>, end: u32) -> (Game, Replay) {
    let text = std::fs::read_to_string(folder().join(name)).unwrap();
    let replay = Replay::parse(&text).unwrap();
    let (mut game, _) = new_game(map, setup, waypoints, end);
    replay.apply_players(&mut game);
    (game, replay)
}

/// Writes replay `name` with `header` in place of the usual first comment line.
pub fn write_replay(name: &str, replay: &Replay, header: &str) {
    let text = replay.to_text().replacen("# robigo replay\n", header, 1);
    std::fs::write(folder().join(name), text).unwrap();
}

/// Runs `game` with `replay`'s commands until `until`, calling `each` after every step.
pub fn run(game: &mut Game, replay: &Replay, until: u32, mut each: impl FnMut(&Game)) {
    let mut pending = replay.commands.iter().peekable();
    while game.clock().step() < until {
        let now = game.clock().step();
        while let Some(command) = pending.next_if(|c| c.step <= now) {
            let _ = game.execute(command.player, command.command);
        }
        game.step();
        each(game);
    }
}

/// `player`'s headquarters.
pub fn headquarters(game: &Game, player: Player) -> Node {
    game.world()
        .buildings()
        .find(|(_, building)| building.owner == player && building.kind == BuildingKind::HEADQUARTERS)
        .map(|(_, building)| building.node)
        .unwrap()
}

/// The building record on `node`.
pub fn building_at(game: &Game, node: Node) -> u16 {
    match game.world().object(node) {
        MapObject::Building(record) => record,
        other => panic!("{other:?}"),
    }
}

/// Plays a game for one player, keeping every command that took effect.
pub struct Planner {
    pub game: Game,
    pub player: Player,
    pub commands: Vec<Stamped>,
}

impl Planner {
    pub fn new(game: Game, player: Player) -> Self {
        Self {
            game,
            player,
            commands: Vec::new(),
        }
    }

    /// Gives `command` and records it when it took effect.
    pub fn execute(&mut self, command: Command) -> bool {
        let done = self.game.execute(self.player, command).is_ok();
        if done {
            self.commands.push(Stamped {
                step: self.game.clock().step(),
                player: self.player,
                command,
            });
        }
        done
    }

    pub fn step(&self) -> u32 {
        self.game.clock().step()
    }

    pub fn neighbour(&self, node: Node, direction: Direction) -> Node {
        let (width, height) = (self.game.nodes().width(), self.game.nodes().height());
        node.neighbour(direction, width, height)
    }

    /// Every node the player owns, in row order.
    pub fn owned(&self) -> Vec<Node> {
        let (width, height) = (self.game.nodes().width(), self.game.nodes().height());
        (0..height)
            .flat_map(|y| (0..width).map(move |x| Node { x, y }))
            .filter(|&node| self.game.nodes().get(node).owner == Some(self.player))
            .collect()
    }

    /// Whether a road may pass `node`.
    pub fn passable(&self, node: Node) -> bool {
        let data = self.game.nodes().get(node);
        data.owner == Some(self.player)
            && data.roads.is_empty()
            && data.sites.passable()
            && matches!(self.game.world().object(node), MapObject::Nothing)
    }

    /// A road from the flag on `from` to the nearest flag already on a road, by breadth-first
    /// search, with flags placed along it.
    pub fn connect(&mut self, from: Node) -> bool {
        let world = self.game.world();
        let home = world
            .buildings()
            .find(|(_, building)| {
                building.owner == self.player && building.kind == BuildingKind::HEADQUARTERS
            })
            .map(|(_, building)| building.flag);
        let joined = |node: Node| {
            world.flag_at(node).is_some_and(|record| {
                Some(record) == home
                    || world
                        .flag(record)
                        .is_some_and(|flag| flag.links.iter().any(Option::is_some))
            })
        };
        let mut came = BTreeMap::new();
        let mut queue = VecDeque::from([(from, 0)]);
        let mut seen = BTreeSet::from([from]);
        let mut goal = None;
        while let Some((node, depth)) = queue.pop_front() {
            if node != from && world.flag_at(node).is_some() {
                if joined(node) {
                    goal = Some(node);
                    break;
                }
                continue;
            }
            if depth >= LONGEST_ROAD {
                continue;
            }
            for direction in Direction::ALL {
                let next = self.neighbour(node, direction);
                if seen.contains(&next) {
                    continue;
                }
                if world.flag_at(next).is_none() && !self.passable(next) {
                    continue;
                }
                seen.insert(next);
                came.insert(next, (node, direction));
                queue.push_back((next, depth + 1));
            }
        }
        let Some(goal) = goal else {
            return false;
        };
        let mut steps = Vec::new();
        let mut at = goal;
        while at != from {
            let (back, direction) = came[&at];
            steps.push(direction);
            at = back;
        }
        steps.reverse();
        if !self.execute(Command::StartRoad {
            node: from,
            water: false,
        }) {
            return false;
        }
        for direction in steps {
            if !self.execute(Command::ExtendRoad { direction }) {
                self.execute(Command::CancelRoad);
                return false;
            }
        }
        true
    }

    /// Places a `kind` on the first node of `candidates` that takes one, with a road from its
    /// flag to the network. Returns its node.
    pub fn place(&mut self, kind: BuildingKind, candidates: &[Node]) -> Option<Node> {
        for &node in candidates {
            if !self.execute(Command::PlaceBuilding { node, kind }) {
                continue;
            }
            let flag = self.neighbour(node, Direction::SouthEast);
            if self.connect(flag) {
                return Some(node);
            }
            self.execute(Command::Demolish { node });
        }
        None
    }

    /// Steps until `done`, or `limit` steps have passed. Returns whether `done` came true.
    pub fn run_until(&mut self, limit: u32, done: impl Fn(&Game) -> bool) -> bool {
        for _ in 0..limit {
            self.game.step();
            if done(&self.game) {
                return true;
            }
        }
        false
    }
}
