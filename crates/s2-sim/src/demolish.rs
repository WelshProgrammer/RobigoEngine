//! Demolishing a building or a site (`0x2c9f7`).

use crate::figures::{Figure, FigureKind, Gatherer, Trip};
use crate::game::Game;
use crate::grid::Direction;
use crate::job::Job;
use crate::transport::{ROAD_MODE, WARE_MODE, Wanted, WareState};
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, Category, MapObject};

/// An order placed with this tag is left alone by demolition.
const KEPT_TAG: u8 = 3;

impl Game {
    /// Demolishes building `record` (`0x2c9f7`). Soldiers attacking it or belonging to it go
    /// home first. Then the building is emptied ([`Self::vacate`]): everyone inside a
    /// storehouse or military building, a workplace's worker inside and a builder at work become
    /// new lost settlers on its node, a planer levelling it is lost where it stands, and anyone
    /// in its door is lost, a carrier with the ware it brought. Each order at its flag is
    /// settled and freed: a ware on its way is stranded, and a builder or planer on its way
    /// turns for home. A harbour's ships and orders lose it. Last the building goes, and a
    /// finished one leaves a fire.
    pub(crate) fn demolish(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        if let Some(owner) = self.world.building_owner(record) {
            self.release_soldiers(record, owner, false);
            if building.state.contains(BuildingState::FINISHED) {
                self.lose_stock(record);
                self.world.uncount_building(owner, building.kind);
            } else {
                self.world.uncount_site(owner, building.kind);
            }
        }
        self.vacate(record);
        let category = self
            .world
            .building_types()
            .get(building.kind)
            .map(|row| row.category);
        let occupied = building
            .state
            .contains(BuildingState::FINISHED | BuildingState::MANNED);
        let builder_present = building
            .state
            .contains(BuildingState::BUILDING | BuildingState::PRESENT);
        if builder_present && let Some(builder) = building.builder {
            self.world.free_loose_figure(builder);
        }
        if let Some((military, _)) = self.world.military_of(record) {
            self.world.military.remove(military);
        }
        let harbour = self.world.harbour_of(record);
        if let Some(harbour) = harbour {
            self.strand_ships(harbour);
            self.world.unlink_harbour(harbour);
        }
        let releases = building.kind == BuildingKind::HEADQUARTERS
            || building.kind == BuildingKind::HARBOUR
            || (category == Some(Category::Military) && occupied);
        if releases {
            self.world.set_object(building.node, MapObject::Nothing);
            self.release_land(record);
        }
        if let Some(harbour) = harbour {
            self.world.harbours.remove(harbour);
        }
        self.world.remove_building(&mut self.nodes, record);
    }

    /// What finished building `record` held comes off its owner's ware count: a storehouse loses
    /// its whole stock (`0x2cc6a`), a military building and a workplace lose their input stock
    /// (`0x2cc3a`, `0x2ccf5`), a workplace loses every ware waiting in an output slot
    /// (`0x2cd38`), and a harbour fitting out an expedition loses the boards and stones it
    /// gathered (`0x2cd8e`). A site loses nothing, because each construction ware came off as it
    /// arrived.
    fn lose_stock(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let owner = building.owner;
        let Some(row) = self.world.building_types().get(building.kind) else {
            return;
        };
        match row.category {
            Category::Storehouse => {
                if let Some((_, store)) = self.world.storehouse_of(record) {
                    let stock = store.wares;
                    for (index, &count) in stock.iter().enumerate() {
                        let ware = WareKind(u8::try_from(index).unwrap_or_default());
                        self.world.drop_ware(owner, ware, count);
                    }
                }
            }
            Category::Military | Category::Workplace => {
                for (slot, input) in row.inputs.iter().enumerate() {
                    if let Some(ware) = *input {
                        self.world.drop_ware(owner, ware, u16::from(building.stock[slot]));
                    }
                }
                if row.category == Category::Workplace {
                    for made in building.outputs.iter().flatten() {
                        self.world.drop_ware(owner, *made, 1);
                    }
                }
            }
        }
        if let Some(harbour) = self
            .world
            .harbour_of(record)
            .and_then(|harbour| self.world.harbours.get(harbour))
            .copied()
            .filter(|harbour| harbour.expedition)
        {
            for (ware, count) in WareKind::CONSTRUCTION
                .into_iter()
                .zip([harbour.boards, harbour.stones])
            {
                self.world.drop_ware(owner, ware, u16::from(count));
            }
        }
    }

    /// Empties building `record` without removing it (`0x2c9a0`).
    pub(crate) fn vacate(&mut self, record: u16) {
        let worker = self
            .world
            .building(record)
            .and_then(|building| self.world.building_types().get(building.kind))
            .and_then(|row| row.worker);
        self.vacate_of(record, worker);
    }

