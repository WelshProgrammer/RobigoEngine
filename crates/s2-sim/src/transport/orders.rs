//! Transport orders: one ware or one person wanted at a flag. Every 16 steps the orders age and the
//! twenty most urgent are served.

use std::hash::{Hash, Hasher};

use crate::job::Job;
use crate::ware::WareKind;
use crate::world::{Player, World};

/// The most order records; their numbers are 16-bit with `u16::MAX` meaning none. The pool's real
/// limit comes from the game's set-up and has not been read.
pub const MAX_ORDERS: usize = 65_535;

/// The mode road carrier orders carry (`0x25b00`).
pub const ROAD_MODE: u8 = 3;

/// The most urgent an order gets.
pub const MOST_URGENT: u8 = 100;

/// How many orders each pass serves.
pub const CHOSEN: usize = 20;

/// Starting urgency of an order for a helper or a pack donkey.
const CARRIER_URGENCY: u8 = 70;

/// Starting urgency of an order for anyone else.
const PERSON_URGENCY: u8 = 50;

/// The mode byte of a ware order.
pub const WARE_MODE: u8 = u8::MAX;

/// A player's construction weight for boards and stones (settings `+0x3e0`) at its default. The
/// player's own settings are not modelled yet.
pub const CONSTRUCTION_WEIGHT: u8 = 100;

/// Urgency of a ware ordered to a flag with no building.
pub const NO_BUILDING_URGENCY: u8 = 10;

/// An order's state as byte `+0x03` holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum OrderKind {
    /// 1: a ware.
    Ware,
    /// 2: a person.
    Person,
    /// 3: what was wanted is on its way.
    Delivering,
}

/// What an order wants (`+0x05`, with byte `+0x03` saying which).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Wanted {
    /// A ware of this type.
    Ware(WareKind),
    /// A person with this job.
    Person(Job),
}

/// A transport order: 12 bytes in `S2.EXE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Order {
    /// Where the item is wanted (`+0x00`).
    pub flag: u16,
    /// Urgency, 0 to [`MOST_URGENT`] (`+0x02`).
    pub urgency: u8,
    /// What is wanted.
    pub wanted: Wanted,
    /// What was wanted is on its way (`+0x03` = 3).
    pub delivering: bool,
    /// For a person, a mode given by whoever placed the order (`+0x04`), [`ROAD_MODE`] for a
    /// road's carrier; [`WARE_MODE`] for a ware.
    pub mode: u8,
    /// The ware or figure on its way (`+0x06`).
    pub on_way: Option<u16>,
    /// A harbour link (`+0x08`, inferred).
    pub harbour: Option<u16>,
    /// A byte from whoever placed the order (`+0x0a`).
    pub tag: u8,
    /// Owner of the flag (`+0x0b`).
    pub owner: Player,
}

impl Order {
    /// Its state as byte `+0x03` holds it.
    #[must_use]
    pub const fn kind(&self) -> OrderKind {
        match self.wanted {
            _ if self.delivering => OrderKind::Delivering,
            Wanted::Ware(_) => OrderKind::Ware,
            Wanted::Person(_) => OrderKind::Person,
        }
    }

    /// The ware type wanted, if a ware is.
    #[must_use]
    pub const fn ware(&self) -> Option<WareKind> {
        match self.wanted {
            Wanted::Ware(ware) => Some(ware),
            Wanted::Person(_) => None,
        }
    }

    /// The job wanted, if a person is.
    #[must_use]
    pub const fn job(&self) -> Option<Job> {
        match self.wanted {
            Wanted::Person(job) => Some(job),
            Wanted::Ware(_) => None,
        }
    }
}

impl Hash for Order {
    /// Hashes the record's fields as `S2.EXE` lays them out, so state hashes match those of the
    /// packed form.
    fn hash<H: Hasher>(&self, state: &mut H) {
        let item = match self.wanted {
            Wanted::Ware(ware) => ware.0,
            Wanted::Person(job) => job.0,
        };
        self.flag.hash(state);
        self.urgency.hash(state);
        self.kind().hash(state);
        self.mode.hash(state);
        item.hash(state);
        self.on_way.hash(state);
        self.harbour.hash(state);
        self.tag.hash(state);
        self.owner.hash(state);
    }
}

impl World {
    /// Order record `record`, if it is in use.
    #[must_use]
    pub fn order(&self, record: u16) -> Option<&Order> {
        self.orders.get(record)
    }

    /// The order records in use, in record order, with their numbers.
    pub fn orders(&self) -> impl Iterator<Item = (u16, &Order)> {
        self.orders.iter()
    }

    /// Orders a person with job `job` to flag `flag` (`0x2aefc`) and returns the order. Helpers
    /// and pack donkeys start at urgency 70, other jobs at 50. `None` when the flag is gone or
    /// the pool is full.
    pub fn order_person(&mut self, flag: u16, job: Job, mode: u8, tag: u8) -> Option<u16> {
        let owner = self.flag(flag)?.owner;
        let urgency = if job == Job::HELPER || job == Job::PACK_DONKEY {
            CARRIER_URGENCY
        } else {
            PERSON_URGENCY
        };
        self.orders.insert(
            Order {
                flag,
                urgency,
                wanted: Wanted::Person(job),
                delivering: false,
                mode,
                on_way: None,
                harbour: None,
                tag,
                owner,
            },
            MAX_ORDERS,
        )
    }

    /// Orders a ware of type `ware` to flag `flag` (`0x2afa0`) and returns the order, at the
    /// urgency [`World::order_urgency`] gives it. Military buildings' own urgency for coins is
    /// not modelled yet.
    pub fn order_ware(&mut self, flag: u16, ware: WareKind, tag: u8) -> Option<u16> {
        let owner = self.flag(flag)?.owner;
        let urgency = self.order_urgency(flag, ware)?;
        self.orders.insert(
            Order {
                flag,
                urgency,
                wanted: Wanted::Ware(ware),
                delivering: false,
                mode: WARE_MODE,
                on_way: None,
                harbour: None,
                tag,
                owner,
            },
            MAX_ORDERS,
        )
    }

    /// Ages every order not yet being delivered by one, up to [`MOST_URGENT`], and returns the
    /// [`CHOSEN`] most urgent in the order they are to be served (`0x2b2d0`). The first twenty
    /// orders in record order are taken as they come. After that an order replaces the least
    /// urgent chosen one, the first of equals, only when it is strictly more urgent.
    pub fn choose_orders(&mut self) -> Vec<u16> {
        let mut chosen: Vec<(u16, u8)> = Vec::with_capacity(CHOSEN);
        let mut least = (0, 0_u8);
        for (record, order) in self.orders.iter_mut() {
            if order.delivering {
                continue;
            }
            order.urgency = order.urgency.saturating_add(1).min(MOST_URGENT);
            let urgency = order.urgency;
            if chosen.len() < CHOSEN {
                chosen.push((record, urgency));
            } else if urgency > least.1 {
                chosen[least.0] = (record, urgency);
            } else {
                continue;
            }
            if chosen.len() == CHOSEN {
                least = chosen
                    .iter()
                    .enumerate()
                    .fold((0, u8::MAX), |least, (slot, &(_, urgency))| {
                        if urgency < least.1 { (slot, urgency) } else { least }
                    });
            }
        }
        chosen.into_iter().map(|(record, _)| record).collect()
    }
}

#[cfg(test)]
mod tests;
