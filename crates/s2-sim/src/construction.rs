//! Building sites: placing one, the levelling or builder order it starts with, and the
//! construction start.

use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::nodes::{NodeMap, SiteFlags, SiteSize};
use crate::sites::refresh_sites;
use crate::world::{
    Building, BuildingKind, BuildingState, BuildingTypes, FlagKind, LARGE_FOOTPRINT, MapObject, Player, Size,
    WaypointSet, World, connect_to_flag,
};

/// The mode of a builder order: the walker's state 2 at the site's flag.
pub const BUILDER_MODE: u8 = 2;

/// The mode of a planer order: the walker's state 4 at the site's flag.
pub const PLANER_MODE: u8 = 4;

/// Whether a node whose slot 8 offers `offered` takes a building of size `size` (`0x1974b`,
/// `0x1976c`, `0x19877`, `0x1978d`).
#[must_use]
pub const fn fits(size: Size, offered: SiteSize) -> bool {
    match size {
        Size::Small => matches!(offered, SiteSize::Small | SiteSize::Medium | SiteSize::Large),
        Size::Medium => matches!(offered, SiteSize::Medium | SiteSize::Large),
        Size::Large => matches!(offered, SiteSize::Large),
        Size::Mine => matches!(offered, SiteSize::Mine),
    }
}

impl Game {
    /// Plays by building type table `types` from now on.
    pub fn set_building_types(&mut self, types: BuildingTypes) {
        self.world.set_building_types(types);
    }

    /// Makes `player` one of nation `nation`, 0 Romans to 3 Africans, whose buildings play
    /// that nation's work animations.
    pub fn set_nation(&mut self, player: Player, nation: u8) {
        self.world.nations[player.index()] = nation;
    }

    /// Gives `player`'s builders waypoint set `set`.
    pub fn set_waypoints(&mut self, player: Player, set: WaypointSet) {
        self.world.set_waypoints(player, set);
    }
}

impl World {
    /// Places a site of type `kind` for `owner` on `node` (`0x26221`, `0x2c1b4`) and returns its
    /// record. The caller has checked the node. A type 1 flag goes on the south-east neighbour
    /// first when none stands there, splitting any road through it. A large type also covers its
    /// west, north-west and north-east neighbours. A medium or large site on uneven ground orders
    /// a planer; any other starts construction at once. Then the site is linked to its flag and
    /// slot 8 is recomputed two rings round it, twice.
    ///
    /// `None` when the flag or building pool is full; a flag placed before that stays.
    pub fn place_site(
        &mut self,
        nodes: &mut NodeMap,
        node: Node,
        kind: BuildingKind,
        owner: Player,
    ) -> Option<u16> {
        let (width, height) = (self.width(), self.height());
        let flag_node = node.neighbour(Direction::SouthEast, width, height);
        let flag = match self.flag_at(flag_node) {
            Some(flag) => flag,
            None => self.place_flag(nodes, flag_node, FlagKind::Normal, true)?,
        };
        let building = self.add_building(Building::new(kind, owner, node, flag, BuildingState::LEVELLING))?;
        self.set_object(node, MapObject::Building(building));
        self.count_site(owner, kind);
        let row = self.building_types().get(kind)?;
        if row.size == Size::Large {
            for direction in LARGE_FOOTPRINT {
                let at = node.neighbour(direction, width, height);
                let mut data = nodes.get(at);
                data.sites.flags |= SiteFlags::FOOTPRINT;
                nodes.set(at, data);
            }
        }
        if row.levelled && !flat(nodes, node) {
            self.order_person(flag, Job::PLANER, PLANER_MODE, 0);
        } else {
            self.start_construction(building);
        }
        connect_to_flag(nodes, self, node);
        for _ in 0..2 {
            refresh_sites(nodes, self, node, 2);
        }
        Some(building)
    }

    /// The construction start (`0x2c3e8`): the site is under construction with its counts
    /// cleared, and orders its builder.
    pub(crate) fn start_construction(&mut self, building: u16) {
        let Some(site) = self.building_mut(building) else {
            return;
        };
        site.state |= BuildingState::BUILDING;
        site.state -= BuildingState::PRESENT | BuildingState::LEVELLING;
        site.on_way = [0; 4];
        site.stock = [0; 4];
        site.used = [0; 2];
        site.requested = 0;
        let flag = site.flag;
        self.order_person(flag, Job::BUILDER, BUILDER_MODE, 0);
        if let Some(site) = self.building_mut(building) {
            site.requested += 1;
        }
    }
}

/// Whether every neighbour of `node` stands at its altitude (`0x3f6fc`).
fn flat(nodes: &NodeMap, node: Node) -> bool {
    let altitude = nodes.get(node).altitude;
    Direction::ALL.into_iter().all(|direction| {
        let near = node.neighbour(direction, nodes.width(), nodes.height());
        nodes.get(near).altitude == altitude
    })
}

#[cfg(test)]
mod tests;
