//! The catapult: picking a military building to throw at, the stone in flight and what its
//! landing does.

use crate::game::Game;
use crate::gather::{Find, Search};
use crate::grid::{Direction, Node};
use crate::world::{BuildingState, Category, Decoration, MapObject};

/// The catapult's stone slot.
const STONE_SLOT: usize = 0;

/// Search kind 10: occupied military buildings round the catapult, of any owner.
const TARGET_SEARCH: Search = Search {
    find: Find::Garrison,
    centre: false,
    radius: 12,
    hits: 5,
    same_area: false,
};

/// The hit roll is a draw from 0 to this; above [`HITS_UP_TO`] misses.
const HIT_ROLL: u16 = 100;
const HITS_UP_TO: u16 = 70;

/// A miss lands one step off in a direction drawn from 0 to this.
const MISS_ROLL: u16 = 5;

/// The catapult's throwing animation in `ROM_ANIM.DAT` for each way it faces, east first.
const THROWING: [u16; 6] = [3206, 3236, 3266, 3296, 3326, 3356];

/// Most stones in flight at once (`0x21f94`).
const MAX_STONES: usize = 50;

/// Steps a stone waits before it flies, flies, and lies after landing.
const WAIT: u8 = 22;
const FLIGHT: u8 = 48;
const LYING: u8 = 4;

/// What a miss leaves on empty ground (object word `0xc803`).
const SPENT_STONE: Decoration = Decoration { set: 0, index: 3 };

/// Where a thrown stone is (`0x200c6a`, 42 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Flight {
    /// About to fly.
    Waiting,
    /// In the air.
    Flying,
    /// Landed, shown breaking up.
    Landed,
}

/// A stone thrown by a catapult. `S2.EXE` keeps its positions in pixels for drawing; the sim
/// keeps the nodes and the way the catapult faced, from which they follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Stone {
    /// Where it is.
    pub flight: Flight,
    /// Steps into that part (`+0x1a`).
    pub counter: u8,
    /// The catapult's node.
    pub from: Node,
    /// The way the catapult faced.
    pub facing: Direction,
    /// Where it lands (`+0x22`, `+0x24`).
    pub to: Node,
}

impl crate::world::World {
    /// The stones in flight, by record.
    pub fn stones(&self) -> impl Iterator<Item = (u16, &Stone)> {
        (0..)
            .zip(&self.stones)
            .filter_map(|(record, stone)| stone.as_ref().map(|stone| (record, stone)))
    }

    /// Whether `node` holds a finished, occupied military building of any owner (`0x2e880`).
    pub(crate) fn garrison_at(&self, node: Node) -> Option<u16> {
        let record = self.object(node).building()?;
        let building = self.building(record)?;
        let military = self
            .building_types()
            .get(building.kind)
            .is_some_and(|row| row.category == Category::Military);
        (military
            && building
                .state
                .contains(BuildingState::FINISHED | BuildingState::MANNED))
        .then_some(record)
    }
}

impl Game {
    /// Catapult `record` starts a shot (`0x2e903`): with a stone in stock it picks one of the
    /// first five occupied military buildings within 12 rings, and gives up when the pick is
    /// its own owner's. Otherwise a draw from 0 to 100 above 70 misses, and a second draw moves
    /// a miss one step off. The catapult turns to the target, uses the stone and throws.
    pub(crate) fn start_catapult(&mut self, record: u16) -> bool {
        let Some(catapult) = self.world.building(record).copied() else {
            return false;
        };
        if catapult.stock[STONE_SLOT] == 0 {
            return false;
        }
        let hits = self.search(catapult.node, TARGET_SEARCH);
        let Some(target) = self.pick(&hits).and_then(|node| self.world.garrison_at(node)) else {
            return false;
        };
        let Some(target) = self.world.building(target).copied() else {
            return false;
        };
        let owner = |flag: u16| self.world.flag(flag).map(|flag| flag.owner);
        if owner(target.flag) == owner(catapult.flag) {
            return false;
        }
        let miss = self.rng.up_to(HIT_ROLL) > HITS_UP_TO;
        if let Some(catapult) = self.world.building_mut(record) {
            catapult.state |= BuildingState::WORKING;
        }
        let mut to = target.node;
        if miss {
            let way = usize::from(self.rng.up_to(MISS_ROLL));
            to = to.neighbour(Direction::ALL[way], self.nodes.width(), self.nodes.height());
        }
        let facing = facing(catapult.node, to);
        let animation = self.start_animation_at(THROWING[facing.index()], catapult.node);
        if let Some(catapult) = self.world.building_mut(record) {
            catapult.animation = animation;
            catapult.stock[STONE_SLOT] -= 1;
        }
        self.world
            .drop_ware(catapult.owner, crate::ware::WareKind::STONES, 1);
        let stone = Stone {
            flight: Flight::Waiting,
            counter: 0,
            from: catapult.node,
            facing,
            to,
        };
        match self.world.stones.iter().position(Option::is_none) {
            Some(free) => self.world.stones[free] = Some(stone),
            None if self.world.stones.len() < MAX_STONES => self.world.stones.push(Some(stone)),
            None => self.stone_hits(to),
        }
        true
    }

