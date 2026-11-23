//! The whole economy on a real map, "The Green Island" (`DATA/MAPS3/OMAP03.WLD`). Until military
//! buildings claim land each player has only its headquarters' nine rings, too little for every
//! building, so all four players share the chains, with player 1, whose land holds coal, iron ore
//! and gold, running the metals. The replay `replays/omap03-economy.replay` places the buildings
//! and connects each by road; the replays test pins its state hashes. These tests need the
//! player's own S2 data through `S2_DATA_PLAIN`.
//!
//! `ROBIGO_ECONOMY=write cargo test -p s2-sim --test economy -- --ignored write` places the
//! buildings again and rewrites the replay's commands; bless it afterwards. The soak runs the
//! replay on for four hours of game time, twice.
//!
//! Each test here runs for hours of game time and takes minutes, so `cargo test` leaves them out.
//! `cargo test -p s2-sim --test economy -- --include-ignored` runs them, after a change to the
//! economy and before a release.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use s2_data::DataSet;
use s2_formats::bob::RomAnim;
use s2_formats::map::Map;
use s2_sim::replay::{MapSource, Replay, content_hash};
use s2_sim::{
    AnimationScript, BuildingKind, BuildingState, Command, Direction, Game, MapObject, Node, Player,
    Property, Resources, Setup, Stamped, StartGoods, WareKind,
};

const MAP: &str = "DATA/MAPS3/OMAP03.WLD";
const ROM_ANIM: &str = "DATA/ANIMDAT/ROM_ANIM.DAT";

/// Steps the replay runs: about three hours of game time, long enough for the farms' grain to
/// reach bread and beer.
const END: u32 = 80_000;

/// Four hours of game time at 7.5 steps a second.
const SOAK: u32 = 108_000;

/// Steps between integrity checks in the soak.
const CHECK_EVERY: u32 = 1_000;

/// How far round a site the generator looks for what its building wants.
const NEAR_REACH: u16 = 6;

/// How far a mine digs from its site (search kinds 12 to 15).
const MINE_REACH: u16 = 2;

/// The rings round a farm kept clear for its fields (the farmer's reach, search kinds 1 and 2).
const FARM_ROOM: u16 = 2;

/// The rings round a forester kept clear for its trees.
const FORESTER_ROOM: u16 = 1;

/// The longest road the generator draws.
const LONGEST_ROAD: usize = 24;

/// What a building wants near its site.
#[derive(Clone, Copy)]
enum Near {
    Headquarters,
    Trees,
    Granite,
    Fish,
    Water,
    Mineral(u8),
}

