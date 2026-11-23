//! Task 10 keeps a computer player's flags (`0x6f43a`): every third call it looks at its next
//! flag, takes down a workplace with nothing left to work, a lone flag, and asks for a shorter
//! road where the roads go round. Task 3 lays such a road between two flags (`0x6b93b`).

use super::ComputerPlayer;
use super::board::FOOD;
use super::chain::AROUND;
use super::expansion::Visit;
use super::jobs::{Job, JobFlags, JobKind, Payload};
use super::profiles::RoadRating;
use super::tasks::Think;
use crate::grid::{Direction, Node};
use crate::nodes::SiteFlags;
use crate::world::{BuildingKind, BuildingState, Category, FlagKind, Tree};
use crate::{SearchOptions, search};

/// Task 10 looks at a flag every this many calls (`0x6f46b`).
const EVERY: u16 = 3;

/// The radius of the build order that replaces a mine with nothing left (`0x6f5e0`).
const MINE_AGAIN: u16 = 15;

/// The chance in hundredths, against a draw of 0 to 100 taken as more than, that a fishery with
/// nothing left asks for food (`0x6f62d`).
const FOOD_CHANCE: u16 = 50;

/// A woodcutter at this productivity or below, with no tree near, is taken down (`0x6f66c`).
const IDLE_WOODCUTTER: u8 = 30;

/// The rings round a woodcutter searched for a tree (`0x6f695`) and for a forester (`0x6f6c0`).
const TREE_RINGS: u16 = 6;
const FORESTER_RINGS: u16 = 8;

/// Task 10's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FlagKeeper {
    /// Calls since the last look (`+0x06`).
    pub calls: u16,
    /// The flag looked at last, `None` when the player had none (`+0x08`).
    pub cursor: Option<u16>,
}

impl Default for FlagKeeper {
    fn default() -> Self {
        Self {
            calls: 0,
            cursor: Some(0),
        }
    }
}

/// The best flag a road search from the looked-at flag met, as task 10 scores it.
#[derive(Default)]
struct Shortcut {
    flag: Option<u16>,
    value: u16,
    radius: u16,
}

