//! Harbour records: the seas a harbour touches, the demand linked to it by orders its own roads
//! cannot serve, and the supply pass that pulls stock to it.

use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::homeward::RING;
use crate::job::Job;
use crate::transport::Wanted;
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, MapObject, Player, WARE_TYPES, World};

/// The most harbour records (`0x25271`).
pub const MAX_HARBOURS: usize = 4000;

/// The highest a harbour's waiting count rises (`0x42a00`).
const MOST_WAITING: u8 = 250;

/// The tag of the person orders the supply pass places (`0x43467`).
const SUPPLY_TAG: u8 = 2;

/// Harbours a player needs before the supply pass looks at any of them.
const SUPPLY_HARBOURS: usize = 2;

/// A harbour record (259 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Harbour {
    /// The harbour building (`+0x00`).
    pub building: u16,
    /// Wares wanted through this harbour: open ware orders linked to it (`+0x02`).
    pub wares_wanted: [u16; WARE_TYPES],
    /// People wanted through this harbour: open person orders linked to it (`+0x40`).
    pub people_wanted: [u16; Job::PEOPLE],
    /// Wares this harbour has ordered in for that demand (`+0x7c`).
    pub wares_ordered: [u16; WARE_TYPES],
    /// People this harbour has ordered in for that demand (`+0xba`).
    pub people_ordered: [u16; Job::PEOPLE],
    /// Owner (`+0xf6`).
    pub owner: Player,
    /// The sea next to each landing node (`+0xf7`), north-west round to west.
    pub seas: [Option<u8>; 6],
    /// Passes of the harbour pass since a ship last loaded here or for here (`+0xfd`).
    pub waiting: u8,
    /// An expedition is being prepared (`+0xfe`).
    pub expedition: bool,
    /// Boards collected for the expedition (`+0x100`).
    pub boards: u8,
    /// Stones collected for the expedition (`+0x101`).
    pub stones: u8,
    /// Builders collected for the expedition (`+0x102`).
    pub builders: u8,
}

/// The direction from a harbour's node to its landing node `c` (`0x425b3`): north-west, north-east,
/// east, south-east, south-west, west.
fn landing_direction(c: usize) -> Direction {
    Direction::from_index(c + 4)
}

impl Harbour {
    /// Whether one of its landing nodes lies on sea `sea` (`0x4295e`).
    #[must_use]
    pub fn touches(&self, sea: u8) -> bool {
        self.seas.contains(&Some(sea))
    }

    /// Whether it shares a sea with `other` (`0x43194`).
    #[must_use]
    pub fn shares_sea(&self, other: &Self) -> bool {
        self.seas.iter().flatten().any(|&sea| other.touches(sea))
    }
}

impl World {
    /// Harbour record `record`.
    #[must_use]
    pub fn harbour(&self, record: u16) -> Option<&Harbour> {
        self.harbours.get(record)
    }

    /// The harbour records in use, in record order, with their numbers.
    pub fn harbours(&self) -> impl Iterator<Item = (u16, &Harbour)> {
        self.harbours.iter()
    }

    /// The harbour record of building `building` (`+0x12`).
    #[must_use]
    pub fn harbour_of(&self, building: u16) -> Option<u16> {
        self.harbours
            .iter()
            .find(|(_, harbour)| harbour.building == building)
            .map(|(record, _)| record)
    }

    /// The harbour record of the finished harbour standing at flag `flag`.
    pub(crate) fn harbour_at_flag(&self, flag: u16) -> Option<u16> {
        let building = self.building_at_flag(flag)?;
        let record = self.building(building)?;
        if record.kind != BuildingKind::HARBOUR || !record.state.contains(BuildingState::FINISHED) {
            return None;
        }
        self.harbour_of(building)
    }

    /// The node a ship on sea `sea` sails to at harbour `record` (`0x428b1`): the first landing
    /// node on that sea with no flag, or with a flag on every one, the last on that sea.
    #[must_use]
    pub fn landing(&self, record: u16, sea: u8) -> Option<Node> {
        let harbour = self.harbours.get(record)?;
        let node = self.building(harbour.building)?.node;
        let mut last = None;
        for (c, &entry) in harbour.seas.iter().enumerate() {
            if entry != Some(sea) {
                continue;
            }
            let at = node.neighbour(landing_direction(c), self.width(), self.height());
            if !matches!(self.object(at), MapObject::Flag(_)) {
                return Some(at);
            }
            last = Some(at);
        }
        last
    }