/// What each player builds, in order, with what each building wants near it: mines first, on
/// the few nodes within reach of their mineral, then the large buildings while there is room,
/// the medium and the small. Player 1 has the minerals and
/// runs the metal chains, fed by fish and game; player 3, on flat land, bread, beer and boats;
/// player 2 donkeys; player 0 meat.
const PLANS: [(u8, &[(BuildingKind, Near)]); 4] = [
    (
        1,
        &[
            (BuildingKind::COAL_MINE, Near::Mineral(0x00)),
            (BuildingKind::GOLD_MINE, Near::Mineral(0x10)),
            (BuildingKind::IRON_MINE, Near::Mineral(0x08)),
            (BuildingKind::SAWMILL, Near::Headquarters),
            (BuildingKind::IRON_SMELTER, Near::Headquarters),
            (BuildingKind::METALWORKS, Near::Headquarters),
            (BuildingKind::ARMOURY, Near::Headquarters),
            (BuildingKind::MINT, Near::Headquarters),
            (BuildingKind::WOODCUTTER, Near::Trees),
            (BuildingKind::FORESTER, Near::Trees),
            (BuildingKind::QUARRY, Near::Granite),
            (BuildingKind::FISHERY, Near::Fish),
            (BuildingKind::HUNTER, Near::Trees),
            (BuildingKind::LOOKOUT_TOWER, Near::Headquarters),
        ],
    ),
    (
        3,
        &[
            (BuildingKind::FARM, Near::Headquarters),
            (BuildingKind::FARM, Near::Headquarters),
            (BuildingKind::SAWMILL, Near::Headquarters),
            (BuildingKind::MILL, Near::Headquarters),
            (BuildingKind::BAKERY, Near::Headquarters),
            (BuildingKind::BREWERY, Near::Headquarters),
            (BuildingKind::SHIPYARD, Near::Headquarters),
            (BuildingKind::WOODCUTTER, Near::Trees),
            (BuildingKind::FORESTER, Near::Trees),
            (BuildingKind::WELL, Near::Water),
            (BuildingKind::WELL, Near::Water),
        ],
    ),
    (
        2,
        &[
            (BuildingKind::FARM, Near::Headquarters),
            (BuildingKind::DONKEY_BREEDER, Near::Headquarters),
            (BuildingKind::WOODCUTTER, Near::Trees),
            (BuildingKind::QUARRY, Near::Granite),
            (BuildingKind::WELL, Near::Water),
        ],
    ),
    (
        0,
        &[
            (BuildingKind::FARM, Near::Headquarters),
            (BuildingKind::PIG_FARM, Near::Headquarters),
            (BuildingKind::SLAUGHTERHOUSE, Near::Headquarters),
            (BuildingKind::WOODCUTTER, Near::Trees),
            (BuildingKind::FORESTER, Near::Trees),
            (BuildingKind::QUARRY, Near::Granite),
            (BuildingKind::WELL, Near::Water),
        ],
    ),
];

/// The distribution page for iron: the armoury first, then the metalworks.
const IRON_GROUP: u8 = 2;

/// Player 1's iron sliders, level as a player would set them so the metalworks gets iron too: at
/// the defaults the armoury's 80 against the metalworks' 40 takes it all.
const IRON_TO_ARMOURY: u8 = 80;
const IRON_TO_METALWORKS: u8 = 80;

/// Steps between samples of what the economy is doing.
const SAMPLE_EVERY: u32 = 10;

/// The buildings that run work cycles, each of which must have run one by the replay's end. The
/// lookout tower only watches.
const WORKS: [BuildingKind; 24] = [
    BuildingKind::WOODCUTTER,
    BuildingKind::FORESTER,
    BuildingKind::QUARRY,
    BuildingKind::FISHERY,
    BuildingKind::HUNTER,
    BuildingKind::SAWMILL,
    BuildingKind::FARM,
    BuildingKind::PIG_FARM,
    BuildingKind::DONKEY_BREEDER,
    BuildingKind::SLAUGHTERHOUSE,
    BuildingKind::MILL,
    BuildingKind::BAKERY,
    BuildingKind::IRON_SMELTER,
    BuildingKind::METALWORKS,
    BuildingKind::ARMOURY,
    BuildingKind::MINT,
    BuildingKind::SHIPYARD,
    BuildingKind::BREWERY,
    BuildingKind::GOLD_MINE,
    BuildingKind::IRON_MINE,
    BuildingKind::COAL_MINE,
    BuildingKind::GRANITE_MINE,
    BuildingKind::WELL,
    BuildingKind::LOOKOUT_TOWER,
];

/// The wares every chain ends in, which the economy must have carried by the replay's end. The
/// armoury makes a shield first and a sword only on its second cycle, which player 1's scarce
/// iron does not reach in this run.
const PRODUCED: [WareKind; 17] = [
    WareKind::WOOD,
    WareKind::BOARDS,
    WareKind::STONES,
    WareKind::FISH,
    WareKind::MEAT,
    WareKind::GRAIN,
    WareKind::FLOUR,
    WareKind::BREAD,
    WareKind::WATER,
    WareKind::HAM,
    WareKind::BEER,
    WareKind::COAL,
    WareKind::IRON_ORE,
    WareKind::GOLD,
    WareKind::IRON,
    WareKind::COINS,
    WareKind::SHIELD,
];

