//! The routines the settings windows run on each player's economy settings: distribution
//! sliders, tool weights and the transport list.
//!
//! The tables the settings start from are in [`s2_systems::economy`].

use crate::transport::Distribution;
use crate::transport::{TRANSPORT_GROUPS, transport_group};
use crate::ware::WareKind;
use crate::world::{BuildingState, Player, WARE_TYPES, World};

pub use s2_systems::economy::{
    Consumer, DistributionGroup, EconomyTables, EconomyTablesError, MAX_CONSUMERS, MAX_WEIGHT,
};

/// The size of a player's settings record, whose offset is left behind on the stack at start-up
/// (`0x3ce68`).
const SETTINGS_RECORD_SIZE: usize = 1116;

/// The byte the set-up leaves where the rank routine keeps the group it swapped with: the low
/// byte of player `player`'s settings record offset. It names a group only for player 0.
fn leftover_at_start(player: Player) -> u8 {
    (player.index() * SETTINGS_RECORD_SIZE).to_le_bytes()[0]
}

/// The number of transport groups, as the rank routine counts them.
const GROUP_COUNT: u8 = 14;

/// What the transport window's default button leaves where the rank routine keeps the group it
/// swapped with: no group. A Gold save taken after pressing it shows every value as the table
/// gives it, player 0's wood included.
pub(crate) const WINDOW_LEFTOVER: u8 = GROUP_COUNT;

const _: () = assert!(GROUP_COUNT as usize == TRANSPORT_GROUPS);

/// The priority value of a group at rank `rank`: `(15 − rank) × 5`, in 8 bits, higher going
/// first.
const fn priority(rank: u8) -> u8 {
    (GROUP_COUNT + 1).wrapping_sub(rank).wrapping_mul(5)
}

impl World {
    /// `player`'s distribution settings: the weight each building type and building sites give
    /// each ware, and the metalworks' tool weights.
    #[must_use]
    pub fn distribution(&self, player: Player) -> &Distribution {
        &self.distribution[player.index()]
    }

    /// The place of each transport group in `player`'s transport list, 0 first.
    #[must_use]
    pub fn transport_ranks(&self, player: Player) -> &[u8; TRANSPORT_GROUPS] {
        &self.transport_ranks[player.index()]
    }

    /// The transport priority value of each ware type for `player`, higher going first.
    #[must_use]
    pub fn transport_priorities(&self, player: Player) -> &[u8; WARE_TYPES] {
        &self.priorities[player.index()]
    }

    /// Slider `consumer` on page `group` of `player`'s distribution window: the weight the
    /// consumer gives the page's ware. `None` past the pages or the page's consumers.
    #[must_use]
    pub fn slider(&self, player: Player, group: usize, consumer: usize) -> Option<u8> {
        let page = self.economy.groups().get(group)?;
        let ware = *page.wares.first()?;
        let settings = &self.distribution[player.index()];
        Some(match *page.consumers.get(consumer)? {
            Consumer::Building(kind) => settings.weight(kind, ware),
            Consumer::Construction => settings.construction(ware),
        })
    }

    /// Whether building `record`'s window offers to stop and restart its production: a
    /// finished building of a stoppable type.
    #[must_use]
    pub fn offers_stop(&self, record: u16) -> bool {
        self.building(record).is_some_and(|building| {
            building.state.contains(BuildingState::FINISHED)
                && self
                    .building_types()
                    .get(building.kind)
                    .is_some_and(|row| row.stoppable)
        })
    }

    /// Sets slider `consumer` on page `group` of `player`'s distribution window to `weight`
    /// (`0x3cfd2`), then writes every slider on the page into the weights of each of the page's
    /// wares (`0x3d157`). `None` past the pages or the page's consumers.
    pub(crate) fn set_slider(
        &mut self,
        player: Player,
        group: usize,
        consumer: usize,
        weight: u8,
    ) -> Option<()> {
        let mut sliders = [0; MAX_CONSUMERS];
        let count = self.economy.groups().get(group)?.consumers.len();
        for (index, slider) in sliders.iter_mut().enumerate().take(count) {
            *slider = self.slider(player, group, index)?;
        }
        *sliders.get_mut(consumer).filter(|_| consumer < count)? = weight;
        self.write_sliders(player, group, sliders);
        Some(())
    }

