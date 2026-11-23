//! Wares out of storehouses: serving a ware order, the emergency programme that keeps the last
//! boards and stones for the first woodcutter and sawmill, the settler who brings the ware out to
//! the flag, and the ware orders a building site places when its builder is sent.
//!
//! Not yet modelled: expeditions, the messages the emergency programme posts, and orders for
//! finished buildings.

use crate::figures::{Figure, FigureKind};
use crate::game::Game;
use crate::grid::Direction;
use crate::job::Job;
use crate::settlers::DOOR_SPEED;
use crate::transport::{MAX_WARES, MOST_URGENT, SearchOptions, Ware, WareState, search};
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, FLAG_CAPACITY, Player};

/// How far the storehouse search for a ware reaches.
const SEARCH_LIMIT: u16 = 250;

/// What a busy storehouse adds to the urgency of a ware order it could not serve.
const BUSY_BONUS: u8 = 5;

/// What a failed ware order loses in urgency.
const FAILED_PENALTY: u8 = 40;

/// The tag of an expedition's orders (`0x43475`).
pub(crate) const EXPEDITION_TAG: u8 = 1;

/// A site whose `requested` count has reached this has already ordered its wares.
pub(crate) const WARES_ORDERED: u8 = 2;

/// The construction index of ware type `ware`: 0 boards, 1 stones.
pub(crate) fn construction_index(ware: WareKind) -> Option<usize> {
    WareKind::CONSTRUCTION.iter().position(|&kind| kind == ware)
}

impl Game {
    /// Serves ware order `record` (`0x2b664`): the nearest finished storehouse with the ware in
    /// stock sends one, unless the emergency programme keeps it back. A storehouse whose door is
    /// busy adds 5 to the urgency, and any failure then takes 40 off. When no storehouse is
    /// found, the order is linked to the first harbour the search passed, or at a harbour's flag
    /// moves on to the next harbour's; an expedition's order takes the harbour's own stock.
    pub(crate) fn serve_ware(&mut self, record: u16) {
        let Some((order, wanted)) = self
            .world
            .order(record)
            .and_then(|order| Some((*order, order.ware()?)))
        else {
            return;
        };
        let destination = self.world.building_at_flag(order.flag);
        let (found, passed) = self.ware_storehouse(order.flag, wanted);
        if found.is_none() {
            self.cross_water(record, passed, order.tag != EXPEDITION_TAG);
            if order.tag == EXPEDITION_TAG && self.take_expedition_ware(record) {
                return;
            }
        }
        let sent = found.and_then(|(store, flag)| {
            if self.emergency_refuses(wanted, destination, flag) {
                return None;
            }
            let (ware, busy) = self.send_ware(Some(record), store, flag, wanted);
            if busy && let Some(order) = self.world.orders.get_mut(record) {
                order.urgency = order.urgency.saturating_add(BUSY_BONUS).min(MOST_URGENT);
            }
            ware
        });
        let Some(order) = self.world.orders.get_mut(record) else {
            return;
        };
        let Some(ware) = sent else {
            order.urgency = order.urgency.saturating_sub(FAILED_PENALTY);
            return;
        };
        order.on_way = Some(ware);
        order.delivering = true;
        self.world.release_link(record);
        let index = construction_index(wanted);
        if let (Some(index), Some(site)) = (
            index,
            destination.and_then(|building| self.world.building_mut(building)),
        ) && site.state.contains(BuildingState::BUILDING)
        {
            site.on_way[index] += 1;
            site.requested += 1;
        }
    }

    /// The storehouse to send ware type `ware` to flag `flag` from (`0x3bd24`): the nearest
    /// finished one, by the congested search that may cross water, that has one in stock. A
    /// storehouse at `flag` itself is never chosen. Returns its building and its flag, and the
    /// first finished harbour the search passed unless a harbour stands at `flag`.
    fn ware_storehouse(&self, flag: u16, ware: WareKind) -> (Option<(u16, u16)>, Option<u16>) {
        let options =
            SearchOptions::BUSY | SearchOptions::LOAD | SearchOptions::WATER | SearchOptions::SKIP_START;
        let mut passed = None;
        let at_harbour = self.world.harbour_at_flag(flag).is_some();
        let found = search(&self.world, flag, SEARCH_LIMIT, options, |reached| {
            if passed.is_none() && !at_harbour {
                passed = self.world.harbour_at_flag(reached.flag);
            }
            self.storehouse_at_flag(reached.flag)
                .is_some_and(|(_, store)| store.wares.get(ware.index()).is_some_and(|&n| n > 0))
        });
        let found = found.and_then(|found| Some((self.storehouse_at_flag(found.flag)?.0, found.flag)));
        (found, passed)
    }