fn data() -> Option<DataSet> {
    std::env::var_os("S2_DATA_PLAIN").map(|path| DataSet::discover(PathBuf::from(path)).unwrap())
}

fn replay_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replays/omap03-economy.replay")
}

fn setup() -> Setup {
    Setup {
        players: Some(4),
        animals: true,
        goods: StartGoods::Normal,
        slots: None,
        starts: s2_sim::StartOrder::InSequence,
        objective: s2_sim::Objective::None,
        bug_fixes: s2_sim::BugFixes::Off,
        mission: None,
    }
}

/// A new game on the map with the animation script, and the replay header for it.
fn new_game(data: &DataSet) -> (Game, Replay) {
    let bytes = data.read(MAP).unwrap();
    let script = data.read(ROM_ANIM).unwrap();
    let map = Map::parse_wld(&bytes).unwrap();
    let mut game = Game::start(&map, &setup()).unwrap();
    game.set_animations(AnimationScript::new(&RomAnim::parse(&script).unwrap()));
    let mut replay = Replay::new(
        MapSource::Data(MAP.into()),
        content_hash(&bytes),
        Some(content_hash(&script)),
        setup(),
    );
    replay.waypoints = vec![0, 0, 0, 0];
    replay.end = END;
    (game, replay)
}

/// The game at the replay's start, with its commands.
fn recorded(data: &DataSet) -> (Game, Replay) {
    let text = std::fs::read_to_string(replay_path()).unwrap();
    let replay = Replay::parse(&text).unwrap();
    let (mut game, _) = new_game(data);
    replay.apply_players(&mut game);
    (game, replay)
}

/// Places the plan's buildings and their roads, recording each command that took effect.
struct Planner {
    game: Game,
    player: Player,
    commands: Vec<Stamped>,
    /// Land kept clear for farms' fields and foresters' trees.
    reserved: BTreeSet<Node>,
}