impl Think<'_> {
    /// The player's next flag after record `cursor`, round to the start, `cursor` itself last
    /// (`0x28448`). `None` starts at record 0.
    fn next_flag(&self, cursor: Option<u16>) -> Option<u16> {
        let start = cursor.map_or(0, |at| at.wrapping_add(1));
        let records: Vec<u16> = self
            .game
            .world
            .flags()
            .filter(|(_, flag)| flag.owner == self.player)
            .map(|(record, _)| record)
            .collect();
        records
            .iter()
            .copied()
            .find(|&record| record >= start)
            .or_else(|| records.first().copied())
    }

    /// Whether the building at flag `flag` is a storehouse, harbour or headquarters, finished or
    /// not (`0x28090`).
    fn storehouse_at(&self, flag: u16) -> bool {
        self.game
            .world
            .building_at_flag(flag)
            .and_then(|record| self.game.world.building(record))
            .and_then(|building| self.game.world.building_types().get(building.kind))
            .is_some_and(|row| row.category == Category::Storehouse)
    }

    /// Grown, standing trees on rings 1 to `reach` round `node` (`0x68bf1`), of every species,
    /// those a woodcutter leaves standing included.
    fn trees_near(&self, node: Node, reach: u16) -> usize {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        node.rings(reach, width, height)
            .skip(1)
            .filter(|&at| self.game.world.object(at).tree().is_some_and(Tree::grown))
            .count()
    }

    /// A road search from `flag` (`0x3b884`) rating each flag met (`0x6f319`, `0x6f3d5`): one
    /// that costs at least the floor, whose cost over the divisor is at least its straight
    /// distance, is worth that less the distance, and the most worth wins, the first on a tie,
    /// with a radius of half again its distance. With `served`, it also notes whether any flag
    /// met has a storehouse.
    fn rate_flags(
        &self,
        flag: u16,
        node: Node,
        rating: RoadRating,
        mut served: Option<&mut bool>,
    ) -> Shortcut {
        let mut best = Shortcut::default();
        search(
            &self.game.world,
            flag,
            rating.limit,
            SearchOptions::empty(),
            |reached| {
                if let Some(served) = served.as_deref_mut()
                    && !*served
                    && self.storehouse_at(reached.flag)
                {
                    *served = true;
                }
                if reached.cost < rating.floor {
                    return false;
                }
                let Some(at) = self.game.world.flag(reached.flag).map(|met| met.node) else {
                    return false;
                };
                let distance = self.distance(at, node);
                let worth = u32::from(reached.cost / rating.divisor.max(1));
                if distance > worth {
                    return false;
                }
                let value = u16::try_from(worth - distance).unwrap_or(u16::MAX);
                if value > best.value {
                    let distance = u16::try_from(distance).unwrap_or(u16::MAX);
                    best = Shortcut {
                        flag: Some(reached.flag),
                        value,
                        radius: distance + distance / 2,
                    };
                }
                false
            },
        );
        best
    }

    /// Lays a road from flag `from` to flag `to` (`0x6b5eb`): both in use and on the player's
    /// land, a flood in the same area from `from` of `radius − 1` rings over the player's flags
    /// and the player's nodes that take roads, until it meets `to`. The walk then goes from `to`
    /// back up the flood, closing a road at each flag it meets and putting a flag down once enough
    /// nodes are pending and one may stand, as task 2's commit does, and ends with a road to
    /// `from` from the node next to it. Anything built before a failure stays.
    fn shortcut(&mut self, from: u16, to: u16, radius: u16) {
        let (Some(start), Some(end)) = (
            self.game.world.flag(from).map(|flag| flag.node),
            self.game.world.flag(to).map(|flag| flag.node),
        ) else {
            return;
        };
        let owner = Some(self.player);
        if self.game.nodes.get(start).owner != owner || self.game.nodes.get(end).owner != owner {
            return;
        }
        let mut met = false;
        let depth = u8::try_from(radius & u16::from(u8::MAX)).unwrap_or(u8::MAX);
        let marks = self.survey(start, depth, true, |node, _| {
            if node == end {
                met = true;
                return Visit::Stop;
            }
            let data = self.game.nodes.get(node);
            if data.owner != owner {
                Visit::Block
            } else if self.game.world.flag_at(node).is_some()
                || !data.sites.flags.contains(SiteFlags::NO_ROAD)
            {
                Visit::Pass
            } else {
                Visit::Block
            }
        });
        if !met {
            return;
        }
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let mark = |node: Node| marks[usize::from(node.y) * usize::from(width) + usize::from(node.x)];
        let spacing = usize::from(self.profile.placing.flag_spacing.saturating_sub(2));
        let mut at = end;
        let mut current = to;
        let mut pending: Vec<Node> = Vec::new();
        let mut wanted = mark(at).wrapping_add(1);
        loop {
            let Some(next) = AROUND
                .iter()
                .map(|&way| at.neighbour(way, width, height))
                .find(|&node| mark(node) == wanted)
            else {
                return;
            };
            at = next;
            if let Some(flag) = self.game.world.flag_at(at) {
                if !self.road(current, flag, &mut pending) {
                    return;
                }
                current = flag;
            } else if spacing >= pending.len()
                || self.game.nodes.get(at).sites.flags.contains(SiteFlags::NO_FLAG)
            {
                pending.push(at);
            } else if let Some(flag) =
                self.game
                    .world
                    .place_flag(&mut self.game.nodes, at, FlagKind::Normal, true)
            {
                if !self.road(current, flag, &mut pending) {
                    return;
                }
                current = flag;
            } else {
                pending.push(at);
            }
            wanted = wanted.wrapping_add(1);
            if u16::from(wanted) == radius {
                break;
            }
        }
        self.road(current, from, &mut pending);
    }
}

