//! Expeditions: a harbour collecting boards, stones and a builder, a ship taking them aboard,
//! the search for harbour sites round the ship, and founding a harbour on another coast.

use super::{SECTORS, ShipMode};
use crate::game::Game;
use crate::grid::{Cursor, Direction, Node};
use crate::homeward::{MAX_PATHS, Path};
use crate::job::Job;
use crate::nodes::{NodeFlags, SiteSize};
use crate::supply::{EXPEDITION_TAG, construction_index};
use crate::ware::WareKind;
use crate::world::BuildingKind;

/// How far round an expedition's landing it reveals the map (`0x3fe8c`).
const LANDING_SIGHT: u16 = 6;

/// The rings the harbour site search walks (`0x41601`): rings 1 and 2 are never searched.
const SITE_RINGS: std::ops::RangeInclusive<i32> = 3..=129;

/// The order `0x40949` looks round a ship for the harbour site to found: north-east round to
/// north-west.
const FOUNDING_LOOK: [Direction; 6] = [
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
];

impl Game {
    /// Harbour building `building` starts preparing an expedition (`0x43475`): the boards and
    /// stones its type costs and a builder come out of its stock, and what is missing is ordered
    /// to its flag with the expedition's tag. No helper becomes a builder here.
    pub(crate) fn prepare_expedition(&mut self, building: u16) {
        let Some(record) = self.world.harbour_of(building) else {
            return;
        };
        let Some(site) = self.world.building(building).copied() else {
            return;
        };
        let Some(row) = self.world.building_types().get(site.kind) else {
            return;
        };
        let costs = [row.cost(0), row.cost(1)];
        let Some((store, _)) = self.world.storehouse_of(building) else {
            return;
        };
        let mut collected = [0; 2];
        for ((&ware, need), held) in WareKind::CONSTRUCTION.iter().zip(costs).zip(&mut collected) {
            let Some(stock) = self
                .world
                .storehouse_mut(store)
                .map(|store| &mut store.wares[ware.index()])
            else {
                return;
            };
            if *stock >= u16::from(need) {
                *stock -= u16::from(need);
                *held = need;
                continue;
            }
            *held = u8::try_from(*stock).unwrap_or(u8::MAX);
            let missing = need - *held;
            *stock = 0;
            for _ in 0..missing {
                self.world.order_ware(site.flag, ware, EXPEDITION_TAG);
            }
        }
        let builder = self.world.storehouse_mut(store).is_some_and(|store| {
            let builders = &mut store.jobs[Job::BUILDER.index()];
            let held = *builders > 0;
            if held {
                *builders -= 1;
            }
            held
        });
        if !builder {
            self.world
                .order_person(site.flag, Job::BUILDER, 0, EXPEDITION_TAG);
        }
        if let Some(harbour) = self.world.harbours.get_mut(record) {
            harbour.expedition = true;
            [harbour.boards, harbour.stones] = collected;
            harbour.builders = u8::from(builder);
        }
    }

    /// An expedition's ware order `record` that no storehouse on the roads can serve takes the
    /// ware from the harbour's own stock when it has one (`0x2b7b3`): the harbour counts it
    /// collected and the order is freed. Returns whether it did.
    pub(crate) fn take_expedition_ware(&mut self, record: u16) -> bool {
        let Some(order) = self.world.order(record).copied() else {
            return false;
        };
        let (Some(ware), Some(building)) = (order.ware(), self.world.building_at_flag(order.flag)) else {
            return false;
        };
        let Some(index) = construction_index(ware) else {
            return false;
        };
        let Some((store, stock)) = self.world.storehouse_of(building) else {
            return false;
        };
        if stock.wares[ware.index()] == 0 {
            return false;
        }
        if let Some(stock) = self.world.storehouse_mut(store) {
            stock.wares[ware.index()] -= 1;
        }
        if let Some(harbour) = self
            .world
            .harbour_of(building)
            .and_then(|harbour| self.world.harbours.get_mut(harbour))
        {
            let held = if index == 0 {
                &mut harbour.boards
            } else {
                &mut harbour.stones
            };
            *held = held.wrapping_add(1);
        }
        self.world.orders.remove(record);
        true
    }