impl Planner {
    fn execute(&mut self, command: Command) -> bool {
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

    fn size(&self) -> (u16, u16) {
        (self.game.nodes().width(), self.game.nodes().height())
    }

    fn neighbour(&self, node: Node, direction: Direction) -> Node {
        let (width, height) = self.size();
        node.neighbour(direction, width, height)
    }

    fn owned(&self) -> Vec<Node> {
        let (width, height) = self.size();
        (0..height)
            .flat_map(|y| (0..width).map(move |x| Node { x, y }))
            .filter(|&node| self.game.nodes().get(node).owner == Some(self.player))
            .collect()
    }

    fn headquarters(&self) -> Node {
        self.game
            .world()
            .buildings()
            .find(|(_, building)| {
                building.owner == self.player && building.kind == BuildingKind::HEADQUARTERS
            })
            .map(|(_, building)| building.node)
            .unwrap()
    }

    /// Whether `node` holds what `near` looks for.
    fn wanted(&self, node: Node, near: Near) -> bool {
        let data = self.game.nodes().get(node);
        match near {
            Near::Headquarters => false,
            Near::Trees => matches!(self.game.world().object(node), MapObject::Tree(_)),
            Near::Granite => matches!(self.game.world().object(node), MapObject::Granite(_)),
            Near::Fish => {
                self.game.nodes().property(node).contains(Property::ALL_WATER) && data.resources.fishable()
            }
            Near::Water => matches!(data.resources, Resources::Water(amount) if amount > 0),
            Near::Mineral(bits) => {
                matches!(data.resources, Resources::Mineral(_, amount) if amount > 0)
                    && data.resources.mineral_bits() == bits
            }
        }
    }

    /// Nodes in rings round `centre` out to `radius`, ring by ring.
    fn rings(&self, centre: Node, radius: u16) -> Vec<Node> {
        let mut out = vec![centre];
        let mut at = centre;
        for ring in 1..=radius {
            at = self.neighbour(at, Direction::NorthWest);
            for direction in [
                Direction::East,
                Direction::SouthEast,
                Direction::SouthWest,
                Direction::West,
                Direction::NorthWest,
                Direction::NorthEast,
            ] {
                for _ in 0..ring {
                    at = self.neighbour(at, direction);
                    out.push(at);
                }
            }
        }
        out
    }

    /// How far `node` is from what `near` wants, in rings, within reach; the headquarters' distance
    /// for buildings that want nothing in particular.
    fn score(&self, node: Node, near: Near, headquarters: Node) -> usize {
        let target = match near {
            Near::Headquarters => {
                return self
                    .rings(headquarters, 12)
                    .iter()
                    .position(|&n| n == node)
                    .unwrap_or(usize::MAX);
            }
            _ => near,
        };
        let reach = if matches!(target, Near::Mineral(_)) {
            MINE_REACH
        } else {
            NEAR_REACH
        };
        self.rings(node, reach)
            .iter()
            .position(|&n| self.wanted(n, target))
            .unwrap_or(usize::MAX)
    }

    /// Places a `kind` on the best site that takes it and a road from its flag to the network.
    fn place(&mut self, kind: BuildingKind, near: Near) -> bool {
        let headquarters = self.headquarters();
        let mut candidates: Vec<(usize, Node)> = self
            .owned()
            .into_iter()
            .map(|node| (self.score(node, near, headquarters), node))
            .filter(|&(score, _)| score != usize::MAX)
            .collect();
        candidates.sort();
        for (_, node) in candidates {
            if !self.execute(Command::PlaceBuilding { node, kind }) {
                continue;
            }
            let flag = self.neighbour(node, Direction::SouthEast);
            if self.reserved.contains(&node) || self.reserved.contains(&flag) || !self.connect(flag) {
                self.execute(Command::Demolish { node });
                continue;
            }
            let room = match kind {
                BuildingKind::FARM => FARM_ROOM,
                BuildingKind::FORESTER => FORESTER_ROOM,
                _ => 0,
            };
            let keep: Vec<Node> = self.rings(node, room).into_iter().skip(1).collect();
            self.reserved
                .extend(keep.into_iter().filter(|&kept| kept != flag));
            return true;
        }
        false
    }

    /// Whether a road may pass `node`.
    fn passable(&self, node: Node) -> bool {
        let data = self.game.nodes().get(node);
        data.owner == Some(self.player)
            && !self.reserved.contains(&node)
            && data.roads.is_empty()
            && data.sites.passable()
            && matches!(self.game.world().object(node), MapObject::Nothing)
    }

    /// A road from the flag on `from` to the nearest flag already on a road, by breadth-first
    /// search, with flags placed along it.
    fn connect(&mut self, from: Node) -> bool {
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
        let mut came = std::collections::BTreeMap::new();
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
            steps.push((direction, at));
            at = back;
        }
        steps.reverse();
        if !self.execute(Command::StartRoad {
            node: from,
            water: false,
        }) {
            return false;
        }
        for &(direction, _) in &steps {
            if !self.execute(Command::ExtendRoad { direction }) {
                self.execute(Command::CancelRoad);
                return false;
            }
        }
        if self.game.road_draft(self.player).is_some() {
            self.execute(Command::CancelRoad);
            return false;
        }
        for &(_, node) in steps.iter().skip(1).step_by(2) {
            if node != goal && !self.reserved.contains(&node) {
                self.execute(Command::PlaceFlag { node, water: false });
            }
        }
        true
    }
}

