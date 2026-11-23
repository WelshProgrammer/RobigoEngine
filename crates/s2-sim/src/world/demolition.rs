//! What a demolished building leaves behind, and the pass that ages fires, their remains and
//! skeletons.

use super::{Ageing, BuildingState, Burning, LARGE_FOOTPRINT, MapObject, Size, World, disconnect_from_flag};
use crate::grid::Node;
use crate::nodes::{NodeMap, SiteFlags};
use crate::sites::refresh_sites;

impl World {
    /// Frees building `record` and what it covers (`0x2c9f7` steps 3 to 13): a large one's
    /// footprint, its storehouse record, the record itself, its object word and its link to the
    /// flag. A finished building leaves a fire. The node and two rings are recomputed twice.
    ///
    /// The player's statistics are not kept yet, so the wares the building held simply go.
    pub(crate) fn remove_building(&mut self, nodes: &mut NodeMap, record: u16) {
        let Some(building) = self.building(record).copied() else {
            return;
        };
        let (width, height) = (self.width, self.height);
        let size = self.building_types().get(building.kind).map(|row| row.size);
        if size == Some(Size::Large) {
            for direction in LARGE_FOOTPRINT {
                let at = building.node.neighbour(direction, width, height);
                let mut data = nodes.get(at);
                data.sites.flags -= SiteFlags::FOOTPRINT;
                nodes.set(at, data);
            }
        }
        if let Some((store, _)) = self.storehouse_of(record) {
            self.stores.remove(store);
        }
        self.buildings.remove(record);
        self.set_object(building.node, MapObject::Nothing);
        disconnect_from_flag(nodes, self, building.node);
        if building.state.contains(BuildingState::FINISHED) {
            let large = matches!(size, Some(Size::Medium | Size::Large));
            self.set_object(building.node, MapObject::Fire(Burning::fire(large)));
        }
        for _ in 0..2 {
            refresh_sites(nodes, self, building.node, 2);
        }
    }

    /// The ageing pass over map row `row` (`0x3dae8`, first part). A fire counts down, then
    /// becomes remains; remains count down, then go, and the node and its ring are recomputed
    /// twice. A skeleton ages, takes its second look, ages again and goes. A row past the map's
    /// height does nothing.
    pub(crate) fn age_row(&mut self, nodes: &mut NodeMap, row: u16) {
        if row >= self.height {
            return;
        }
        for x in 0..self.width {
            let node = Node { x, y: row };
            match self.object(node).age() {
                Ageing::Unchanged => {}
                Ageing::Becomes(object) => self.set_object(node, object),
                Ageing::Gone {
                    refresh_sites: refresh,
                } => {
                    self.set_object(node, MapObject::Nothing);
                    if refresh {
                        for _ in 0..2 {
                            refresh_sites(nodes, self, node, 1);
                        }
                    }
                }
            }
        }
    }
}
