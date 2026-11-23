//! How many wares each player owns, per ware.
//!
//! `S2.EXE` keeps a count per ware and a total for every player, adds to them wherever a ware
//! comes into being and takes off wherever one is used up or thrown away. The counts follow
//! production and consumption rather than storage, so a ware on a flag, in a carrier's hands and
//! in a storehouse all count the same. The statistics screens draw the total and the player's own
//! stock screen draws one count.
//!
//! Every place the original counts a ware in or off has its own call here.

use crate::job::Job;
use crate::people::{JOBS, named_job};
use crate::ware::WareKind;
use crate::world::{Player, World};

/// The entry that names no ware.
const NO_WARE: WareKind = WareKind(u8::MAX);

/// The ware a settler of each job carries (byte 4 of each row of the job table at `0x1fad97`).
/// Only job 37 differs by nation, and the one nation that differs keeps the job in byte 3 as
/// well, which skips the ware, so one copy covers every nation.
#[rustfmt::skip]
const CARRIES: [WareKind; JOBS] = [
    NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE,
    NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE,
    NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, NO_WARE, WareKind::BEER,
    WareKind::BOW, WareKind::SWORD, WareKind::SHIELD, WareKind::SHIELD, WareKind::SHIELD, WareKind::SHIELD,
    WareKind::WOOD, WareKind::BOARDS, WareKind::STONES, WareKind::COINS, WareKind::GOLD, WareKind::IRON_ORE,
    WareKind::COAL, WareKind::STONES, WareKind::IRON, WareKind::FISH, WareKind::GRAIN, NO_WARE,
    WareKind::HAM, WareKind::MEAT, WareKind::FLOUR, WareKind::BREAD, NO_WARE, WareKind::TONGS,
    WareKind::HAMMER, WareKind::AXE, WareKind::PICK_AXE, WareKind::SHOVEL, WareKind::CRUCIBLE,
    WareKind::ROD_AND_LINE, WareKind::SCYTHE, NO_WARE, WareKind::CLEAVER, WareKind::ROLLING_PIN,
    WareKind::SAW, WareKind::MEAT, WareKind::BOAT, WareKind::BOARDS, WareKind::BOAT, WareKind::WATER,
];

/// The ware a settler of `job` loses when it goes lost (`0x36ebc`): byte 4 of its row, read only
/// where byte 3 names a job.
#[must_use]
pub(crate) fn carried(nation: u8, job: Job) -> Option<WareKind> {
    named_job(nation, job)?;
    let (_, column) = crate::people::table_entry(nation, job)?;
    Some(CARRIES[column]).filter(|&ware| ware != NO_WARE)
}

impl World {
    /// How many of `ware` `player` owns (`0x299de`), which the player's own stock screen draws.
    #[must_use]
    pub fn owned_wares(&self, player: Player, ware: WareKind) -> u16 {
        self.owned_wares
            .get(player.index())
            .and_then(|counts| counts.get(ware.index()))
            .copied()
            .unwrap_or_default()
    }

    /// How many wares `player` owns in all (`0x29c21`), which the original keeps as a word of its
    /// own rather than adding the counts up.
    #[must_use]
    pub fn owned_wares_total(&self, player: Player) -> u16 {
        self.owned_total.get(player.index()).copied().unwrap_or_default()
    }

    /// Counts `count` of `ware` off `player` (`0x29991`). `count` is held to what that ware's
    /// count holds, so neither it nor the total can go below zero, and the arrival counters the
    /// merchandise graph draws are left alone.
    pub(crate) fn drop_ware(&mut self, player: Player, ware: WareKind, count: u16) {
        let Some(counts) = self.owned_wares.get_mut(player.index()) else {
            return;
        };
        let Some(held) = counts.get_mut(ware.index()) else {
            return;
        };
        let count = count.min(*held);
        *held -= count;
        let total = &mut self.owned_total[player.index()];
        *total = total.saturating_sub(count);
    }
}

#[cfg(test)]
mod tests;