    /// [`Self::vacate`] for a workplace whose worker inside has job `worker`.
    pub(crate) fn vacate_of(&mut self, record: u16, worker: Option<Job>) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let flag_node = building.node.neighbour(Direction::SouthEast, width, height);
        let category = self
            .world
            .building_types()
            .get(building.kind)
            .map(|row| row.category);
        let occupied = building
            .state
            .contains(BuildingState::FINISHED | BuildingState::MANNED);
        match category {
            Some(Category::Storehouse) if occupied => {
                if let Some((_, store)) = self.world.storehouse_of(record) {
                    let jobs = store.jobs;
                    for (job, &count) in (0..).map(Job).zip(&jobs) {
                        for _ in 0..count {
                            self.lost_at(building.owner, building.node, job);
                        }
                    }
                }
            }
            Some(Category::Military) if occupied => {
                let owner = self
                    .world
                    .flag(building.flag)
                    .map_or(building.owner, |flag| flag.owner);
                if let Some((_, military)) = self.world.military_of(record) {
                    let ranks = military.ranks;
                    for (&job, count) in Job::RANKS.iter().zip(ranks) {
                        for _ in 0..count {
                            self.lost_at(owner, building.node, job);
                        }
                    }
                }
            }
            Some(Category::Workplace) if occupied && building.state.contains(BuildingState::PRESENT) => {
                if let Some(job) = worker {
                    self.lost_at(building.owner, building.node, job);
                }
            }
            _ => {}
        }
        let builder_present = building
            .state
            .contains(BuildingState::BUILDING | BuildingState::PRESENT);
        if builder_present {
            self.lost_at(building.owner, building.node, Job::BUILDER);
        }
        self.lose_workers(record, building.flag);
        for node in [building.node, flag_node] {
            self.clear_door(node);
        }
        self.settle_orders(building.flag);
    }

    /// Every figure in record order tied to building `building` at `flag` (`0x2c65f`, third
    /// part): a planer working for it or a worker out on a trip is lost, and a settler still
    /// leaving a storehouse for an order at the flag forgets the order. With the bug fixes off,
    /// a lost settler walking home whose countdown matches the building's record, or for a
    /// planer the flag's, is lost again: `S2.EXE` keeps the countdown in the byte it reads as the
    /// workplace. Helpers never are, as the job test passes job 0 over (`0x2c8c2`).
    fn lose_workers(&mut self, building: u16, flag: u16) {
        let misread = !self.world.bug_fixes.on();
        for record in 0..self.world.figure_records() {
            match self.world.figure(record).map(|figure| figure.kind) {
                Some(FigureKind::Planer(planer)) if planer.flag == flag => self.make_lost(record),
                Some(FigureKind::Homeward { job, countdown, .. })
                    if misread
                        && job != Job::HELPER
                        && job != Job::GEOLOGIST
                        && job != Job::SCOUT
                        && u16::from(countdown) == if job == Job::PLANER { flag } else { building } =>
                {
                    self.make_lost(record);
                }
                Some(FigureKind::Gatherer(Gatherer {
                    building: workplace,
                    trip: Trip::Walking { .. } | Trip::Working { .. } | Trip::Waiting,
                    ..
                })) if workplace == building => self.make_lost(record),
                Some(FigureKind::Leaving {
                    order: Some(order), ..
                }) => {
                    let ours = self.world.order(order).is_some_and(|order| {
                        order.flag == flag && order.mode != ROAD_MODE && order.tag != KEPT_TAG
                    });
                    if ours && let Some(FigureKind::Leaving { order, .. }) = self.figure_kind_mut(record) {
                        *order = None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Everyone walking through the door on `node`, from the head of its list (`0x2c47a`), is
    /// lost. A ware carried in or out is lost with its carrier, and a carrier's road orders a
    /// helper to replace it.
    fn clear_door(&mut self, node: crate::grid::Node) {
        let records: Vec<u16> = self.world.figures_at(node).map(|(record, _)| record).collect();
        for record in records {
            match self.world.figure(record).map(|figure| figure.kind) {
                Some(FigureKind::Delivering(delivery)) => {
                    if let Some(ware) = delivery.ware {
                        self.drop_carried(delivery.owner, ware);
                    }
                    if let Some(road) = delivery.road {
                        self.reorder_carrier(road);
                    }
                }
                Some(FigureKind::Bringing { owner, ware, .. }) => {
                    if let Some(ware) = ware {
                        self.drop_carried(owner, ware);
                    }
                }
                Some(
                    FigureKind::Leaving { .. }
                    | FigureKind::Entering { .. }
                    | FigureKind::Settling { .. }
                    | FigureKind::Porter(_),
                ) => {}
                Some(FigureKind::Gatherer(_) | FigureKind::Soldier(_))
                    if self.world.figure(record).is_some_and(Figure::in_door) => {}
                _ => continue,
            }
            self.make_lost(record);
        }
    }

    /// Ware `record`, carried by a figure of `owner` that is going, comes off `owner`'s ware
    /// count and its record is freed (`0x2c52f`, `0x2c57e`). The ware keeps no order here.
    fn drop_carried(&mut self, owner: crate::world::Player, record: u16) {
        if let Some(kind) = self.world.ware(record).map(|ware| ware.kind) {
            self.world.drop_ware(owner, kind, 1);
        }
        self.world.wares.remove(record);
    }

    /// Road `road` loses its carrier in a door (`0x2c47a`): with no order open it orders a
    /// helper to its start flag; an open order for a pack donkey not yet on its way asks for a
    /// helper instead.
    fn reorder_carrier(&mut self, road: u16) {
        let Some(record) = self.world.road(road).cloned() else {
            return;
        };
        match record.order {
            None => {
                let order = self.world.order_person(record.start, Job::HELPER, ROAD_MODE, 0);
                if let Some(road) = self.world.road_mut(road) {
                    road.order = order;
                }
            }
            Some(order) => {
                if let Some(order) = self.world.orders.get_mut(order)
                    && !order.delivering
                    && order.wanted == Wanted::Person(Job::PACK_DONKEY)
                {
                    order.wanted = Wanted::Person(Job::HELPER);
                }
            }
        }
    }

    /// Settles and frees every order at `flag` but road orders and those tagged to stay
    /// (`0x2fb4b`), in record order. A ware on its way is stranded where it waits, or loses
    /// its way on a carrier. A walker on its way takes the storehouse nearest its road's start
    /// flag as its target, and is lost with no storehouse to go to.
    pub(crate) fn settle_orders(&mut self, flag: u16) {
        let orders: Vec<_> = self
            .world
            .orders()
            .filter(|(_, order)| {
                order.flag == flag
                    && order.mode != ROAD_MODE
                    && (order.mode == WARE_MODE || order.tag != KEPT_TAG)
            })
            .map(|(record, order)| (record, *order))
            .collect();
        for (record, order) in orders {
            if order.delivering
                && let Some(on_way) = order.on_way
            {
                if order.mode == WARE_MODE {
                    self.strand_ordered(on_way);
                } else if let Some(FigureKind::Walker(walker)) =
                    self.world.figure(on_way).map(|figure| figure.kind)
                    && walker.mode != ROAD_MODE
                {
                    let start = self.world.road(walker.on.road).map(|road| road.start);
                    if start.and_then(|start| self.aim_home(on_way, start)).is_none() {
                        self.make_lost(on_way);
                    }
                }
            }
            self.world.release_link(record);
            self.world.orders.remove(record);
        }
    }

    /// The wares waiting at flag `flag`, which is going (`0x2a960`): from the newest, each one
    /// still on its way gives up its order, each comes off the flag owner's ware count
    /// (`0x2aa13`), and every one is freed.
    pub(crate) fn release_wares(&mut self, flag: u16) {
        let owner = self.world.flag(flag).map(|flag| flag.owner);
        let wares: Vec<u16> = self.world.wares_at(flag).map(|(record, _)| record).collect();
        for record in wares {
            let going = self
                .world
                .ware(record)
                .is_some_and(|ware| !matches!(ware.state, WareState::Stranded | WareState::Unrouted));
            if going {
                self.world.give_up_order(record);
            }
            if let (Some(owner), Some(kind)) = (owner, self.world.ware(record).map(|ware| ware.kind)) {
                self.world.drop_ware(owner, kind, 1);
            }
            self.world.wares.remove(record);
        }
    }

    /// Everyone on flag `flag`'s node, which is going, from the head of its list (`0x25d60`): a
    /// settler on the roads or in a doorway drops its job and is lost. Animals, lost settlers and
    /// settlers working or walking cross-country are left alone.
    pub(crate) fn sweep_flag_node(&mut self, flag: u16) {
        let Some(node) = self.world.flag(flag).map(|flag| flag.node) else {
            return;
        };
        let figures: Vec<u16> = self.world.figures_at(node).map(|(record, _)| record).collect();
        for record in figures {
            let left = self.world.figure(record).is_none_or(Figure::spared_by_removal);
            if !left {
                self.lose(record);
            }
        }
    }

    /// Ware `record`, ordered by a building that is going, loses its order: one waiting at a
    /// flag is stranded there, one on a carrier no longer knows where it goes. A waiting ware
    /// takes the flag [`World::holding_flag`] finds as its own. When that search finds it at
    /// none of them, with the bug fixes on it takes the flag whose list holds it, and with them
    /// off none, which leaves it out of the stranded-ware pass for good.
    pub(crate) fn strand_ordered(&mut self, record: u16) {
        let holder = self.world.holding_flag(record).or_else(|| {
            self.world.bug_fixes.on().then(|| {
                self.world
                    .flags()
                    .map(|(flag, _)| flag)
                    .find(|&flag| self.world.wares_at(flag).any(|(held, _)| held == record))
            })?
        });
        let Some(ware) = self.world.wares.get_mut(record) else {
            return;
        };
        match ware.state {
            WareState::Waiting | WareState::Taken => {
                ware.state = WareState::Stranded;
                ware.flag = holder;
            }
            WareState::Carried => ware.state = WareState::Unrouted,
            WareState::Stranded | WareState::Unrouted => {}
        }
        ware.order = None;
    }
}

#[cfg(test)]
mod tests;
