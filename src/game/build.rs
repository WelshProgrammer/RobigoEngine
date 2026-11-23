//! Debug build controls: one player at a time places flags and draws roads with the mouse, the
//! road being drawn shows S2's icons round its head, and every command that changed the game
//! goes into a log that `--record` saves.

use std::collections::BTreeSet;

use s2_render::terrain::{NODE_WIDTH, Point, ScreenNode};
use s2_render::{Mark, Place};
use s2_sim::{
    BuildingKind, Command, Direction, Game, Job, MapObject, Node, NodeFlags, Player, Refusal, SiteSize,
    Stamped, StepPreview,
};

use super::record::Queue;

/// How far past the frame's right and bottom edges a node may sit and still be drawn there
/// rather than one map period back.
const WRAP_MARGIN: i32 = NODE_WIDTH;

/// The modifier keys held during a click.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Modifiers {
    /// Shift: water flags and water roads.
    pub(crate) shift: bool,
    /// Ctrl: remove instead of place.
    pub(crate) ctrl: bool,
}

/// The controls' state: who is building, the node under the cursor and the commands so far.
pub(crate) struct Builder {
    player: Player,
    hovered: Option<Node>,
    placing: Option<BuildingKind>,
    log: Vec<Stamped>,
}

impl Builder {
    /// Controls for player `start`, or the first player with a headquarters when `start` has none.
    /// The log begins with the commands `replay` has already carried out.
    pub(crate) fn new(game: &Game, start: usize, replay: &Queue) -> Self {
        let players = players(game);
        let player = players
            .iter()
            .copied()
            .find(|p| p.index() == start)
            .or_else(|| players.first().copied())
            .or_else(|| Player::new(0))
            .unwrap_or_else(|| unreachable!("player 0 exists"));
        Self {
            player,
            hovered: None,
            placing: None,
            log: replay.done().to_vec(),
        }
    }

    /// The commands carried out so far, replayed ones included.
    pub(crate) fn log(&self) -> &[Stamped] {
        &self.log
    }

    /// Switches to the next player with a headquarters.
    pub(crate) fn next_player(&mut self, game: &Game) {
        let players = players(game);
        let next = players
            .iter()
            .copied()
            .find(|p| p.index() > self.player.index())
            .or_else(|| players.first().copied());
        if let Some(next) = next {
            self.player = next;
        }
        log::info!("building as player {}", self.player.index());
    }

    /// Starts placing buildings, or chooses the next building type, or the one before with
    /// `back`, skipping the headquarters.
    pub(crate) fn next_building(&mut self, game: &Game, back: bool) {
        let types = game.world().building_types().types();
        let count = u8::try_from(types.len()).unwrap_or(u8::MAX);
        if count < 2 {
            return;
        }
        let step = |BuildingKind(kind): BuildingKind| {
            BuildingKind(if back {
                kind.checked_sub(1).unwrap_or(count - 1)
            } else {
                (kind + 1) % count
            })
        };
        let mut kind = match self.placing {
            Some(kind) => step(kind),
            None if back => BuildingKind(count - 1),
            None => BuildingKind::default(),
        };
        if kind == BuildingKind::HEADQUARTERS {
            kind = step(kind);
        }
        self.placing = Some(kind);
        let size = types.get(kind.index()).map(|row| row.size);
        log::info!(
            "a click places building type {} ({size:?}); right click to stop",
            kind.0
        );
    }

    /// Notes the node under the cursor.
    pub(crate) fn hover(&mut self, node: Option<Node>) {
        self.hovered = node;
    }

    /// Runs one step of `game`, after the commands `replay` holds for this step.
    pub(crate) fn advance(&mut self, game: &mut Game, replay: &mut Queue) {
        self.log.extend_from_slice(replay.catch_up(game));
        game.step();
    }

    /// Acts on a click on `node`. While drawing, a click on the head places a flag there, a click
    /// next to it extends the road and a click further away takes the road there along the
    /// route the game plans. Otherwise a click on one of the player's flags
    /// starts a road and a click elsewhere places a flag; Shift makes them water roads and
    /// flags, and Ctrl removes the flag or road under the cursor, or demolishes the building
    /// there, instead. While placing
    /// buildings a click places a site of the chosen type.
    pub(crate) fn click(&mut self, game: &mut Game, node: Node, modifiers: Modifiers) {
        let Some(command) = self.command_at(game, node, modifiers) else {
            return;
        };
        self.execute(game, command);
    }

    /// Sends a geologist or scout to the flag under the cursor.
    pub(crate) fn send(&mut self, game: &mut Game, job: Job) {
        if let Some(node) = self.hovered {
            let command = if job == Job::SCOUT {
                Command::SendScout { node }
            } else {
                Command::SendGeologist { node }
            };
            self.execute(game, command);
        }
    }