    /// An expedition's builder order `record` is tried against the harbour's own stock before
    /// any search (`0x2b930`): a builder in stock is taken, or a helper with a hammer becomes one
    /// who stays in stock as well, and the harbour counts a builder collected either way. The
    /// order is freed. Returns whether it was served so.
    pub(crate) fn expedition_builder(&mut self, record: u16) -> bool {
        let Some(order) = self.world.order(record).copied() else {
            return false;
        };
        let Some(building) = self.world.building_at_flag(order.flag) else {
            return false;
        };
        let Some((store, stock)) = self.world.storehouse_of(building) else {
            return false;
        };
        if stock.supplies(Job::BUILDER).is_none() {
            return false;
        }
        let mut converted = None;
        if let Some(stock) = self.world.storehouse_mut(store) {
            if stock.jobs[Job::BUILDER.index()] > 0 {
                stock.jobs[Job::BUILDER.index()] -= 1;
            } else {
                converted = Some(stock.take_person(Job::BUILDER));
                stock.jobs[Job::BUILDER.index()] += 1;
            }
        }
        if let Some(taken) = converted {
            self.world.count_conversion(building, Job::BUILDER, taken);
        }
        if let Some(harbour) = self
            .world
            .harbour_of(building)
            .and_then(|harbour| self.world.harbours.get_mut(harbour))
        {
            harbour.builders = harbour.builders.wrapping_add(1);
        }
        self.world.orders.remove(record);
        true
    }

    /// Ship `ship`, just unloaded at harbour `harbour`, takes its expedition aboard if it is
    /// ready (`0x42f34`). With no harbour site in any sector round the landing, the harbour
    /// gives up the expedition and its stock gets the boards, stones and builder back; the
    /// counts collected stay as they were. Returns whether the ship took it.
    pub(crate) fn board_expedition(&mut self, ship: u16, harbour: u16) -> bool {
        if !self.expedition_ready(harbour) {
            return false;
        }
        let Some(building) = self.world.harbours.get(harbour).map(|harbour| harbour.building) else {
            return false;
        };
        let Some(row) = self
            .world
            .building(building)
            .and_then(|site| self.world.building_types().get(site.kind))
        else {
            return false;
        };
        let costs = [row.cost(0), row.cost(1)].map(u16::from);
        let found = self.harbour_sites(ship, None);
        if let Some(record) = self.world.harbours.get_mut(harbour) {
            record.expedition = false;
        }
        if !found {
            let Some((store, _)) = self.world.storehouse_of(building) else {
                return false;
            };
            if let Some(store) = self.world.storehouse_mut(store) {
                for (&ware, cost) in WareKind::CONSTRUCTION.iter().zip(costs) {
                    store.wares[ware.index()] = store.wares[ware.index()].wrapping_add(cost);
                }
                store.jobs[Job::BUILDER.index()] = store.jobs[Job::BUILDER.index()].wrapping_add(1);
            }
            return false;
        }
        if let Some(record) = self.world.ships.get_mut(ship) {
            record.unclaimed = false;
            record.sailing = false;
            record.mode = ShipMode::Expedition { target: None };
            for (&ware, cost) in WareKind::CONSTRUCTION.iter().zip(costs) {
                record.wares[ware.index()] = record.wares[ware.index()].wrapping_add(cost);
            }
            record.people[Job::BUILDER.index()] = record.people[Job::BUILDER.index()].wrapping_add(1);
        }
        true
    }

    /// An expedition ship `ship` has reached its target on `node`: it reveals the map round
    /// it, looks for harbour sites from here and notes whether it stopped on unclaimed land.
    pub(crate) fn expedition_arrives(&mut self, ship: u16, node: Node) {
        let Some(owner) = self.world.ships.get(ship).map(|ship| ship.owner) else {
            return;
        };
        self.nodes.reveal(node, owner, LANDING_SIGHT);
        self.harbour_sites(ship, None);
        let unclaimed = self.nodes.get(node).owner.is_none();
        if let Some(record) = self.world.ships.get_mut(ship) {
            record.unclaimed = unclaimed;
        }
    }