#[test]
#[ignore = "writes the economy replay; run with ROBIGO_ECONOMY=write"]
fn write() {
    let (Some(data), Some(_)) = (data(), std::env::var_os("ROBIGO_ECONOMY")) else {
        return;
    };
    let (game, mut replay) = new_game(&data);
    let mut planner = Planner {
        game,
        player: Player::new(0).unwrap(),
        commands: Vec::new(),
        reserved: BTreeSet::new(),
    };
    for (player, plan) in PLANS {
        planner.player = Player::new(player).unwrap();
        for &(kind, near) in plan {
            let placed = planner.place(kind, near);
            eprintln!(
                "player {player} {kind:?}: {}",
                if placed { "placed" } else { "no room" }
            );
        }
    }
    planner.player = Player::new(1).unwrap();
    for (consumer, weight) in [(0, IRON_TO_ARMOURY), (1, IRON_TO_METALWORKS)] {
        planner.execute(Command::SetDistribution {
            group: IRON_GROUP,
            consumer,
            weight,
        });
    }
    replay.commands = planner.commands;
    let header = "# The four players on \"The Green Island\" place every economy building \
                  between them, metals\n# for player 1, each connected by road. Written by \
                  tests/economy.rs; needs the player's own S2\n# data.\n";
    let text = replay.to_text().replacen("# robigo replay\n", header, 1);
    std::fs::write(replay_path(), text).unwrap();
}

/// Runs `game` with `replay`'s commands until `until`, calling `each` after every step.
fn run(game: &mut Game, replay: &Replay, until: u32, mut each: impl FnMut(&Game)) {
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

#[test]
#[ignore = "minutes long; run with --include-ignored after a change to the economy and before a release"]
fn a_repair_by_the_tables_the_game_plays_by_changes_nothing() {
    let Some(data) = data() else {
        eprintln!("skipped, set S2_DATA_PLAIN to run it");
        return;
    };
    let (mut game, replay) = recorded(&data);
    run(&mut game, &replay, replay.end, |game| {
        if game.clock().step().is_multiple_of(CHECK_EVERY) {
            let mut repaired = game.clone();
            let saved = game.world().building_types().clone();
            assert_eq!(repaired.repair(&saved), [], "step {}", game.clock().step());
            assert_eq!(
                repaired.state_hash(),
                game.state_hash(),
                "step {}",
                game.clock().step()
            );
        }
    });
}

#[test]
#[ignore = "minutes long; run with --include-ignored after a change to the economy and before a release"]
fn every_chain_produces_its_wares() {
    let Some(data) = data() else {
        eprintln!("skipped, set S2_DATA_PLAIN to run it");
        return;
    };
    let (mut game, replay) = recorded(&data);
    let mut seen = BTreeSet::new();
    let mut worked = BTreeSet::new();
    run(&mut game, &replay, replay.end, |game| {
        if game.clock().step().is_multiple_of(SAMPLE_EVERY) {
            seen.extend(game.world().wares().map(|(_, ware)| ware.kind));
            worked.extend(
                game.world()
                    .buildings()
                    .filter(|(_, building)| building.state.contains(BuildingState::WORKING))
                    .map(|(_, building)| building.kind),
            );
        }
    });
    let idle: Vec<_> = PLANS
        .iter()
        .flat_map(|(_, plan)| plan.iter().map(|&(kind, _)| kind))
        .filter(|kind| WORKS.contains(kind) && !worked.contains(kind))
        .collect();
    assert!(idle.is_empty(), "never worked: {idle:?}");
    let missing: Vec<_> = PRODUCED.iter().filter(|ware| !seen.contains(ware)).collect();
    assert!(missing.is_empty(), "never carried: {missing:?}");
    assert!(
        seen.iter().any(|ware| ware.tool().is_some()),
        "the metalworks made no tool"
    );
    let broken = game.world().broken_links();
    assert!(broken.is_empty(), "{broken:#?}");
}

#[test]
#[ignore = "minutes long; run with --include-ignored after a change to the economy and before a release"]
fn soak() {
    let Some(data) = data() else {
        return;
    };
    let mut hashes = Vec::new();
    for _ in 0..2 {
        let (mut game, replay) = recorded(&data);
        run(&mut game, &replay, SOAK, |game| {
            if game.clock().step().is_multiple_of(CHECK_EVERY) {
                let broken = game.world().broken_links();
                assert!(broken.is_empty(), "step {}: {broken:#?}", game.clock().step());
            }
        });
        hashes.push(game.state_hash());
    }
    assert_eq!(hashes[0], hashes[1], "two runs ended apart");
}