    /// Stops placing buildings, or stops drawing and drops the road drawn so far.
    pub(crate) fn cancel(&mut self, game: &mut Game) {
        if self.placing.take().is_some() {
            log::info!("clicks place flags and roads again");
        } else if game.road_draft(self.player).is_some() {
            self.execute(game, Command::CancelRoad);
        }
    }

    /// Whether a click on `node` is the builder's rather than the node menu's: any click while a
    /// road is drawn, or the site of the building being placed.
    pub(crate) fn takes(&self, game: &Game, node: Node) -> bool {
        self.placing.is_some() || self.road_step(game, node, false).is_some()
    }

    /// What a click on `node` does to the road being drawn: a flag at the head, a step to a
    /// node beside it, or a route to any other node. `None` when no road is being drawn.
    fn road_step(&self, game: &Game, node: Node, water: bool) -> Option<Command> {
        let draft = game.road_draft(self.player)?;
        let head = draft.head(game.world())?;
        if node == head {
            return Some(Command::PlaceFlag { node, water });
        }
        let nodes = game.nodes();
        Some(
            head.direction_to(node, nodes.width(), nodes.height())
                .map_or(Command::RouteRoad { node }, |direction| Command::ExtendRoad {
                    direction,
                }),
        )
    }

    fn command_at(&self, game: &Game, node: Node, modifiers: Modifiers) -> Option<Command> {
        let world = game.world();
        if game.road_draft(self.player).is_some() {
            return self.road_step(game, node, modifiers.shift);
        }
        if let Some(kind) = self.placing {
            return Some(Command::PlaceBuilding { node, kind });
        }
        if modifiers.ctrl && matches!(world.object(node), MapObject::Building(_)) {
            return Some(Command::Demolish { node });
        }
        let flag = world.flag_at(node);
        Some(match (flag, modifiers.ctrl) {
            (Some(_), true) => Command::RemoveFlag { node },
            (None, true) => Command::RemoveRoad { node },
            (Some(_), false) => Command::StartRoad {
                node,
                water: modifiers.shift,
            },
            (None, false) => Command::PlaceFlag {
                node,
                water: modifiers.shift,
            },
        })
    }

    /// The head of the road being drawn, if one is.
    pub(crate) fn head(&self, game: &Game) -> Option<Node> {
        game.road_draft(self.player)?.head(game.world())
    }

    /// The player the controls build as.
    pub(crate) fn player(&self) -> Player {
        self.player
    }

    /// Carries out `command` for that player and keeps it in the log.
    pub(crate) fn execute(&mut self, game: &mut Game, command: Command) {
        let stamped = Stamped {
            step: game.clock().step(),
            player: self.player,
            command,
        };
        match game.execute(self.player, command) {
            Ok(()) => {
                log::info!(
                    "step {}, player {}: {command:?}",
                    stamped.step,
                    self.player.index()
                );
                self.log.push(stamped);
            }
            Err(refusal @ Refusal::Road(_)) => {
                log::info!("{command:?}: drawing stopped, {refusal:?}");
                self.log.push(stamped);
            }
            Err(refusal) => log::info!("{command:?} refused: {refusal:?}"),
        }
    }

    /// What the view marks over the objects for these controls (`0x1fa5c`): while a road is
    /// drawn, the icons round its head and the head itself; otherwise the node of the open
    /// action menu, `menu`; and last the node under the mouse.
    pub(crate) fn marks(&self, game: &Game, menu: Option<Node>) -> Vec<Mark> {
        let mut marks = Vec::new();
        let mut mark = |place, item| marks.push(Mark { place, item });
        let nodes = game.nodes();
        let draft = game
            .road_draft(self.player)
            .and_then(|draft| Some((draft.head(game.world())?, draft.back(game.world()))));
        if let Some((head, back)) = draft {
            for direction in Direction::ALL {
                let to = head.neighbour(direction, nodes.width(), nodes.height());
                if back == Some(direction) {
                    mark(Place::Node(to), item::BACK);
                    continue;
                }
                let rise = i32::from(nodes.get(to).altitude) - i32::from(nodes.get(head).altitude);
                let slope = slope_item(rise);
                match game.preview_step(self.player, direction) {
                    Some(StepPreview::Free) => {
                        if let Some(slope) = slope {
                            mark(Place::Node(to), slope);
                        }
                    }
                    Some(StepPreview::Flag) => {
                        if let Some(slope) = slope {
                            mark(Place::Between(head, to), slope);
                        }
                        mark(Place::Node(to), item::FLAG);
                    }
                    _ => {}
                }
            }
            mark(Place::Node(head), item::HEAD);
        } else if let Some(node) = menu {
            mark(Place::Node(node), item::FLAG);
        }
        if let Some(node) = self.hovered {
            let data = nodes.get(node);
            let site = (draft.is_none() && data.owner == Some(self.player))
                .then(|| site_item(data.sites.size, data.flags.contains(NodeFlags::HARBOUR)))
                .flatten();
            mark(Place::Node(node), site.unwrap_or(item::HOVER));
        }
        marks
    }
}