    /// The first harbour of `owner` in record order that touches sea `sea` (`0x42994`).
    pub(crate) fn first_harbour(&self, owner: Player, sea: u8) -> Option<u16> {
        self.harbours
            .iter()
            .find(|(_, harbour)| harbour.owner == owner && harbour.touches(sea))
            .map(|(record, _)| record)
    }

    /// The flag an order that failed at harbour `record`'s flag moves to (`0x431f2`): that of
    /// the next harbour in record order, wrapping round, with the same owner and a sea in
    /// common; with none, the harbour's own flag.
    pub(crate) fn next_harbour_flag(&self, record: u16) -> Option<u16> {
        let harbour = self.harbours.get(record)?;
        let slots = u16::try_from(self.harbours.slots()).unwrap_or(u16::MAX);
        let next = (1..slots)
            .map(|offset| (record + offset) % slots)
            .find(|&other| {
                self.harbours
                    .get(other)
                    .is_some_and(|other| other.owner == harbour.owner && harbour.shares_sea(other))
            })
            .unwrap_or(record);
        let building = self.harbours.get(next)?.building;
        Some(self.building(building)?.flag)
    }

    /// Order `record`'s harbour link is released (`0x427d5`, `0x4282d`): the harbour wants one
    /// fewer of the item and counts one fewer ordered in, neither going below 0, for all five
    /// ranks when the item is a soldier.
    pub(crate) fn release_link(&mut self, record: u16) {
        let Some(order) = self.orders.get_mut(record) else {
            return;
        };
        let (Some(link), wanted) = (order.harbour.take(), order.wanted) else {
            return;
        };
        let Some(harbour) = self.harbours.get_mut(link) else {
            return;
        };
        let lower = |count: &mut u16| *count = count.saturating_sub(1);
        match wanted {
            Wanted::Ware(ware) => {
                if let Some(count) = harbour.wares_wanted.get_mut(ware.index()) {
                    lower(count);
                }
                if let Some(count) = harbour.wares_ordered.get_mut(ware.index()) {
                    lower(count);
                }
            }
            Wanted::Person(job) => {
                for job in linked_jobs(job) {
                    if let Some(count) = harbour.people_wanted.get_mut(job.index()) {
                        lower(count);
                    }
                    if let Some(count) = harbour.people_ordered.get_mut(job.index()) {
                        lower(count);
                    }
                }
            }
        }
    }

    /// Order `record`, which no storehouse on its roads could serve, is linked to harbour
    /// `harbour` and raises its demand (`0x4277b`, `0x42795`), for all five ranks when it wants a
    /// soldier.
    pub(crate) fn link_to_harbour(&mut self, record: u16, harbour: u16) {
        let Some(order) = self.orders.get_mut(record) else {
            return;
        };
        order.harbour = Some(harbour);
        let wanted = order.wanted;
        let Some(harbour) = self.harbours.get_mut(harbour) else {
            return;
        };
        let raise = |count: &mut u16| *count = count.wrapping_add(1);
        match wanted {
            Wanted::Ware(ware) => {
                if let Some(count) = harbour.wares_wanted.get_mut(ware.index()) {
                    raise(count);
                }
            }
            Wanted::Person(job) => {
                for job in linked_jobs(job) {
                    if let Some(count) = harbour.people_wanted.get_mut(job.index()) {
                        raise(count);
                    }
                }
            }
        }
    }

    /// Every order but those being delivered loses its link to harbour `record`, which is going
    /// (`0x43141`). Its counts go with the record.
    pub(crate) fn unlink_harbour(&mut self, record: u16) {
        for (_, order) in self.orders.iter_mut() {
            if !order.delivering && order.harbour == Some(record) {
                order.harbour = None;
            }
        }
    }
}

/// The jobs an order for `job` counts against at a harbour: all five ranks for a soldier.
fn linked_jobs(job: Job) -> Vec<Job> {
    if job.is_soldier() {
        Job::RANKS.to_vec()
    } else {
        vec![job]
    }
}

