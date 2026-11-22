//! Wares waiting at flags and carried along roads.

use crate::supply::construction_index;
use crate::transport::{SearchOptions, next_flag};
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, FLAG_CAPACITY, Player, WARE_TYPES, World};

/// Most ware records (`0x1fa500`).
pub const MAX_WARES: usize = 10_000;

/// How far a ware's next flag is searched for.
const ROUTE_LIMIT: u16 = 250;

/// What each ware type looks like on a carrier and which transport group it is in: bytes 0 and 4
/// of the ware table at `0x1fa8e0`.
const WARE_TABLE: [(u8, u8); WARE_TYPES] = [
    (22, 0),
    (23, 1),
    (24, 2),
    (33, 3),
    (27, 3),
    (18, 3),
    (19, 3),
    (32, 3),
    (20, 3),
    (11, 4),
    (0, 5),
    (31, 6),
    (30, 7),
    (29, 8),
    (17, 9),
    (28, 10),
    (1, 11),
    (3, 11),
    (4, 11),
    (5, 11),
    (2, 11),
    (6, 11),
    (7, 11),
    (8, 11),
    (9, 11),
    (12, 11),
    (13, 11),
    (14, 11),
    (16, 12),
    (21, 12),
    (15, 13),
];

pub use s2_systems::ware::{TRANSPORT_GROUP_NAMES, TRANSPORT_GROUPS};

/// What a ware is doing (`+0x01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WareState {
    /// Waiting at a flag with a route (4).
    Waiting,
    /// On a carrier (5).
    Carried,
    /// At a flag with no destination or no route (6).
    Stranded,
    /// Waiting, with a carrier on its way to fetch it (7).
    Taken,
    /// Made without a destination and not yet put down (8).
    Unrouted,
}

/// A ware record (9 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Ware {
    /// Its type (`+0x00`).
    pub kind: WareKind,
    /// What it is doing (`+0x01`).
    pub state: WareState,
    /// Its place 0 to 7 at its flag, which only decides where it is drawn (`+0x02`).
    pub slot: u8,
    /// Waiting: the next flag on its way. Stranded: the flag it is at (`+0x03`).
    pub flag: Option<u16>,
    /// Its transport order, whose flag is its destination (`+0x05`).
    pub order: Option<u16>,
    /// At a flag: the next ware waiting there. Carried: the carrier (`+0x07`).
    pub link: Option<u16>,
}

impl Ware {
    /// A new ware of type `kind` with no order, before it is put down.
    #[must_use]
    pub const fn new(kind: WareKind) -> Self {
        Self {
            kind,
            state: WareState::Unrouted,
            slot: 0,
            flag: None,
            order: None,
            link: None,
        }
    }

    /// The bitmap a carrier shows for it, also the item of `CARRIER.BOB`.
    #[must_use]
    pub fn bitmap(&self) -> Option<u8> {
        WARE_TABLE.get(self.kind.index()).map(|&(bitmap, _)| bitmap)
    }
}

/// The transport group of ware type `ware` (byte 4 of its row).
pub(crate) fn transport_group(ware: WareKind) -> Option<u8> {
    WARE_TABLE.get(ware.index()).map(|&(_, group)| group)
}

/// Every ware type in transport list order for a player whose groups have ranks `ranks`: the
/// group ranked 0 first (`0x3d0f0`), each group's wares in type order.
pub(crate) fn wares_by_rank(ranks: &[u8; TRANSPORT_GROUPS]) -> impl Iterator<Item = WareKind> + '_ {
    (0..)
        .take(TRANSPORT_GROUPS)
        .filter_map(|rank| ranks.iter().position(|&r| r == rank))
        .flat_map(|group| {
            WARE_TABLE
                .iter()
                .enumerate()
                .filter(move |(_, (_, of))| usize::from(*of) == group)
                .filter_map(|(ware, _)| u8::try_from(ware).ok().map(WareKind))
        })
}

impl World {
    /// Ware record `record`, if it is in use.
    #[must_use]
    pub fn ware(&self, record: u16) -> Option<&Ware> {
        self.wares.get(record)
    }

    /// The ware records in use, in record order.
    pub fn wares(&self) -> impl Iterator<Item = (u16, &Ware)> {
        self.wares.iter()
    }

    /// The wares waiting at flag `flag`, newest first.
    pub fn wares_at(&self, flag: u16) -> impl Iterator<Item = (u16, &Ware)> {
        let mut next = self.flag(flag).and_then(|flag| flag.wares);
        std::iter::from_fn(move || {
            let record = next?;
            let ware = self.wares.get(record)?;
            next = ware.link;
            Some((record, ware))
        })
    }

