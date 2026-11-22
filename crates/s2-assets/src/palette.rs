//! The frame palette as the VGA shows it, and the water and lava colour cycles.

use std::time::Duration;

use s2_formats::Palette;

/// A palette range the game rotates: `count` entries from `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CycleRange {
    /// First index.
    pub first: u8,
    /// Entries in the range.
    pub count: u8,
}

/// The ranges `S2.EXE` rotates, hard-coded in its palette object (`0x12f12`): water at
/// 240–247 and lava at 248–251. The files' own `CRNG` chunks are ignored.
pub const CYCLES: [CycleRange; 2] = [
    CycleRange { first: 240, count: 8 },
    CycleRange { first: 248, count: 4 },
];

/// Timer ticks between cycle steps; the timer runs at [`TICK_RATE`].
pub const CYCLE_TICKS: u32 = 9;

/// The game timer's rate in hertz.
pub const TICK_RATE: u32 = 60;

/// Time between cycle steps: [`CYCLE_TICKS`] ticks of [`TICK_RATE`], 150 ms.
pub const CYCLE_PERIOD: Duration = Duration::from_millis(1000 * CYCLE_TICKS as u64 / TICK_RATE as u64);

/// The palette the DAC shows for a file's 8-bit `CMAP`. The game drops the low two bits of each
/// channel when uploading. The six bits left widen back to eight by repeating their top two bits,
/// as `DOSBox` does.
#[must_use]
pub fn dac_palette(cmap: &Palette) -> Palette {
    let mut out = cmap.clone();
    for colour in &mut out.0 {
        for channel in colour {
            let six = *channel >> 2;
            *channel = (six << 2) | (six >> 4);
        }
    }
    out
}

/// A base palette and the water and lava cycles applied to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteCycle {
    base: Palette,
}

impl PaletteCycle {
    /// Cycles of `base`, the palette as loaded.
    #[must_use]
    pub fn new(base: Palette) -> Self {
        Self { base }
    }

    /// The palette as loaded.
    #[must_use]
    pub fn base(&self) -> &Palette {
        &self.base
    }

    /// The palette after `steps` steps: in each range, entry `first + i` shows the base colour
    /// at `first + (steps + i) mod count`, so colours move one index down per step.
    #[must_use]
    pub fn at(&self, steps: u64) -> Palette {
        let mut out = self.base.clone();
        for range in CYCLES {
            let first = usize::from(range.first);
            let count = usize::from(range.count);
            let offset = usize::try_from(steps % u64::from(range.count)).unwrap_or_default();
            for i in 0..count {
                out.0[first + i] = self.base.0[first + (offset + i) % count];
            }
        }
        out
    }

    /// Steps taken in the `elapsed` time since the cycles started.
    #[must_use]
    pub fn steps_after(elapsed: Duration) -> u64 {
        u64::try_from(elapsed.as_millis() / CYCLE_PERIOD.as_millis()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered() -> Palette {
        let mut palette = Palette::default();
        for (i, colour) in palette.0.iter_mut().enumerate() {
            *colour = [u8::try_from(i).unwrap(); 3];
        }
        palette
    }

    #[test]
    fn the_dac_drops_two_bits_and_dosbox_widens_six() {
        let mut cmap = Palette::default();
        cmap.0[1] = [255, 3, 128];
        cmap.0[2] = [254, 1, 131];
        let dac = dac_palette(&cmap);
        assert_eq!(dac.0[1], [255, 0, 130]);
        assert_eq!(dac.0[2], dac.0[1]);
    }

    #[test]
    fn colours_move_one_index_down_per_step() {
        let cycle = PaletteCycle::new(numbered());
        assert_eq!(cycle.at(0), numbered());
        let one = cycle.at(1);
        assert_eq!(one.colour(240), [241; 3]);
        assert_eq!(one.colour(247), [240; 3]);
        assert_eq!(one.colour(248), [249; 3]);
        assert_eq!(one.colour(251), [248; 3]);
        assert_eq!(one.colour(239), [239; 3]);
        assert_eq!(one.colour(252), [252; 3]);
        assert_eq!(cycle.at(8).colour(240), [240; 3]);
        assert_eq!(cycle.at(8).colour(248), [248; 3]);
        assert_eq!(cycle.at(6).colour(248), [250; 3]);
    }

    #[test]
    fn steps_every_150_ms() {
        assert_eq!(CYCLE_PERIOD, Duration::from_millis(150));
        assert_eq!(PaletteCycle::steps_after(Duration::from_millis(149)), 0);
        assert_eq!(PaletteCycle::steps_after(Duration::from_millis(300)), 2);
    }
}
