//! A shipyard building ships: the site search, the shipwright's trips to the site and the stages
//! of the ship there.

use crate::game::Game;
use crate::gather::{Find, Search};
use crate::grid::Node;
use crate::ground::Property;
use crate::job::Job;
use crate::world::{MapObject, Player, ShipSite};

/// Search kind 16 (`0x1fa66a`): a node on the shore within 8, in the shipyard's area.
const SHIP_SITE_SEARCH: Search = Search {
    find: Find::ShipSite,
    centre: false,
    radius: 8,
    hits: 5,
    same_area: true,
};

/// The shipwright's work animation on the site (`0x339b8`).
pub(crate) const SHIPBUILDING: u16 = 3476;

impl Game {
    /// Starts work at shipyard `record` (`0x2f977`). Building boats, or in ship mode while it
    /// looks for a site, it starts as any workplace does, using a board for a cycle that makes
    /// nothing in ship mode. With a site and a board, the shipwright takes the board out to the
    /// site. A site it cannot reach is looked for again. Finding no site switches it back to
    /// boats.
    pub(crate) fn start_shipyard(&mut self, record: u16) -> bool {
        let Some(yard) = self.world.building(record).copied() else {
            return false;
        };
        if !yard.ships {
            return self.start_generic(record);
        }
        match yard.ship_site {
            None => {
                let hits = self.search_round(record, SHIP_SITE_SEARCH);
                let site = self.pick(&hits);
                if let Some(yard) = self.world.building_mut(record) {
                    yard.ship_site = site;
                    yard.ships = site.is_some();
                }
            }
            Some(site) => {
                if yard.stock[0] == 0 {
                    return false;
                }
                if self.send_out(record, Job::SHIPWRIGHT_OUT, site) {
                    if let Some(yard) = self.world.building_mut(record) {
                        yard.stock[0] -= 1;
                    }
                    return true;
                }
                let hits = self.search_round(record, SHIP_SITE_SEARCH);
                if let Some(site) = self.pick(&hits)
                    && let Some(yard) = self.world.building_mut(record)
                {
                    yard.ship_site = Some(site);
                }
            }
        }
        self.start_generic(record)
    }

    /// Whether a ship can be built on `node` (`0x2e7bf`): not all water, open to figures, no
    /// flag, and a neighbour that is all water. Whatever object stands there is not looked at.
    pub(crate) fn ship_site_fits(&self, node: Node) -> bool {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        !self.nodes.property(node).contains(Property::ALL_WATER)
            && self.nodes.get(node).sites.passable()
            && self.world.flag_at(node).is_none()
            && crate::homeward::RING.into_iter().any(|direction| {
                self.nodes
                    .property(node.neighbour(direction, width, height))
                    .contains(Property::ALL_WATER)
            })
    }

    /// A shipwright of `owner` from shipyard `yard` has worked on `node` (`0x384e1`): a part-built
    /// ship there gets its next part, and anything else is replaced by a new one. The thirteenth
    /// trip finishes the ship, which floats, and the shipyard looks for a new site.
    pub(crate) fn add_ship_part(&mut self, yard: u16, owner: Player, node: Node) {
        let next = match self.world.object(node) {
            MapObject::ShipSite(site) => site.next(),
            _ => Some(ShipSite::NEW),
        };
        if let Some(site) = next {
            self.world.set_object(node, MapObject::ShipSite(site));
            return;
        }
        self.world.set_object(node, MapObject::Nothing);
        self.launch_ship(node, owner);
        if let Some(yard) = self.world.building_mut(yard) {
            yard.ship_site = None;
        }
    }
}

#[cfg(test)]
mod tests;