    /// Makes a ware record for `ware` in the lowest free slot (`0x2a28c`).
    pub(crate) fn add_ware(&mut self, ware: Ware) -> Option<u16> {
        self.wares.insert(ware, MAX_WARES)
    }

    /// Where ware `record` is going: the flag of its order (`0x2a616`). A stranded or unrouted
    /// ware is going nowhere, whatever its order field still says.
    #[must_use]
    pub fn destination(&self, record: u16) -> Option<u16> {
        let ware = self.wares.get(record)?;
        if matches!(ware.state, WareState::Stranded | WareState::Unrouted) {
            return None;
        }
        Some(self.orders.get(ware.order?)?.flag)
    }

    /// Puts ware `record` down at flag `flag` (`0x2a3fd`). Returns false when the flag is full:
    /// the ware keeps the state it had and the caller keeps the ware.
    ///
    /// A ware made without a destination becomes stranded here. Any other ware that is not
    /// stranded waits, and is handed over at once if this is its destination. Otherwise it goes to
    /// the head of the flag's list in the lowest free place, and a ware with somewhere to go
    /// looks up its next flag, or is stranded here when there is no route.
    pub(crate) fn put_down(&mut self, record: u16, flag: u16) -> bool {
        let Some(ware) = self.wares.get_mut(record) else {
            return false;
        };
        let original = ware.state;
        match ware.state {
            WareState::Unrouted => {
                ware.state = WareState::Stranded;
                ware.flag = Some(flag);
            }
            WareState::Stranded => {}
            _ => ware.state = WareState::Waiting,
        }
        let destination = self.destination(record);
        if destination == Some(flag) {
            self.hand_over(record, flag);
            return true;
        }
        let Some(head) = self.flag(flag).map(|flag| (flag.waiting, flag.wares)) else {
            return false;
        };
        if head.0 >= FLAG_CAPACITY {
            if let Some(ware) = self.wares.get_mut(record) {
                ware.state = original;
            }
            return false;
        }
        let taken: Vec<u8> = self.wares_at(flag).map(|(_, ware)| ware.slot).collect();
        let slot = (0..FLAG_CAPACITY).find(|slot| !taken.contains(slot)).unwrap_or(0);
        let Some(ware) = self.wares.get_mut(record) else {
            return false;
        };
        ware.slot = slot;
        ware.link = head.1;
        let stranded = ware.state == WareState::Stranded;
        if let Some(flag) = self.flag_mut(flag) {
            flag.wares = Some(record);
            flag.waiting += 1;
        }
        if stranded {
            return true;
        }
        let next = destination.and_then(|to| {
            next_flag(
                self,
                flag,
                to,
                ROUTE_LIMIT,
                SearchOptions::BUSY | SearchOptions::LOAD | SearchOptions::WATER,
            )
        });
        if let Some(next) = next {
            if let Some(ware) = self.wares.get_mut(record) {
                ware.flag = Some(next);
            }
        } else {
            self.give_up_order(record);
            if let Some(ware) = self.wares.get_mut(record) {
                ware.state = WareState::Stranded;
                ware.flag = Some(flag);
            }
        }
        true
    }

    /// Takes ware `record` off flag `flag`'s list (`0x2a324`). Returns false when it is not
    /// there.
    pub(crate) fn take_ware(&mut self, flag: u16, record: u16) -> bool {
        let Some(head) = self.flag(flag).and_then(|flag| flag.wares) else {
            return false;
        };
        let next = self.wares.get(record).and_then(|ware| ware.link);
        if head == record {
            if let Some(flag) = self.flag_mut(flag) {
                flag.wares = next;
                flag.waiting = flag.waiting.saturating_sub(1);
            }
            return true;
        }
        let Some(before) = self
            .wares_at(flag)
            .find(|(_, ware)| ware.link == Some(record))
            .map(|(r, _)| r)
        else {
            return false;
        };
        if let Some(ware) = self.wares.get_mut(before) {
            ware.link = next;
        }
        if let Some(flag) = self.flag_mut(flag) {
            flag.waiting = flag.waiting.saturating_sub(1);
        }
        true
    }

    /// The ware at flag `flag` a carrier takes towards flag `toward` (`0x2a652`): of the wares
    /// there with `toward` as their next flag, neither stranded nor taken, the one with the
    /// highest transport priority for the flag's owner, and of those the one put down first.
    #[must_use]
    pub fn best_ware(&self, flag: u16, toward: u16) -> Option<u16> {
        let owner = self.flag(flag)?.owner;
        let priorities = self.priorities.get(owner.index())?;
        let mut best: Option<(u16, u8)> = None;
        for (record, ware) in self.wares_at(flag) {
            if matches!(ware.state, WareState::Stranded | WareState::Taken) || ware.flag != Some(toward) {
                continue;
            }
            let value = priorities.get(ware.kind.index()).copied().unwrap_or(0);
            if best.is_none_or(|(_, top)| value >= top) {
                best = Some((record, value));
            }
        }
        best.map(|(record, _)| record)
    }

