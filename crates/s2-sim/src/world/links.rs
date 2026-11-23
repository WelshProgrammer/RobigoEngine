//! A check that every record the world holds points at records that exist, for soak tests and
//! fuzzing. `S2.EXE` keeps the same links; a broken one here is a bug in Robigo.

use crate::figures::{Exploring, FigureKind, PlanerWork, SoldierState, Trip};
use crate::transport::{Wanted, WareState};
use crate::world::{BuildingKind, MapObject, World};

impl World {
    /// Every link between records that points nowhere, or disagrees with the record at the
    /// other end, described in a line each. Empty when the world is whole.
    #[must_use]
    pub fn broken_links(&self) -> Vec<String> {
        let mut broken = Vec::new();
        self.check_flags(&mut broken);
        self.check_roads(&mut broken);
        self.check_wares(&mut broken);
        self.check_orders(&mut broken);
        self.check_buildings(&mut broken);
        self.check_figures(&mut broken);
        self.check_soldiers(&mut broken);
        self.check_ships(&mut broken);
        broken
    }

    fn check_flags(&self, broken: &mut Vec<String>) {
        for (record, flag) in self.flags() {
            if self.object(flag.node) != MapObject::Flag(record) {
                broken.push(format!("flag {record} is not on its node {:?}", flag.node));
            }
            for link in flag.links.iter().flatten() {
                if self.road(link.road).is_none() {
                    broken.push(format!("flag {record} links to missing road {}", link.road));
                }
            }
            let mut count = 0;
            let mut next = flag.wares;
            while let Some(ware) = next {
                count += 1;
                if count > usize::from(u8::MAX) {
                    broken.push(format!("flag {record}'s ware list loops"));
                    break;
                }
                match self.ware(ware) {
                    Some(held)
                        if matches!(
                            held.state,
                            WareState::Waiting | WareState::Stranded | WareState::Taken
                        ) =>
                    {
                        next = held.link;
                    }
                    Some(held) => {
                        broken.push(format!(
                            "flag {record} lists ware {ware}, which is {:?}",
                            held.state
                        ));
                        break;
                    }
                    None => {
                        broken.push(format!("flag {record} lists missing ware {ware}"));
                        break;
                    }
                }
            }
            if count != usize::from(flag.waiting) {
                broken.push(format!(
                    "flag {record} counts {} waiting but lists {count}",
                    flag.waiting
                ));
            }
        }
    }

    fn check_roads(&self, broken: &mut Vec<String>) {
        for (record, road) in self.roads() {
            for flag in std::iter::once(road.start).chain(road.end) {
                if self.flag(flag).is_none() {
                    broken.push(format!("road {record} ends at missing flag {flag}"));
                }
            }
            if let Some(order) = road.order.filter(|&order| self.order(order).is_none()) {
                broken.push(format!("road {record} holds missing order {order}"));
            }
        }
    }

    fn check_wares(&self, broken: &mut Vec<String>) {
        for (record, ware) in self.wares() {
            let at_flag = matches!(
                ware.state,
                WareState::Waiting | WareState::Stranded | WareState::Taken
            );
            if at_flag && ware.flag.is_none_or(|flag| self.flag(flag).is_none()) {
                broken.push(format!("ware {record} waits at missing flag {:?}", ware.flag));
            }
            let travelling = matches!(ware.state, WareState::Waiting | WareState::Carried);
            if let Some(order) = ware
                .order
                .filter(|&order| travelling && self.order(order).is_none())
            {
                broken.push(format!("ware {record} is for missing order {order}"));
            }
        }
    }

    fn check_orders(&self, broken: &mut Vec<String>) {
        for (record, order) in self.orders() {
            if self.flag(order.flag).is_none() {
                broken.push(format!("order {record} is at missing flag {}", order.flag));
            }
            let Some(on_way) = order.on_way.filter(|_| order.delivering) else {
                continue;
            };
            let found = match order.wanted {
                Wanted::Ware(_) => self.ware(on_way).is_some(),
                Wanted::Person(_) => self.figure(on_way).is_some(),
            };
            if !found {
                broken.push(format!("order {record}'s delivery {on_way} is missing"));
            }
        }
    }

    fn check_buildings(&self, broken: &mut Vec<String>) {
        for (record, building) in self.buildings() {
            if self.flag(building.flag).is_none() {
                broken.push(format!("building {record} has missing flag {}", building.flag));
            }
            if self.object(building.node) != MapObject::Building(record) {
                broken.push(format!(
                    "building {record} is not on its node {:?}",
                    building.node
                ));
            }
            if building.is_storehouse() && !building.is_site() && self.storehouse_of(record).is_none() {
                broken.push(format!("storehouse {record} has no stock record"));
            }
        }
    }