    /// Every stone in flight, in record order, after the ageing pass (`0x22004`): it waits 22
    /// steps, flies 48 and lands, then lies 4 before its record is freed.
    pub(crate) fn step_stones(&mut self) {
        for record in 0..self.world.stones.len() {
            let Some(mut stone) = self.world.stones[record] else {
                continue;
            };
            stone.counter += 1;
            match stone.flight {
                Flight::Waiting if stone.counter == WAIT => {
                    stone.flight = Flight::Flying;
                    stone.counter = 0;
                }
                Flight::Flying if stone.counter == FLIGHT => {
                    self.stone_lands(stone.to);
                    stone.flight = Flight::Landed;
                    stone.counter = 0;
                }
                Flight::Landed if stone.counter == LYING => {
                    self.world.stones[record] = None;
                    continue;
                }
                _ => {}
            }
            self.world.stones[record] = Some(stone);
        }
    }

    /// A stone lands on `node`: a building there takes the hit, and empty ground keeps the
    /// stone.
    fn stone_lands(&mut self, node: Node) {
        match self.world.object(node) {
            MapObject::Building(record) => {
                if let Some(hit) = self.world.building(record).copied()
                    && let Some(owner) = self.world.building_owner(record)
                {
                    self.post_message(
                        crate::messages::MessageId::CATAPULT_HIT,
                        owner,
                        Some(hit.node),
                        Some(hit.kind),
                        0,
                    );
                }
                self.stone_hits(node);
            }
            MapObject::Nothing => self.world.set_object(node, MapObject::Decoration(SPENT_STONE)),
            _ => {}
        }
    }

    /// A stone hits the building on `node` (`0x2ebc4`): an occupied military building, whoever
    /// owns it, loses its weakest soldier inside, and with nobody left inside it is demolished.
    fn stone_hits(&mut self, node: Node) {
        let Some(record) = self.world.garrison_at(node) else {
            return;
        };
        let Some((military, held)) = self.world.military_of(record).map(|(m, held)| (m, *held)) else {
            return;
        };
        if let Some(rank) = held
            .weakest()
            .and_then(|job| crate::job::Job::RANKS.iter().position(|&r| r == job))
            && let Some(held) = self.world.military.get_mut(military)
        {
            held.ranks[rank] -= 1;
            held.inside -= 1;
        }
        if self
            .world
            .military
            .get(military)
            .is_some_and(|held| held.inside == 0)
        {
            self.demolish(record);
        }
    }
}

/// The way a catapult on `from` faces to throw at `to` (`0x22df8`), from raw map coordinates
/// with no wrap: `dx` counts half-node columns, `dy` half-rows.
pub(crate) fn facing(from: Node, to: Node) -> Direction {
    let column = |node: Node| 2 * i32::from(node.x) + i32::from(node.y % 2);
    let dx = column(to) - column(from);
    let dy = 2 * (i32::from(to.y) - i32::from(from.y));
    if dx >= 0 {
        if dx < dy {
            Direction::SouthEast
        } else if -dx > dy {
            Direction::NorthEast
        } else {
            Direction::East
        }
    } else if -dx < dy {
        Direction::SouthWest
    } else if dx > dy {
        Direction::NorthWest
    } else {
        Direction::West
    }
}

#[cfg(test)]
mod tests;