    /// Puts page `group` of `player`'s distribution window back to the type weights, and a
    /// site's weight to the table's (`0x3cb82` with a group, then `0x3d157`). `None` past the
    /// pages.
    pub(crate) fn reset_sliders(&mut self, player: Player, group: usize) -> Option<()> {
        let page = self.economy.groups().get(group)?;
        let mut sliders = [0; MAX_CONSUMERS];
        for (slider, &consumer) in sliders.iter_mut().zip(page.consumers.iter()) {
            *slider = match consumer {
                Consumer::Building(kind) => self.economy.type_weight(kind),
                Consumer::Construction => self.economy.construction(),
            };
        }
        self.write_sliders(player, group, sliders);
        Some(())
    }

    /// Writes `sliders` into the weights each consumer on page `group` gives each of the page's
    /// wares (`0x3d157`).
    fn write_sliders(&mut self, player: Player, group: usize, sliders: [u8; MAX_CONSUMERS]) {
        let Some(page) = self.economy.groups().get(group) else {
            return;
        };
        let settings = &mut self.distribution[player.index()];
        for (&consumer, weight) in page.consumers.iter().zip(sliders) {
            for &ware in page.wares.iter() {
                match consumer {
                    Consumer::Building(kind) => settings.set_weight(kind, ware, weight),
                    Consumer::Construction => settings.set_construction(ware, weight),
                }
            }
        }
    }

    /// Sets the weight `player`'s metalworks give tool `tool` (`0x3cf99`). `None` for a ware
    /// that is not a tool.
    pub(crate) fn set_tool_weight(&mut self, player: Player, tool: WareKind, weight: u8) -> Option<()> {
        *self.distribution[player.index()]
            .tools_mut()
            .get_mut(tool.tool()?)? = weight;
        Some(())
    }

    /// Puts `player`'s tool weights back to the table's (`0x3ce2c`).
    pub(crate) fn reset_tool_weights(&mut self, player: Player) {
        *self.distribution[player.index()].tools_mut() = *self.economy.tools();
    }

    /// Moves group `group` to place `rank` in `player`'s transport list one place at a time, as
    /// the window's up and down buttons do (`0x5dc9d`, `0x5dce6`). `None` past the groups or
    /// the places.
    pub(crate) fn move_transport(&mut self, player: Player, group: u8, rank: u8) -> Option<()> {
        let from = *self.transport_ranks[player.index()].get(usize::from(group))?;
        if usize::from(rank) >= TRANSPORT_GROUPS {
            return None;
        }
        let mut at = from;
        while at != rank {
            at = if rank < at { at - 1 } else { at + 1 };
            self.rank_transport(player, group, at, GROUP_COUNT);
        }
        Some(())
    }

    /// Gives every player the settings the tables start a game with (`0x3ce68`).
    pub(crate) fn reset_economy(&mut self) {
        for player in Player::ALL {
            self.distribution[player.index()] = Distribution::new(&self.economy);
            self.reset_transport(player, leftover_at_start(player));
        }
    }

    /// Puts group `group` at place `rank` in `player`'s transport list (`0x3d012`), swapping
    /// places with the group there, and writes the priority value of both groups' wares.
    ///
    /// `leftover` is what the routine's swap local holds from before. When the group already has
    /// that place the swap is skipped and the local keeps it, but the second write still goes to
    /// the wares of the group it names, with the first group's value. Returns what the local
    /// holds afterwards.
    pub(crate) fn rank_transport(&mut self, player: Player, group: u8, rank: u8, leftover: u8) -> u8 {
        let ranks = &mut self.transport_ranks[player.index()];
        let Some(&old) = ranks.get(usize::from(group)) else {
            return leftover;
        };
        let mut other = leftover;
        if old != rank {
            ranks[usize::from(group)] = rank;
            other = (0..)
                .zip(ranks.iter_mut())
                .find(|(index, place)| *index != group && **place == rank)
                .map_or(GROUP_COUNT, |(index, place)| {
                    *place = old;
                    index
                });
        }
        let values = &mut self.priorities[player.index()];
        for (ware, value) in (0..).zip(values.iter_mut()).take(WARE_TYPES) {
            let of = transport_group(WareKind(ware));
            if of == Some(group) {
                *value = priority(rank);
            }
            if of == Some(other) {
                *value = priority(old);
            }
        }
        other
    }

    /// Puts `player`'s transport list back to the table's order (`0x3cdd5`): every rank is
    /// written first, then each group is ranked in turn, which never swaps and so writes the
    /// value of each group in turn to the wares of the group `leftover` names. Returns what the
    /// swap local holds afterwards.
    pub(crate) fn reset_transport(&mut self, player: Player, leftover: u8) -> u8 {
        let ranks = *self.economy.ranks();
        self.transport_ranks[player.index()] = ranks;
        (0..).zip(ranks).fold(leftover, |leftover, (group, rank)| {
            self.rank_transport(player, group, rank, leftover)
        })
    }
}

#[cfg(test)]
mod tests;