    /// Ware `record` has reached its destination flag `flag` and goes into the building there.
    fn hand_over(&mut self, record: u16, flag: u16) {
        match self.building_at_flag(flag) {
            Some(building) => self.receive(building, record, true),
            None => {
                if let Some(ware) = self.wares.remove(record)
                    && let Some(order) = ware.order
                {
                    self.orders.remove(order);
                }
            }
        }
    }

    /// Building `building` takes ware `record` in (`0x2d177`) and the ware record is freed. A
    /// site stocks boards and stones, which come off the owner's ware count (`0x2d1f7`), drops
    /// anything else, and frees the order. A storehouse adds
    /// the ware to its stock, and a finished building that uses it adds it to that input. Then a
    /// ware `direct` from its journey settles its order, and one a storehouse's settler carries
    /// back in leaves its order wanting the ware again.
    pub(crate) fn receive(&mut self, building: u16, record: u16, direct: bool) {
        let Some(ware) = self.wares.get(record).copied() else {
            return;
        };
        let Some(target) = self.building(building).copied() else {
            self.wares.remove(record);
            return;
        };
        if target.state.contains(BuildingState::BUILDING) {
            if let Some(index) = construction_index(ware.kind) {
                if let Some(site) = self.building_mut(building) {
                    site.stock[index] += 1;
                }
                self.drop_ware(target.owner, ware.kind, 1);
            }
            if let Some(order) = ware.order {
                self.orders.remove(order);
            }
        } else {
            if let Some(slot) = self.input_slot(building, ware.kind)
                && let Some(workplace) = self.building_mut(building)
            {
                workplace.stock[slot] = workplace.stock[slot].saturating_add(1);
            }
            let expedition = target.kind == BuildingKind::HARBOUR
                && ware
                    .order
                    .and_then(|order| self.orders.get(order))
                    .is_some_and(|order| order.tag == crate::supply::EXPEDITION_TAG);
            if expedition {
                if let Some(index) = construction_index(ware.kind)
                    && let Some(harbour) = self
                        .harbour_of(building)
                        .and_then(|harbour| self.harbours.get_mut(harbour))
                {
                    let held = if index == 0 {
                        &mut harbour.boards
                    } else {
                        &mut harbour.stones
                    };
                    *held = held.wrapping_add(1);
                }
            } else if target.is_storehouse()
                && let Some(count) = self
                    .storehouse_of(building)
                    .map(|(store, _)| store)
                    .and_then(|store| self.storehouse_mut(store))
                    .and_then(|store| store.wares.get_mut(ware.kind.index()))
            {
                *count = count.saturating_add(1);
            }
            if direct {
                self.give_up_order(record);
            } else if let Some(order) = ware.order.and_then(|order| self.orders.get_mut(order)) {
                order.delivering = false;
                order.on_way = None;
            }
        }
        self.wares.remove(record);
    }

    /// Ware `record` gives up its order (`0x2aa9f`) and is stranded. A site's order wants the
    /// ware again, and for boards and stones the site counts one fewer on its way; any other
    /// order is freed, and a finished building that uses the ware counts one fewer on its way.
    /// The ware's order field keeps pointing at the order, as in `S2.EXE`;
    /// [`Self::destination`] ignores it from then on.
    pub(crate) fn give_up_order(&mut self, record: u16) {
        let Some(ware) = self.wares.get_mut(record) else {
            return;
        };
        if ware.state == WareState::Stranded {
            return;
        }
        ware.state = WareState::Stranded;
        let (kind, order) = (ware.kind, ware.order);
        let Some(order) = order else {
            return;
        };
        let site = self
            .orders
            .get(order)
            .and_then(|order| self.building_at_flag(order.flag))
            .filter(|&building| {
                self.building(building)
                    .is_some_and(|record| record.state.contains(BuildingState::BUILDING))
            });
        let Some(building) = site else {
            let input = self
                .orders
                .get(order)
                .and_then(|order| self.building_at_flag(order.flag))
                .and_then(|building| Some((building, self.input_slot(building, kind)?)));
            if let Some((building, slot)) = input
                && let Some(workplace) = self.building_mut(building)
            {
                workplace.on_way[slot] = workplace.on_way[slot].saturating_sub(1);
                workplace.requested = workplace.requested.saturating_sub(1);
            }
            self.orders.remove(order);
            return;
        };
        if let Some(index) = construction_index(kind)
            && let Some(site) = self.building_mut(building)
        {
            site.on_way[index] = site.on_way[index].saturating_sub(1);
            site.requested = site.requested.saturating_sub(1);
        }
        if let Some(order) = self.orders.get_mut(order) {
            order.delivering = false;
        }
    }

