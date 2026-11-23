//! The tool pass (`0x3d23b`): every 480 cycles each computer player's
//! tool weights follow how short it is of each tool.

use crate::game::Game;
use crate::job::Job;
use crate::ware::WareKind;
use crate::world::{BuildingKind, Category, JOB_TOOLS, Player};

/// The pass runs when the cycle counter plus this is a multiple of [`TOOL_PERIOD`] (`0x270b8`).
pub(super) const TOOL_OFFSET: u16 = 7;
pub(super) const TOOL_PERIOD: u16 = 480;

/// The jobs the pass looks at: 1 to 28, job 0 left out (`0x3d2ae`).
const FIRST_JOB: u8 = 1;
const LAST_JOB: u8 = 28;

/// The building types the pass looks at, 0 to 31 (`0x3d270`).
const BUILDING_TYPES: u8 = 32;

impl Game {
    /// Sets `player`'s tool weights from its shortages (`0x3d23b`). A tool's shortage is the
    /// finished buildings and sites whose workers need it, less the people with those jobs and
    /// the tools in stock, when above 0. With none short nothing changes; otherwise each weight
    /// becomes its shortage over the largest, out of 100. As in `S2.EXE` the largest is kept as
    /// the low byte of its word, and each weight as the low byte of its quotient.
    pub(crate) fn tool_pass(&mut self, player: Player) {
        let mut shortages = [0u16; WareKind::TOOLS];
        for (slot, shortage) in shortages.iter_mut().enumerate() {
            let tool = WareKind(WareKind::FIRST_TOOL.0 + u8::try_from(slot).unwrap_or_default());
            let stock = u32::from(self.world.owned_wares(player, tool));
            let (mut held, mut need) = (0u32, 0u32);
            for job in (FIRST_JOB..=LAST_JOB).map(Job) {
                if JOB_TOOLS.get(job.index()).copied().flatten() != Some(tool) {
                    continue;
                }
                for kind in (0..BUILDING_TYPES).map(BuildingKind) {
                    let Some(row) = self.world.building_types().get(kind) else {
                        continue;
                    };
                    if row.worker == Some(job) && row.category == Category::Workplace {
                        need += u32::from(self.world.finished_buildings(player, kind))
                            + u32::from(self.world.building_sites(player, kind));
                    }
                }
                held += u32::from(self.world.people(player, job));
            }
            if need > held + stock {
                *shortage = u16::try_from((need - held - stock) & u32::from(u16::MAX)).unwrap_or_default();
            }
        }
        let mut largest = 0u8;
        for &shortage in &shortages {
            if shortage > u16::from(largest) {
                largest = shortage.to_le_bytes()[0];
            }
        }
        if largest == 0 {
            return;
        }
        let divisor = u32::from(largest) * 10;
        for (slot, &shortage) in shortages.iter().enumerate() {
            let weight = (u32::from(shortage) * 1000 / divisor).to_le_bytes()[0];
            let tool = WareKind(WareKind::FIRST_TOOL.0 + u8::try_from(slot).unwrap_or_default());
            let _ = self.world.set_tool_weight(player, tool, weight);
        }
    }
}

#[cfg(test)]
mod tests;