/// The players with a headquarters, lowest first.
fn players(game: &Game) -> Vec<Player> {
    let owners: BTreeSet<usize> = game
        .world()
        .buildings()
        .filter(|(_, building)| building.kind == BuildingKind::HEADQUARTERS)
        .map(|(_, building)| building.owner.index())
        .collect();
    owners
        .into_iter()
        .filter_map(|index| u8::try_from(index).ok().and_then(Player::new))
        .collect()
}

/// Where `node` is drawn on a `size` frame scrolled to `scroll`, choosing the copy of the wrapping map
/// that lands on or just past the frame.
pub(crate) fn on_frame(
    game: &Game,
    node: Node,
    scroll: Point,
    period: (u32, u32),
    size: (u32, u32),
) -> Point {
    let world = node.world_position(game.nodes().get(node).altitude);
    let wrap = |offset: i32, period: u32, size: u32| {
        let period = i32::try_from(period).unwrap_or(i32::MAX).max(1);
        let offset = offset.rem_euclid(period);
        if offset
            > i32::try_from(size)
                .unwrap_or(i32::MAX)
                .saturating_add(WRAP_MARGIN)
        {
            offset - period
        } else {
            offset
        }
    };
    Point::new(
        wrap(world.x - scroll.x, period.0, size.0),
        wrap(world.y - scroll.y, period.1, size.1),
    )
}

/// Items of the landscape set's `MAP_<set>_Z.LST` the controls show.
mod item {
    /// A flag the next step would end on, and the node of the open action menu.
    pub(super) const FLAG: usize = 20;
    /// The head of the road being drawn.
    pub(super) const HEAD: usize = 21;
    /// The node under the mouse while drawing, or where it offers nothing to build.
    pub(super) const HOVER: usize = 22;
    /// The step back along the last segment.
    pub(super) const BACK: usize = 67;
    /// A flat step; the rises and falls follow it (`0x1f9050`).
    pub(super) const FLAT: usize = 60;
    /// The node under the mouse when it takes a flag; mines, small, medium, large and harbour
    /// sites follow it.
    pub(super) const SITE_FLAG: usize = 40;
}

/// The slope icon for a step that rises by `rise`, from the table at `0x1f9050`, which runs
/// from −10 to 10. S2 reads past the table for anything larger; here there is no icon.
fn slope_item(rise: i32) -> Option<usize> {
    let up = |n: usize| item::FLAT + n;
    let down = |n: usize| item::FLAT + 3 + n;
    Some(match rise {
        -10..=-3 => down(3),
        -2 => down(2),
        -1 => down(1),
        0 => item::FLAT,
        1 => up(1),
        2 => up(2),
        3..=10 => up(3),
        _ => return None,
    })
}

/// The marker under the mouse on the player's own land for what the node takes (`0x19398`).
const fn site_item(size: SiteSize, harbour: bool) -> Option<usize> {
    Some(match size {
        SiteSize::Nothing => return None,
        SiteSize::Flag => item::SITE_FLAG,
        SiteSize::Mine => item::SITE_FLAG + 1,
        SiteSize::Small => item::SITE_FLAG + 2,
        SiteSize::Medium => item::SITE_FLAG + 3,
        SiteSize::Large if harbour => item::SITE_FLAG + 5,
        SiteSize::Large => item::SITE_FLAG + 4,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slope_table_sorts_rises_and_falls_into_three_steps() {
        let items: Vec<_> = (-11..=11).map(slope_item).collect();
        assert_eq!(items[0], None);
        assert_eq!(items[22], None);
        assert_eq!(&items[1..9], &[Some(66); 8]);
        assert_eq!(&items[9..14], &[Some(65), Some(64), Some(60), Some(61), Some(62)]);
        assert_eq!(&items[14..22], &[Some(63); 8]);
    }

    #[test]
    fn the_marker_under_the_mouse_shows_what_the_node_takes() {
        assert_eq!(site_item(SiteSize::Flag, false), Some(40));
        assert_eq!(site_item(SiteSize::Mine, false), Some(41));
        assert_eq!(site_item(SiteSize::Small, false), Some(42));
        assert_eq!(site_item(SiteSize::Medium, false), Some(43));
        assert_eq!(site_item(SiteSize::Large, false), Some(44));
        assert_eq!(site_item(SiteSize::Large, true), Some(45));
        assert_eq!(site_item(SiteSize::Nothing, true), None);
    }
}