    /// The road from flag `at` to flag `toward` is going (`0x2ac34`): every ware waiting at `at`
    /// for `toward` that is not already stranded gives up its order and is stranded at `at`.
    pub(crate) fn strand_wares_for(&mut self, at: u16, toward: u16) {
        let cut_off: Vec<u16> = self
            .wares_at(at)
            .filter(|(_, ware)| {
                !matches!(ware.state, WareState::Stranded | WareState::Unrouted) && ware.flag == Some(toward)
            })
            .map(|(record, _)| record)
            .collect();
        for record in cut_off {
            self.give_up_order(record);
            if let Some(ware) = self.wares.get_mut(record) {
                ware.flag = Some(at);
            }
        }
    }

    /// The road from flag `at` to flag `toward` is split at flag `via` (`0x28f04`): every ware
    /// waiting at `at` for `toward` goes to `via` instead, and one a carrier was coming to fetch
    /// waits again.
    pub(crate) fn redirect_wares(&mut self, at: u16, toward: u16, via: u16) {
        let mut next = self.flag(at).and_then(|flag| flag.wares);
        while let Some(record) = next {
            let Some(ware) = self.wares.get_mut(record) else {
                break;
            };
            if ware.flag == Some(toward) {
                ware.flag = Some(via);
                if ware.state == WareState::Taken {
                    ware.state = WareState::Waiting;
                }
            }
            next = ware.link;
        }
    }

    /// The stranded-ware pass (`0x2a7e4`), every 80 steps: each stranded ware at a flag, in
    /// record order, looks for a destination. One that finds it is sent there and waits with
    /// somewhere to go: handed straight in when the destination is at its own flag, otherwise
    /// given its next flag.
    pub(crate) fn route_stranded(&mut self) {
        let stranded: Vec<(u16, u16, WareKind)> = self
            .wares
            .iter()
            .filter(|(_, ware)| ware.state == WareState::Stranded)
            .filter_map(|(record, ware)| Some((record, ware.flag?, ware.kind)))
            .collect();
        for (record, flag, kind) in stranded {
            let Some(to) = self.destination_for(flag, kind) else {
                continue;
            };
            let Some(to_flag) = self.building(to).map(|building| building.flag) else {
                continue;
            };
            self.send_ware(record, to);
            if to_flag == flag {
                self.take_ware(flag, record);
                self.receive(to, record, true);
            } else if let Some(next) = next_flag(
                self,
                flag,
                to_flag,
                ROUTE_LIMIT,
                SearchOptions::BUSY | SearchOptions::LOAD | SearchOptions::WATER,
            ) && let Some(ware) = self.wares.get_mut(record)
            {
                ware.flag = Some(next);
            }
        }
    }

    /// The input slot of finished building `building` that takes ware type `ware`.
    pub(crate) fn input_slot(&self, building: u16, ware: WareKind) -> Option<usize> {
        let building = self.building(building)?;
        if !building.state.contains(BuildingState::FINISHED) {
            return None;
        }
        self.building_types()
            .get(building.kind)?
            .inputs
            .iter()
            .position(|&input| input == Some(ware))
    }

    /// The flag waiting ware `record` sits at (`0x2ab7d`): its next flag, or else one of the
    /// flags that flag's roads lead to.
    pub(crate) fn holding_flag(&self, record: u16) -> Option<u16> {
        let next = self.wares.get(record)?.flag?;
        let far = self
            .flag(next)
            .into_iter()
            .flat_map(|flag| flag.links.into_iter().flatten().filter_map(|link| link.far));
        std::iter::once(next)
            .chain(far)
            .find(|&flag| self.wares_at(flag).any(|(held, _)| held == record))
    }

    /// Ware `record` is lost with the carrier that held it (`0x32454`): it comes off `owner`'s
    /// count (`0x29991`), it gives up its order and its record is freed.
    pub(crate) fn lose_ware(&mut self, owner: Player, record: u16) {
        if let Some(kind) = self.wares.get(record).map(|ware| ware.kind) {
            self.drop_ware(owner, kind, 1);
        }
        self.give_up_order(record);
        self.wares.remove(record);
    }
}

#[cfg(test)]
mod tests;