    /// Order `record`, which no storehouse on its roads could serve, turns to the harbours
    /// (`0x3bd24`, `0x3befc`, `0x2b664`, `0x2b8d4`): with no link yet it is linked to harbour
    /// `passed`, the first its search passed. At a finished harbour's flag, when `hop` allows,
    /// it moves to the next harbour's flag for the next pass to search from.
    pub(crate) fn cross_water(&mut self, record: u16, passed: Option<u16>, hop: bool) {
        let Some(order) = self.world.order(record).copied() else {
            return;
        };
        if let Some(harbour) = passed
            && order.harbour.is_none()
        {
            self.world.link_to_harbour(record, harbour);
        }
        if hop
            && let Some(next) = self
                .world
                .harbour_at_flag(order.flag)
                .and_then(|harbour| self.world.next_harbour_flag(harbour))
            && let Some(order) = self.world.orders.get_mut(record)
        {
            order.flag = next;
        }
    }

    /// Whether the emergency programme refuses to send boards or stones from the storehouse at
    /// `flag` to `destination` (`0x2b458`), switching the programme on or off on the way.
    ///
    /// With a woodcutter and a sawmill finished the programme goes off. Otherwise, while it is
    /// off, a storehouse holding no more of the ware than a missing sawmill and woodcutter would
    /// take switches it on and refuses. While it is on, the ware goes only to the first
    /// woodcutter and sawmill to ask, and only while none of that type is finished.
    fn emergency_refuses(&mut self, ware: WareKind, destination: Option<u16>, flag: u16) -> bool {
        let Some(index) = construction_index(ware) else {
            return false;
        };
        let Some(owner) = self.world.flag(flag).map(|flag| flag.owner) else {
            return false;
        };
        let woodcutters = self.finished(owner, BuildingKind::WOODCUTTER);
        let sawmills = self.finished(owner, BuildingKind::SAWMILL);
        let programme = self.world.emergency[owner.index()];
        if woodcutters && sawmills {
            self.world.emergency[owner.index()].on = false;
            return false;
        }
        if !programme.on {
            let types = self.world.building_types();
            let cost = |kind: BuildingKind| types.get(kind).map_or(0, |row| row.cost(index));
            let mut limit = 0;
            if !sawmills {
                limit += cost(BuildingKind::SAWMILL);
            }
            if !woodcutters && index == 0 {
                limit += cost(BuildingKind::WOODCUTTER);
            }
            let stock = self
                .storehouse_at_flag(flag)
                .and_then(|(_, store)| store.wares.get(ware.index()).copied())
                .unwrap_or(0);
            if limit != 0 && stock <= u16::from(limit) {
                self.world.emergency[owner.index()] = crate::world::Emergency {
                    on: true,
                    woodcutter: None,
                    sawmill: None,
                };
                return true;
            }
            return false;
        }
        let kind = destination
            .and_then(|building| self.world.building(building))
            .map(|building| building.kind);
        let (claimed, built) = match kind {
            Some(BuildingKind::WOODCUTTER) => {
                (&mut self.world.emergency[owner.index()].woodcutter, woodcutters)
            }
            Some(BuildingKind::SAWMILL) => (&mut self.world.emergency[owner.index()].sawmill, sawmills),
            _ => return true,
        };
        match *claimed {
            Some(site) => Some(site) != destination,
            None if !built => {
                *claimed = destination;
                false
            }
            None => true,
        }
    }

    /// Whether `owner` has a finished building of type `kind` (player block `+0x40` and `+0x4a`,
    /// inferred to count finished buildings only: a woodcutter site claims the programme only
    /// while the count is 0).
    fn finished(&self, owner: Player, kind: BuildingKind) -> bool {
        self.world.buildings().any(|(_, building)| {
            building.owner == owner
                && building.kind == kind
                && building.state.contains(BuildingState::FINISHED)
        })
    }

