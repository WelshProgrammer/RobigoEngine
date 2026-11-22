//! Flags and the wares waiting at them.
//!
//! [`FlagSlots::ORIGINAL`] holds the slot offsets of `S2.EXE` at `0x1f7c5c` (x) and
//! `0x1f7c6c` (y); Gold has the same values at `0x1f7f34` and `0x1f7f44`.

use s2_sim::{FLAG_CAPACITY, FlagKind};

use super::{ObjectPass, WARE_ITEMS, darken};
use crate::Layers;
use crate::sprite::{Paint, colour_base};
use crate::terrain::{Point, Surface};

/// The slots a flag draws its waiting wares in.
const SLOTS: usize = FLAG_CAPACITY as usize;

/// Where each of a flag's eight slots draws its ware, as pixel offsets from the flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlagSlots {
    slots: [Point; SLOTS],
}

impl FlagSlots {
    /// The offsets of `S2.EXE`: pairs either side of the pole, rising and spreading out.
    pub const ORIGINAL: Self = Self::new([
        Point::new(2, 5),
        Point::new(-2, 5),
        Point::new(5, 4),
        Point::new(-5, 4),
        Point::new(8, 3),
        Point::new(-8, 3),
        Point::new(11, 0),
        Point::new(-11, 0),
    ]);

    /// A custom table: `slots[n]` is where slot `n` draws its ware.
    #[must_use]
    pub const fn new(slots: [Point; SLOTS]) -> Self {
        Self { slots }
    }

    /// The offset of slot `slot`, or none past the table.
    #[must_use]
    pub fn get(&self, slot: u8) -> Point {
        self.slots.get(usize::from(slot)).copied().unwrap_or_default()
    }
}

impl Default for FlagSlots {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl ObjectPass<'_> {
    /// Flag `record` (`0x1ad54`): its shadow, its bitmap in the owner's colours, then the wares
    /// waiting at it, newest first, each in its slot with no shadow.
    pub(super) fn flag(&self, surface: &mut Surface<'_>, top: i32, record: u16, at: Point) {
        let Some(flag) = self.world.flag(record) else {
            return;
        };
        let Some((_, sheet)) = self.graphics.nation(flag.owner) else {
            return;
        };
        let phase = (usize::from(record) + self.anim as usize) & 7;
        let (body, shadow) = match flag.kind {
            FlagKind::Normal => (100, 110),
            FlagKind::Donkey => (120, 130),
            FlagKind::Water => (140, 150),
        };
        let paint = darken(&self.graphics.shadow);
        self.sprite(
            surface,
            top,
            Some(sheet),
            shadow + phase,
            at,
            Layers::FLAGS,
            paint,
        );
        let paint = Paint {
            base: colour_base(flag.owner),
            shadow: None,
        };
        self.sprite(surface, top, Some(sheet), body + phase, at, Layers::FLAGS, paint);
        let map = Some(self.graphics.map.as_ref());
        for (_, ware) in self.world.wares_at(record) {
            let Some(bitmap) = ware.bitmap() else {
                continue;
            };
            let offset = self.graphics.tables.flag_slots.get(ware.slot);
            let item = WARE_ITEMS + usize::from(bitmap);
            self.sprite(
                surface,
                top,
                map,
                item,
                at.offset(offset.x, offset.y),
                Layers::FLAGS,
                Paint::PLAIN,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_pair_up_either_side_of_the_pole() {
        let slots = FlagSlots::ORIGINAL;
        for pair in 0..4 {
            let (right, left) = (slots.get(2 * pair), slots.get(2 * pair + 1));
            assert_eq!((right.x, right.y), (-left.x, left.y));
        }
        assert_eq!(slots.get(8), Point::default(), "past the table");
    }
}