    /// The harbour sites round ship `ship` (`0x41601`): rings 3 to 129 round its node, without
    /// wrapping, each split into six sides north to north-west. Each side not yet settled is
    /// tested node by node for a free harbour site of the map with water of the ship's sea two
    /// nodes away; the node beside the site towards that water is noted for the side. Noting
    /// one shifts the walk two nodes off the ring for the rest of the search, as `S2.EXE` does.
    /// With `only`, the ship's target becomes that side's node and the answer says whether it
    /// had one; otherwise the ship notes which sides have one and the answer says whether any
    /// does.
    fn harbour_sites(&mut self, ship: u16, only: Option<usize>) -> bool {
        let Some(record) = self.world.ships.get(ship).copied() else {
            return false;
        };
        let Some(centre) = self.world.figure(record.figure).map(crate::figures::Figure::node) else {
            return false;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let mut found: [Option<Node>; SECTORS] = [None; SECTORS];
        let mut cursor = Cursor::at(centre)
            .step(Direction::NorthWest)
            .step(Direction::NorthWest);
        for ring in SITE_RINGS {
            cursor = cursor.step(Direction::NorthWest);
            for (side, &direction) in Direction::ALL.iter().enumerate() {
                for _ in 0..ring {
                    if found[side].is_none()
                        && let Some(water) = self.expedition_site(cursor, record.sea)
                    {
                        cursor = cursor.step(Direction::from_index(water + 4));
                        found[side] = Some(Node::wrapped(cursor.x, cursor.y, width, height));
                        cursor = cursor.step(Direction::from_index(water + 3));
                    }
                    cursor = cursor.step(direction);
                }
            }
        }
        let Some(record) = self.world.ships.get_mut(ship) else {
            return false;
        };
        let Some(side) = only else {
            record.sectors = found.map(|node| node.is_some());
            return found.iter().any(Option::is_some);
        };
        let Some(target) = found.get(side).copied().flatten() else {
            return false;
        };
        record.mode = ShipMode::Expedition { target: Some(target) };
        true
    }

    /// Whether `cursor` stands on a free harbour site of the map that offers a large building,
    /// and if so the side of the ring two nodes round it, east first, whose first node lies on
    /// sea `sea` (`0x41601`).
    fn expedition_site(&self, cursor: Cursor, sea: u8) -> Option<usize> {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let node = cursor.node(width, height)?;
        let data = self.nodes.get(node);
        if !data.flags.contains(NodeFlags::HARBOUR)
            || data.sites.size != SiteSize::Large
            || data.owner.is_some()
        {
            return None;
        }
        let mut at = cursor.step(Direction::NorthWest).step(Direction::NorthWest);
        let mut side = None;
        for (k, &direction) in Direction::ALL.iter().enumerate() {
            for _ in 0..2 {
                if side.is_none()
                    && at
                        .node(width, height)
                        .is_some_and(|node| self.nodes.get(node).area == sea)
                {
                    side = Some(k);
                }
                at = at.step(direction);
            }
        }
        side
    }

    /// The player sends expedition ship `ship` to the harbour site in sector `sector`
    /// (`0x43039`): the sector's target is looked up from where the ship is, and the ship sets
    /// sail when it has one and a route there. `S2.EXE` takes a path record first and keeps it
    /// in the ship record whether or not it is used, so every failed try holds one for good;
    /// Robigo frees it.
    pub(crate) fn sail_expedition(&mut self, ship: u16, sector: usize) {
        let Some(record) = self.world.ships.get(ship).copied() else {
            return;
        };
        let Some(node) = self.world.figure(record.figure).map(crate::figures::Figure::node) else {
            return;
        };
        let path = self.world.paths.insert(Path::empty(node), MAX_PATHS);
        let target = self
            .harbour_sites(ship, Some(sector))
            .then(|| self.world.ships.get(ship).map(|ship| ship.mode))
            .flatten();
        let sailing = match (path, target) {
            (Some(path), Some(ShipMode::Expedition { target: Some(target) })) => {
                self.sea_route(path, node, target, record.sea)
            }
            _ => false,
        };
        if !sailing {
            if let Some(path) = path {
                self.world.paths.remove(path);
            }
            return;
        }
        if let Some(record) = self.world.ships.get_mut(ship) {
            record.path = path;
            record.sailing = true;
        }
    }

    /// The player calls expedition ship `ship` off (`0x418d0`): a path it sails is freed, and it
    /// becomes a transport ship with no harbour, its cargo still aboard.
    pub(crate) fn cancel_expedition(&mut self, ship: u16) {
        let Some(record) = self.world.ships.get_mut(ship) else {
            return;
        };
        if !matches!(record.mode, ShipMode::Expedition { .. }) {
            return;
        }
        if record.sailing
            && let Some(path) = record.path.take()
        {
            self.world.paths.remove(path);
        }
        if let Some(record) = self.world.ships.get_mut(ship) {
            record.mode = ShipMode::Transport { harbour: None };
            record.sailing = false;
        }
    }

    /// The player has expedition ship `ship` found a harbour where it lies (`0x43039`,
    /// `0x40949`), on land that belongs to nobody. On success the ship becomes a transport ship
    /// with no harbour.
    pub(crate) fn found_from_ship(&mut self, ship: u16) {
        let Some(record) = self.world.ships.get(ship).copied() else {
            return;
        };
        let Some(node) = self.world.figure(record.figure).map(crate::figures::Figure::node) else {
            return;
        };
        if self.nodes.get(node).owner.is_some() || !self.found_harbour(ship, node) {
            return;
        }
        if let Some(record) = self.world.ships.get_mut(ship) {
            record.sailing = false;
            record.mode = ShipMode::Transport { harbour: None };
        }
    }

    /// Expedition ship `ship` on `node` founds a harbour (`0x40949`) on the first neighbouring
    /// harbour site of the map, north-east round to north-west, that offers a large building and
    /// belongs to nobody. The site and its neighbours become the ship's owner's, a harbour site
    /// goes there with its builder at work at once, the land round it is claimed and its
    /// protection lifted, and the site gets its boards and stones whether or not the ship
    /// carried them all. The builder order the site placed is settled. Returns whether it did.
    fn found_harbour(&mut self, ship: u16, node: Node) -> bool {
        let Some(record) = self.world.ships.get(ship).copied() else {
            return false;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let Some(site) = FOUNDING_LOOK
            .into_iter()
            .map(|direction| node.neighbour(direction, width, height))
            .find(|&at| self.nodes.get(at).flags.contains(NodeFlags::HARBOUR))
        else {
            return false;
        };
        let data = self.nodes.get(site);
        if data.sites.size != SiteSize::Large || data.owner.is_some() {
            return false;
        }
        for at in std::iter::once(site)
            .chain(Direction::ALL.map(|direction| site.neighbour(direction, width, height)))
        {
            let mut data = self.nodes.get(at);
            data.owner = Some(record.owner);
            self.nodes.set(at, data);
        }
        let Some(building) =
            self.world
                .place_site(&mut self.nodes, site, BuildingKind::HARBOUR, record.owner)
        else {
            return true;
        };
        self.start_builder(building);
        self.claim_land(building);
        for node in crate::territory::protected_nodes((width, height), site) {
            let mut data = self.nodes.get(node);
            data.protected = false;
            self.nodes.set(node, data);
        }
        let costs = self
            .world
            .building_types()
            .get(BuildingKind::HARBOUR)
            .map_or([0; 2], |row| [row.cost(0), row.cost(1)]);
        for (index, (&ware, need)) in WareKind::CONSTRUCTION.iter().zip(costs).enumerate() {
            for _ in 0..need {
                if let Some(cargo) = self.world.ships.get_mut(ship) {
                    let aboard = &mut cargo.wares[ware.index()];
                    *aboard = aboard.saturating_sub(1);
                }
                if let Some(site) = self.world.building_mut(building) {
                    site.stock[index] += 1;
                    site.on_way[index] += 1;
                    site.requested += 1;
                }
            }
        }
        if let Some(flag) = self.world.building(building).map(|site| site.flag) {
            self.settle_orders(flag);
        }
        if let Some(cargo) = self.world.ships.get_mut(ship) {
            let builders = &mut cargo.people[Job::BUILDER.index()];
            *builders = builders.saturating_sub(1);
        }
        true
    }

    /// Whether finished harbour `record` takes boards or stones, `ware`, for an expedition it
    /// prepares: a produced ware sent there claims an open expedition order for it at the
    /// harbour's flag (`0x2dab7`). Returns the order it claimed.
    pub(crate) fn claim_expedition_order(&mut self, ware: u16, record: u16) -> Option<u16> {
        let building = self.world.building(record).copied()?;
        let kind = self.world.ware(ware)?.kind;
        if building.kind != BuildingKind::HARBOUR
            || !building.state.contains(crate::world::BuildingState::FINISHED)
            || construction_index(kind).is_none()
            || !self
                .world
                .harbour_of(record)
                .and_then(|harbour| self.world.harbours.get(harbour))
                .is_some_and(|harbour| harbour.expedition)
        {
            return None;
        }
        let order = self
            .world
            .orders()
            .find(|(_, order)| {
                order.flag == building.flag
                    && order.tag == EXPEDITION_TAG
                    && order.ware() == Some(kind)
                    && order.kind() == crate::transport::OrderKind::Ware
            })
            .map(|(order, _)| order)?;
        if let Some(open) = self.world.orders.get_mut(order) {
            open.delivering = true;
            open.on_way = Some(ware);
        }
        if let Some(sent) = self.world.wares.get_mut(ware) {
            sent.order = Some(order);
            sent.state = crate::transport::WareState::Waiting;
        }
        Some(order)
    }
}

#[cfg(test)]
mod tests;