impl Game {
    /// Harbour `building` is finished (`0x425b3`): it gets a harbour record with no demand,
    /// noting for each landing node the sea it touches. Each landing node's neighbours are looked
    /// at north-east round to north-west, and the last one in a body of water gives the sea.
    pub(crate) fn add_harbour(&mut self, building: u16) {
        let Some(record) = self.world.building(building).copied() else {
            return;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let seas = std::array::from_fn(|c| {
            let landing = record.node.neighbour(landing_direction(c), width, height);
            RING.into_iter()
                .map(|direction| self.nodes.get(landing.neighbour(direction, width, height)).area)
                .rfind(|&area| self.nodes.is_sea(area))
        });
        self.world.harbours.insert(
            Harbour {
                building,
                wares_wanted: [0; WARE_TYPES],
                people_wanted: [0; Job::PEOPLE],
                wares_ordered: [0; WARE_TYPES],
                people_ordered: [0; Job::PEOPLE],
                owner: record.owner,
                seas,
                waiting: 0,
                expedition: false,
                boards: 0,
                stones: 0,
                builders: 0,
            },
            MAX_HARBOURS,
        );
    }

    /// The harbour pass (`0x42a00`), every 100 steps: the supply pass, then every harbour waits
    /// one pass longer, up to 250.
    pub(crate) fn harbour_pass(&mut self) {
        self.supply_harbours();
        for (_, harbour) in self.world.harbours.iter_mut() {
            if harbour.waiting < MOST_WAITING {
                harbour.waiting += 1;
            }
        }
    }

    /// The supply pass (`0x43285`): each harbour of a player with at least two finished harbours
    /// orders one of each ware and job it wants more of than it has ordered and that no other
    /// harbour of its owner on a shared sea holds. Wares are ordered first, then people.
    fn supply_harbours(&mut self) {
        let records: Vec<u16> = self.world.harbours().map(|(record, _)| record).collect();
        for record in records {
            let Some(harbour) = self.world.harbours.get(record).copied() else {
                continue;
            };
            let Some(owner) = self.world.building_owner(harbour.building) else {
                continue;
            };
            if self.finished_harbours(owner) < SUPPLY_HARBOURS {
                continue;
            }
            let mut wares = [false; WARE_TYPES];
            let mut people = [false; Job::PEOPLE];
            for (other, partner) in self.world.harbours() {
                if other == record || partner.owner != harbour.owner || !harbour.shares_sea(partner) {
                    continue;
                }
                let Some((_, store)) = self.world.storehouse_of(partner.building) else {
                    continue;
                };
                for (held, &count) in wares.iter_mut().zip(&store.wares) {
                    *held |= count > 0;
                }
                for (held, &count) in people.iter_mut().zip(&store.jobs) {
                    *held |= count > 0;
                }
            }
            let Some(flag) = self
                .world
                .building(harbour.building)
                .map(|building| building.flag)
            else {
                continue;
            };
            for (kind, held) in (0..).map(WareKind).zip(wares) {
                let Some(open) = self.world.harbours.get_mut(record) else {
                    break;
                };
                let ware = kind.index();
                if held || open.wares_wanted[ware] <= open.wares_ordered[ware] {
                    continue;
                }
                open.wares_ordered[ware] += 1;
                self.world.order_ware(flag, kind, 0);
            }
            for (job, held) in (0..).map(Job).zip(people) {
                let Some(open) = self.world.harbours.get_mut(record) else {
                    break;
                };
                let index = job.index();
                if held || open.people_wanted[index] <= open.people_ordered[index] {
                    continue;
                }
                open.people_ordered[index] += 1;
                self.world.order_person(flag, job, 0, SUPPLY_TAG);
            }
        }
    }

    /// How many finished harbours `owner` has (player block `+0x6e`, inferred to count finished
    /// buildings as the woodcutter count does).
    fn finished_harbours(&self, owner: Player) -> usize {
        self.world
            .buildings()
            .filter(|(_, building)| {
                building.owner == owner
                    && building.kind == BuildingKind::HARBOUR
                    && building.state.contains(BuildingState::FINISHED)
            })
            .count()
    }
}

#[cfg(test)]
pub(crate) mod tests;
