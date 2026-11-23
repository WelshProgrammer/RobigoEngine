//! Workplaces: the worker a finished workplace orders, its walk in through the door, the
//! lookout tower's view once it is manned, and the productivity rating.

use crate::figures::FigureKind;
use crate::game::Game;
use crate::grid::Direction;
use crate::job::Job;
use crate::settlers::DOOR_SPEED;
use crate::world::{BUILDING_TYPES, Building, BuildingKind, BuildingState, Category, Player};

/// The mode of a workplace's worker order.
pub(crate) const WORKER_MODE: u8 = 0;

/// A rating at or above this counts as 100 (`0x2dede`).
const FULL_FROM: u16 = 97;

/// The building types whose averages the average pass stores, in its order (`0x1fa8a8`). The
/// [`BuildingKind::DONKEY_BREEDER`] and the [`BuildingKind::IRON_SMELTER`] are missing, so theirs
/// are never stored.
#[rustfmt::skip]
const AVERAGED: [BuildingKind; 20] = [
    BuildingKind::WOODCUTTER, BuildingKind::QUARRY, BuildingKind::FISHERY, BuildingKind::HUNTER,
    BuildingKind::SAWMILL, BuildingKind::MILL, BuildingKind::BAKERY, BuildingKind::WELL, BuildingKind::FARM,
    BuildingKind::PIG_FARM, BuildingKind::SLAUGHTERHOUSE, BuildingKind::METALWORKS, BuildingKind::ARMOURY,
    BuildingKind::MINT, BuildingKind::BREWERY, BuildingKind::SHIPYARD, BuildingKind::GOLD_MINE,
    BuildingKind::IRON_MINE, BuildingKind::COAL_MINE, BuildingKind::GRANITE_MINE,
];

impl Game {
    /// Worker walker `record` has reached its workplace's flag (`0x37a50`, `0x37cb4`): it turns
    /// to walk in through the door, at the fixed door speed.
    pub(crate) fn settle_worker(&mut self, record: u16, owner: Player, job: Job, order: u16) {
        let Some(node) = self.world.figure(record).map(crate::figures::Figure::node) else {
            return;
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Settling { owner, job, order };
        }
        let door = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        self.world.move_figure(record, door);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, DOOR_SPEED);
        }
    }

    /// A worker has walked in through its workplace's door (`0x3596c`, `0x35cf8`): the building
    /// is manned with its worker present and its door shut, a rated type starts from the
    /// player's average productivity, the order is freed and the figure is gone. With no
    /// building to enter it is lost.
    pub(crate) fn step_settling(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Settling { owner, order, job } = figure.kind else {
            return;
        };
        let building = self
            .world
            .order(order)
            .and_then(|order| self.world.building_at_flag(order.flag))
            .or_else(|| self.world.object(figure.node()).building());
        let Some(building) = building else {
            self.make_lost(record);
            return;
        };
        if self
            .world
            .building(building)
            .is_some_and(crate::world::Building::is_storehouse)
        {
            self.settle_in_store(record, building, order);
            return;
        }
        let row = self
            .world
            .building(building)
            .and_then(|site| self.world.building_types().get(site.kind));
        if row.is_some_and(|row| row.category == Category::Military) {
            self.soldier_arrives(record, building, job, order);
            return;
        }
        let rated = row.is_some_and(|row| row.rated);
        if let Some(row) = row.filter(|row| row.sight > 0)
            && let Some(site) = self.world.building(building)
        {
            self.nodes.reveal(site.node, owner, u16::from(row.sight));
        }
        let averages = self.world.averages;
        if let Some(site) = self.world.building_mut(building) {
            site.state -= BuildingState::DOOR;
            site.state |= BuildingState::MANNED | BuildingState::PRESENT;
            if rated && let Some(&average) = averages[owner.index()].get(site.kind.index()) {
                site.productivity = average;
            }
        }
        self.world.orders.remove(order);
        self.world.remove_figure(record);
    }

    /// A settler has walked into storehouse `building` for order `order`, as a new pack donkey
    /// does (`0x35d83`): it joins the stock, the order is freed and the figure is gone. The door
    /// is left as it is. For an expedition's order a builder counts as collected and anyone else
    /// is not counted at all (`0x35dc5`).
    fn settle_in_store(&mut self, record: u16, building: u16, order: u16) {
        let Some(FigureKind::Settling { job, .. }) = self.world.figure(record).map(|figure| figure.kind)
        else {
            return;
        };
        let expedition = self
            .world
            .order(order)
            .is_some_and(|order| order.tag == crate::supply::EXPEDITION_TAG);
        if expedition {
            if job == crate::job::Job::BUILDER
                && let Some(harbour) = self
                    .world
                    .harbour_of(building)
                    .and_then(|harbour| self.world.harbours.get_mut(harbour))
            {
                harbour.builders = harbour.builders.wrapping_add(1);
            }
        } else if let Some((store, _)) = self.world.storehouse_of(building)
            && let fixes = self.world.bug_fixes
            && let Some(store) = self.world.storehouse_mut(store)
        {
            store.take_in(job, fixes);
        }
        self.world.orders.remove(order);
        self.world.remove_figure(record);
    }

    /// Rates workplace `record`'s productivity, every 960 steps in the building tick
    /// (`0x2dede`): two thirds of the old rating plus the tally of work since, and 100 from 97 up.
    /// The tally starts again from 0.
    pub(crate) fn rate_productivity(&mut self, record: u16) {
        if let Some(building) = self.world.building_mut(record) {
            let rating = (3 * u16::from(building.tally) + 2 * u16::from(building.productivity)) / 3;
            building.productivity = if rating >= FULL_FROM {
                100
            } else {
                u8::try_from(rating).unwrap_or(u8::MAX)
            };
            building.tally = 0;
        }
    }

    /// The average pass (`0x2fe8a`), every 400 steps: for each player and rated building type,
    /// the running mean of its buildings' productivity in record order, each new one halving
    /// the distance. Sites are left out. A type the player has none of averages 0.
    pub(crate) fn average_productivity(&mut self) {
        let mut sums: [[Option<u8>; BUILDING_TYPES]; Player::COUNT] = [[None; BUILDING_TYPES]; Player::COUNT];
        for (_, building) in self.world.buildings() {
            if building
                .state
                .intersects(BuildingState::BUILDING | BuildingState::LEVELLING)
                || !self.rated(building)
            {
                continue;
            }
            let Some(sum) = sums[building.owner.index()].get_mut(building.kind.index()) else {
                continue;
            };
            *sum = Some(match *sum {
                None => building.productivity,
                Some(mean) => mean.midpoint(building.productivity),
            });
        }
        for (averages, sums) in self.world.averages.iter_mut().zip(&sums) {
            for kind in AVERAGED.map(BuildingKind::index) {
                averages[kind] = sums[kind].unwrap_or(0);
            }
        }
    }

    /// Whether `building`'s type is a rated workplace.
    fn rated(&self, building: &Building) -> bool {
        self.world
            .building_types()
            .get(building.kind)
            .is_some_and(|row| row.rated && row.category == Category::Workplace)
    }
}

#[cfg(test)]
mod tests;