    fn check_figures(&self, broken: &mut Vec<String>) {
        for (record, figure) in self.figures() {
            let loose = matches!(figure.kind, FigureKind::Builder(_));
            if !loose && !self.figures_at(figure.node()).any(|(listed, _)| listed == record) {
                broken.push(format!(
                    "figure {record} ({:?}) is missing from its node's list",
                    figure.kind
                ));
            }
            let path = match figure.kind {
                FigureKind::Gatherer(gatherer) => match gatherer.trip {
                    Trip::Leaving { path } | Trip::Walking { path } => Some(path),
                    _ => None,
                },
                FigureKind::Explorer(explorer) => match explorer.work {
                    Exploring::Walking { path } => Some(path),
                    _ => None,
                },
                FigureKind::Planer(planer) => match planer.work {
                    PlanerWork::Walking { path } => Some(path),
                    PlanerWork::Digging { .. } => None,
                },
                FigureKind::Homeward { path, .. } => Some(path),
                FigureKind::Soldier(soldier) => match soldier.state {
                    SoldierState::Leaving { path, .. } | SoldierState::Walking { path, .. } => path,
                    _ => None,
                },
                _ => None,
            };
            if let Some(path) = path.filter(|&path| self.paths.get(path).is_none()) {
                broken.push(format!("figure {record} walks missing path {path}"));
            }
            let ware = match figure.kind {
                FigureKind::Carrier(carrier) => carrier.ware,
                FigureKind::Delivering(delivery) => delivery.ware,
                FigureKind::Bringing { ware, .. } => ware,
                _ => None,
            };
            if let Some(ware) = ware.filter(|&ware| self.ware(ware).is_none()) {
                broken.push(format!("figure {record} carries missing ware {ware}"));
            }
            if let FigureKind::Carrier(carrier) = figure.kind
                && self.road(carrier.on.road).is_none()
            {
                broken.push(format!("carrier {record} is on missing road {}", carrier.on.road));
            }
        }
    }

    fn check_soldiers(&self, broken: &mut Vec<String>) {
        for (record, held) in self.soldiers.iter() {
            let back = match self.figure(held.figure).map(|figure| figure.kind) {
                Some(FigureKind::Soldier(soldier)) => soldier.record,
                _ => None,
            };
            if back != Some(record) {
                broken.push(format!(
                    "soldier record {record} names figure {}, which does not name it back",
                    held.figure
                ));
            }
        }
        for (record, figure) in self.figures() {
            if let FigureKind::Soldier(soldier) = figure.kind
                && let Some(held) = soldier.record
                && self.soldiers.get(held).is_none_or(|held| held.figure != record)
            {
                broken.push(format!(
                    "soldier {record} names soldier record {held}, which is not its own"
                ));
            }
        }
    }

    fn check_ships(&self, broken: &mut Vec<String>) {
        for (record, ship) in self.ships() {
            let back = match self.figure(ship.figure).map(|figure| figure.kind) {
                Some(FigureKind::Ship { record, .. }) => Some(record),
                _ => None,
            };
            if back != Some(record) {
                broken.push(format!(
                    "ship {record} names figure {}, which does not name it back",
                    ship.figure
                ));
            }
            if let Some(path) = ship.path.filter(|&path| self.paths.get(path).is_none()) {
                broken.push(format!("ship {record} sails missing path {path}"));
            }
            if ship.sailing && ship.path.is_none() {
                broken.push(format!("ship {record} sails with no path"));
            }
        }
        for (record, figure) in self.figures() {
            if let FigureKind::Ship { record: ship, .. } = figure.kind
                && self.ships.get(ship).is_none_or(|held| held.figure != record)
            {
                broken.push(format!(
                    "ship figure {record} names ship {ship}, which is not its own"
                ));
            }
        }
        for (record, harbour) in self.harbours() {
            if self
                .building(harbour.building)
                .is_none_or(|building| building.kind != BuildingKind::HARBOUR)
            {
                broken.push(format!(
                    "harbour {record} names building {}, which is no harbour",
                    harbour.building
                ));
            }
        }
        for (record, order) in self.orders() {
            if let Some(harbour) = order
                .harbour
                .filter(|&harbour| self.harbours.get(harbour).is_none())
            {
                broken.push(format!("order {record} is linked to missing harbour {harbour}"));
            }
        }
    }
}
