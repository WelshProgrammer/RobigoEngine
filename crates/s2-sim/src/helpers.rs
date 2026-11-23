//! New helpers and recruits in each player's storehouses every 240 steps (`0x2728f`).

use crate::game::Game;
use crate::job::Job;
use crate::ware::WareKind;
use crate::world::{BuildingState, Player};

/// The most helpers the pass fills a storehouse to.
const MOST_HELPERS: u16 = 100;

/// The owned nodes that earn each storehouse one more helper per pass.
const LAND_PER_HELPER: usize = 200;

/// What each recruit uses up, in the order the original takes them off the player's count
/// (`0x2739e`).
const RECRUIT_WARES: [WareKind; 3] = [WareKind::SHIELD, WareKind::SWORD, WareKind::BEER];

impl Game {
    /// The helper and recruit pass (`0x2728f`). Each player with a finished storehouse earns
    /// `max(1, land / 200 / storehouses)` helpers per storehouse, filling each to 100 at most.
    /// Before the helpers arrive a storehouse with beer, swords, shields and helpers turns
    /// helpers into privates, one of each ware per private, as many as the scarcest of the four
    /// allows times the recruiting share.
    pub(crate) fn make_helpers(&mut self) {
        for player in Player::ALL {
            let stores = self
                .world
                .buildings()
                .filter(|(_, building)| {
                    building.owner == player
                        && building.is_storehouse()
                        && building.state.contains(BuildingState::FINISHED)
                })
                .count();
            if stores == 0 {
                continue;
            }
            let recruiting = u16::from(self.world.military_settings(player)[crate::military::RECRUITING]);
            let land = self.owned_land(player);
            let per = u16::try_from(land / LAND_PER_HELPER / stores)
                .unwrap_or(u16::MAX)
                .max(1);
            let records: Vec<u16> = self
                .world
                .storehouses()
                .filter(|(_, store)| {
                    self.world
                        .building(store.building)
                        .is_some_and(|building| building.owner == player)
                })
                .map(|(record, _)| record)
                .collect();
            for record in records {
                let Some(store) = self.world.storehouse_mut(record) else {
                    continue;
                };
                let (recruits, helpers) = store.recruit_and_add(per, recruiting);
                for ware in RECRUIT_WARES {
                    self.world.drop_ware(player, ware, recruits);
                }
                self.world.drop_people(player, Job::HELPER, recruits);
                self.world.count_people(player, Job::PRIVATE, recruits);
                self.world.count_people(player, Job::HELPER, helpers);
            }
        }
    }

    /// How many nodes `player` owns.
    fn owned_land(&self, player: Player) -> usize {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        (0..height)
            .flat_map(|y| (0..width).map(move |x| crate::grid::Node { x, y }))
            .filter(|&node| self.nodes.get(node).owner == Some(player))
            .count()
    }
}

impl crate::world::Storehouse {
    /// One storehouse's share of the pass: recruits first, at `recruiting` percent of what the
    /// stock allows, then helpers. A store that would pass 100 helpers gains 100 less what it
    /// held instead of `per`, which leaves it at 100 less the recruits; one already past 100
    /// gains a wrapped word (`0x2746c`). Gives back how many recruits it made and how many helpers it gained, which the
    /// player's people and ware counts follow (`0x273d0`, `0x273df`, `0x273fb` and the three
    /// drops from `0x2739e`).
    fn recruit_and_add(&mut self, per: u16, recruiting: u16) -> (u16, u16) {
        let helpers = self.jobs[Job::HELPER.index()];
        let beer = self.wares[WareKind::BEER.index()];
        let swords = self.wares[WareKind::SWORD.index()];
        let shields = self.wares[WareKind::SHIELD.index()];
        let recruits = if beer > 0 && helpers > 0 && swords.min(shields) > 0 {
            u16::try_from(u32::from(beer.min(swords).min(shields).min(helpers)) * u32::from(recruiting) / 100)
                .unwrap_or(0)
        } else {
            0
        };
        for ware in [WareKind::BEER, WareKind::SWORD, WareKind::SHIELD] {
            self.wares[ware.index()] -= recruits;
        }
        self.jobs[Job::PRIVATE.index()] += recruits;
        let added = if helpers + per > MOST_HELPERS {
            MOST_HELPERS.wrapping_sub(helpers)
        } else {
            per
        };
        self.jobs[Job::HELPER.index()] = (helpers - recruits).wrapping_add(added);
        (recruits, added)
    }
}

#[cfg(test)]
mod tests;
