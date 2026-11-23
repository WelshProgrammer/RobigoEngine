//! Mines and wells: workplaces that take from slot 11 under or next to the building and work
//! inside.

use crate::game::Game;
use crate::gather::{Find, Search};
use crate::nodes::Mineral;
use crate::world::{BuildingKind, BuildingState};

/// The mineral a mine type digs (the jump table at `0x2f667`).
const fn mineral(kind: BuildingKind) -> Option<Mineral> {
    match kind {
        BuildingKind::GOLD_MINE => Some(Mineral::Gold),
        BuildingKind::IRON_MINE => Some(Mineral::IronOre),
        BuildingKind::COAL_MINE => Some(Mineral::Coal),
        BuildingKind::GRANITE_MINE => Some(Mineral::Granite),
        _ => None,
    }
}

/// Search kinds 11 to 15: the building's node and the two rings round it, any area.
const fn resource_search(find: Find) -> Search {
    Search {
        find,
        centre: true,
        radius: 2,
        hits: 1,
        same_area: false,
    }
}

/// The food a mine needs in stock to start.
const FOOD_NEEDED: u8 = 1;

impl Game {
    /// Starts work at mine `record` (`0x2f677`), unless it has given up: with a food in stock,
    /// ore of its mineral under or near it. It takes one from the node drawn, then the first
    /// food in stock of fish, bread and meat. A mine that finds none gives up for good and
    /// keeps its food.
    pub(crate) fn start_mine(&mut self, record: u16) -> bool {
        let Some(mine) = self.world.building(record).copied() else {
            return false;
        };
        if mine.exhausted {
            return false;
        }
        let inputs = self.input_count(mine.kind);
        let food: u8 = mine.stock[..inputs].iter().sum();
        if food < FOOD_NEEDED {
            return false;
        }
        let Some(mineral) = mineral(mine.kind) else {
            return false;
        };
        let hits = self.search_round(record, resource_search(Find::Mineral(mineral)));
        let Some(target) = self.pick(&hits) else {
            self.give_up(record);
            self.post_message(
                crate::messages::MessageId::MINE_DRY,
                mine.owner,
                Some(mine.node),
                Some(mine.kind),
                0,
            );
            return false;
        };
        self.take_resource(target);
        let mut eaten = None;
        if let Some(mine) = self.world.building_mut(record) {
            if let Some(slot) = mine.stock[..inputs].iter().position(|&stock| stock > 0) {
                mine.stock[slot] -= 1;
                eaten = Some(slot);
            }
            mine.state |= BuildingState::WORKING;
        }
        if let Some(ware) = eaten.and_then(|slot| {
            self.world
                .building_types()
                .get(mine.kind)
                .and_then(|row| row.inputs[slot])
        }) {
            self.world.drop_ware(mine.owner, ware, 1);
        }
        self.start_work_animation(record);
        true
    }

    /// Starts work at well `record` (`0x2f87f`), unless it has given up: water under or near
    /// it. The water drawn is never used up. A well that finds none gives up for good.
    pub(crate) fn start_well(&mut self, record: u16) -> bool {
        if self.world.building(record).is_none_or(|well| well.exhausted) {
            return false;
        }
        let hits = self.search_round(record, resource_search(Find::Water));
        if self.pick(&hits).is_none() {
            self.give_up(record);
            if let Some(well) = self.world.building(record).copied() {
                self.post_message(
                    crate::messages::MessageId::WELL_DRY,
                    well.owner,
                    Some(well.node),
                    Some(well.kind),
                    0,
                );
            }
            return false;
        }
        self.start_work_animation(record);
        if let Some(well) = self.world.building_mut(record) {
            well.state |= BuildingState::WORKING;
        }
        true
    }

    /// The input slots building type `kind` has, up to the first empty one.
    fn input_count(&self, kind: BuildingKind) -> usize {
        self.world.building_types().get(kind).map_or(0, |row| {
            row.inputs.iter().take_while(|input| input.is_some()).count()
        })
    }

    /// One taken from slot 11 at `node` (`0x3dea8`).
    pub(crate) fn take_resource(&mut self, node: crate::grid::Node) {
        let mut data = self.nodes.get(node);
        data.resources.take_one();
        self.nodes.set(node, data);
    }
}

#[cfg(test)]
mod tests;
