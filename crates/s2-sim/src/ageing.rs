//! The ageing pass's counters (`0x3dae8`): which map rows each of its three parts visits next.
//! Every counter starts at 0 when a map loads (`0x25447`).

use crate::game::Game;

/// Growth runs on every this many steps.
const GROWTH_EVERY: u16 = 2;

/// [`Ageing::gate`]'s bit that raises the third part's threshold from 1 to 2.
const GATE_RAISED: u16 = 1 << 15;

/// The low bits of [`Ageing::gate`] that count turns.
const GATE_COUNT: u16 = (1 << 4) - 1;

/// Where the ageing pass is in each of its three parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Ageing {
    /// The next row for fires, remains, skeletons and stumps (`0x20173e`), every step.
    pub(crate) row: u8,
    /// Steps since growth last ran (`0x201740`).
    pub(crate) growth_wait: u16,
    /// The next row for growth (`0x201742`).
    pub(crate) growth_row: u8,
    /// The third part's gate (`0x201744`): it runs on two of every three of growth's turns.
    pub(crate) gate: u16,
    /// The next row for fields and signs (`0x201746`).
    pub(crate) marks_row: u8,
}

impl Game {
    /// One step of the ageing pass (`0x3dae8`). Each part's row counter wraps at 256, so a map
    /// shorter than that has empty turns.
    pub(crate) fn age(&mut self) {
        self.world.age_row(&mut self.nodes, u16::from(self.ageing.row));
        self.ageing.row = self.ageing.row.wrapping_add(1);
        self.ageing.growth_wait += 1;
        if self.ageing.growth_wait < GROWTH_EVERY {
            return;
        }
        self.ageing.growth_wait = 0;
        self.world.grow_row(u16::from(self.ageing.growth_row));
        self.ageing.growth_row = self.ageing.growth_row.wrapping_add(1);
        self.ageing.gate = self.ageing.gate.wrapping_add(1);
        let threshold = if self.ageing.gate & GATE_RAISED == 0 { 1 } else { 2 };
        if self.ageing.gate & GATE_COUNT < threshold {
            return;
        }
        self.ageing.gate = (self.ageing.gate & GATE_RAISED) ^ GATE_RAISED;
        self.world.age_marks_row(u16::from(self.ageing.marks_row));
        self.ageing.marks_row = self.ageing.marks_row.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests;