impl ComputerPlayer {
    /// Task 10's call (`0x6f43a`).
    pub(super) fn keep_flags(&mut self, think: &mut Think<'_>) {
        let keeper = &mut self.tasks.flag_keeper;
        keeper.calls += 1;
        if keeper.calls < EVERY {
            return;
        }
        keeper.calls = 0;
        let cursor = think.next_flag(keeper.cursor);
        keeper.cursor = cursor;
        let Some(flag) = cursor else {
            return;
        };
        let Some(node) = think.game.world.flag(flag).map(|flag| flag.node) else {
            return;
        };
        let building = think
            .game
            .world
            .building_at_flag(flag)
            .and_then(|record| Some((record, *think.game.world.building(record)?)));
        match building {
            Some((record, building))
                if building
                    .state
                    .contains(BuildingState::FINISHED | BuildingState::MANNED) =>
            {
                if exhausted(building.kind, building.exhausted) {
                    Self::take_down(think, record, building.node);
                    if matches!(
                        building.kind,
                        BuildingKind::GOLD_MINE
                            | BuildingKind::IRON_MINE
                            | BuildingKind::COAL_MINE
                            | BuildingKind::GRANITE_MINE
                    ) {
                        self.add_order(think, (building.kind, building.node, MINE_AGAIN), None, None);
                    }
                    if building.kind == BuildingKind::FISHERY && think.game.rng.up_to(100) > FOOD_CHANCE {
                        let at = (building.node.x.cast_signed(), building.node.y.cast_signed());
                        self.tasks.board.add(FOOD, at, think.profile.add_wait, false);
                    }
                    return;
                }
                if building.kind == BuildingKind::WOODCUTTER
                    && building.productivity <= IDLE_WOODCUTTER
                    && think.trees_near(building.node, TREE_RINGS) == 0
                    && think.count_near(
                        think.player,
                        Some(BuildingKind::FORESTER),
                        building.node,
                        FORESTER_RINGS,
                    ) == 0
                {
                    Self::take_down(think, record, building.node);
                    return;
                }
            }
            None => {
                let roads = think
                    .game
                    .world
                    .flag(flag)
                    .map_or(0, |flag| flag.links.iter().flatten().count());
                if roads <= 1 {
                    think.game.remove_flag_record(flag);
                    return;
                }
            }
            Some(_) => {}
        }
        self.road_check(think, flag, node);
    }

    /// Demolishes building `record` on `node` and removes the flag at its door (`0x2649a`,
    /// `0x25763`).
    fn take_down(think: &mut Think<'_>, record: u16, node: Node) {
        think.game.demolish(record);
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        let door = node.neighbour(Direction::SouthEast, width, height);
        if let Some(flag) = think.game.world.flag_at(door) {
            think.game.remove_flag_record(flag);
        }
    }

    /// The road check (`0x6f77a`): a road search from flag `flag` rates the flags it meets. From a
    /// storehouse's flag the best gets a road from here. From any other, a search that met no
    /// storehouse first sends a road job joining the flag to the roads, and then the best gets a
    /// road.
    fn road_check(&mut self, think: &Think<'_>, flag: u16, node: Node) {
        let profile = think.profile.flag_keeper;
        if think.storehouse_at(flag) {
            let best = think.rate_flags(flag, node, profile.storehouse, None);
            if let Some(to) = best.flag {
                self.add(Job::shortcut(flag, to, best.radius, think.step), None, None);
            }
            return;
        }
        let mut served = false;
        let best = think.rate_flags(flag, node, profile.other, Some(&mut served));
        if !served {
            let join = Job::with(
                JobKind::Connect,
                Payload::Connect {
                    flag: node,
                    mode: 1,
                    kind: BuildingKind::WOODCUTTER,
                },
                think.step,
            );
            self.add(join, None, None);
        }
        if let Some(to) = best.flag {
            self.add(Job::shortcut(flag, to, best.radius, think.step), None, None);
        }
    }

    /// Task 3's turn (`0x6b93b`): the road is laid and the job reports 0 whatever came of it, then
    /// ends.
    pub(super) fn shortcut_turn(&mut self, index: u16, think: &mut Think<'_>) -> u16 {
        self.rest(index);
        self.set(index, JobFlags::FINISHED);
        if let Some(Payload::Shortcut { from, to, radius }) = self.job(index).map(|job| job.payload) {
            think.shortcut(from, to, radius);
        }
        self.report(index, 0, 0, think);
        self.end(index);
        0
    }
}

/// Whether a building of `kind` with its nothing-left flag `flag` has nothing left to work
/// (`0x30ca3`): only quarries, fisheries, mines and wells keep the flag.
fn exhausted(kind: BuildingKind, flag: bool) -> bool {
    flag && matches!(
        kind,
        BuildingKind::QUARRY
            | BuildingKind::FISHERY
            | BuildingKind::GOLD_MINE
            | BuildingKind::IRON_MINE
            | BuildingKind::COAL_MINE
            | BuildingKind::GRANITE_MINE
            | BuildingKind::WELL
    )
}

#[cfg(test)]
mod tests;