    /// Storehouse `building`, whose flag is `flag`, sends a ware of type `ware` for order `order`,
    /// or with no order as a ware that is going nowhere yet (`0x2d067`, `0x34d38`). Returns the ware, and whether the storehouse was busy. Nothing is
    /// sent when the flag is full, the stock has run out, the door is busy or the ware pool is
    /// full.
    pub(crate) fn send_ware(
        &mut self,
        order: Option<u16>,
        building: u16,
        flag: u16,
        ware: WareKind,
    ) -> (Option<u16>, bool) {
        let Some(at) = self.world.flag(flag).copied() else {
            return (None, false);
        };
        if at.waiting >= FLAG_CAPACITY {
            return (None, false);
        }
        let Some((store, stock)) = self.world.storehouse_of(building) else {
            return (None, false);
        };
        if stock.wares.get(ware.index()).is_none_or(|&n| n == 0) {
            return (None, false);
        }
        if !self
            .world
            .building(building)
            .is_some_and(|record| record.state.contains(BuildingState::PRESENT))
        {
            return (None, true);
        }
        let mut record = Ware::new(ware);
        if order.is_some() {
            record.state = WareState::Carried;
            record.order = order;
        }
        let Some(made) = self.world.wares.insert(record, MAX_WARES) else {
            return (None, false);
        };
        let shows = self.world.ware(made).and_then(Ware::bitmap);
        let mut figure = Figure::fresh(FigureKind::Bringing {
            owner: at.owner,
            building,
            ware: Some(made),
            shows,
            inside: false,
        });
        figure.start_walk(Direction::SouthEast, DOOR_SPEED);
        self.world.add_figure(at.node, figure);
        if let Some(count) = self
            .world
            .storehouse_mut(store)
            .and_then(|store| store.wares.get_mut(ware.index()))
        {
            *count -= 1;
        }
        if let Some(record) = self.world.building_mut(building) {
            record.state |= BuildingState::DOOR;
            record.state -= BuildingState::PRESENT;
        }
        (Some(made), false)
    }

    /// One step of a settler bringing a ware out who is not walking. At the flag it closes the
    /// door, puts the ware down if the flag has room and walks back in (`0x36614`). Inside, a
    /// ware it still holds goes back into stock and its order wants it again, and the settler is
    /// gone (`0x36067`).
    pub(crate) fn step_bringing(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Bringing {
            owner,
            building,
            mut ware,
            mut shows,
            inside,
        } = figure.kind
        else {
            return;
        };
        if let Some(store) = self.world.building_mut(building) {
            store.state -= BuildingState::DOOR;
        }
        if inside {
            if let Some(ware) = ware {
                self.world.receive(building, ware, false);
            }
            self.world.remove_figure(record);
            return;
        }
        let flag = self.world.building(building).map(|store| store.flag);
        if let (Some(carried), Some(flag)) = (ware, flag)
            && self.world.put_down(carried, flag)
        {
            ware = None;
            shows = None;
        }
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Bringing {
                owner,
                building,
                ware,
                shows,
                inside: true,
            };
        }
        let door = figure
            .node()
            .neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        self.world.move_figure(record, door);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, DOOR_SPEED);
        }
    }

    /// A storehouse has been found for builder order `order` (`0x2b8d4`): a site at the order's
    /// flag that has not yet ordered its wares orders its boards, then its stones, one order per
    /// ware.
    pub(crate) fn order_site_wares(&mut self, order: u16) {
        let Some(order) = self.world.order(order).copied() else {
            return;
        };
        if order.job() != Some(Job::BUILDER) {
            return;
        }
        let Some(building) = self.world.building_at_flag(order.flag) else {
            return;
        };
        let Some(site) = self.world.building(building).copied() else {
            return;
        };
        if !site.state.contains(BuildingState::BUILDING) || site.requested >= WARES_ORDERED {
            return;
        }
        let Some(row) = self.world.building_types().get(site.kind) else {
            return;
        };
        for (index, &ware) in WareKind::CONSTRUCTION.iter().enumerate() {
            for _ in 0..row.cost(index) {
                self.world.order_ware(order.flag, ware, 0);
                if let Some(site) = self.world.building_mut(building) {
                    site.requested += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
