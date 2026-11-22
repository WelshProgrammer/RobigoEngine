//! Colour remap tables: shadows replace each pixel they cover through one of these.
//!
//! `S2.EXE` builds its tables when the palette loads (`0x880fe`, `0x879b5`): every palette
//! colour is transformed, then the whole palette is searched for the entry nearest to the
//! result (`0x87cee`). Nothing is blended when a shadow is drawn.

use s2_formats::Palette;

/// What a table does to a colour before looking up the nearest palette entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tint {
    /// Replace the channels by their mean, `(r + g + b) / 3`, first.
    pub grey: bool,
    /// Then scale red, green and blue by `k / 256`, clamped to 255.
    pub scale: [u16; 3],
}

impl Tint {
    /// The normal shadow: every channel at 75 % (192 / 256). Trees, stones, decorations, buildings,
    /// flags and figures use it (`0x200c56`).
    pub const SHADOW: Self = Self {
        grey: false,
        scale: [192; 3],
    };

    /// The grey shadow: the mean at 75 % (`0x200c5e`), used by fire, the map objects with slot 5
    /// from 220 to 223.
    pub const GREY_SHADOW: Self = Self {
        grey: true,
        scale: [192; 3],
    };

    /// The glow a working building's animation goes through: the mean at 200 % (`0x200c66`).
    pub const WORK: Self = Self {
        grey: true,
        scale: [512; 3],
    };

    /// The lightening a marked control's background goes through, every channel at 125 %. The
    /// original applies it four times over the background (`0x51898`).
    pub const HIGHLIGHT: Self = Self {
        grey: false,
        scale: [320; 3],
    };

    /// The greying a disabled control goes through last, the mean at 125 % (`[[0x200c42] +
    /// 0x14]`, `0x4d96a`).
    pub const DISABLED: Self = Self {
        grey: true,
        scale: [320; 3],
    };

    /// The greying a disabled control goes through on the screens before a game, where no
    /// landscape's tables exist yet: the mean at 87.5 %, built from the screen picture's palette
    /// (`[0x216389]`, `0x605af`) and applied twice (`0x4d96f`).
    pub const SCREEN_DISABLED: Self = Self {
        grey: true,
        scale: [224; 3],
    };

    fn apply(self, colour: [u8; 3]) -> [i32; 3] {
        let colour = if self.grey {
            let mean = colour.iter().map(|&c| u32::from(c)).sum::<u32>() / 3;
            [mean; 3]
        } else {
            colour.map(u32::from)
        };
        let scale = self.scale.map(u32::from);
        std::array::from_fn(|i| u8::try_from(colour[i] * scale[i] / 256).map_or(255, i32::from))
    }
}

/// A table replacing each palette index by another.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Remap([u8; 256]);

impl Remap {
    /// The table that leaves every index as it is.
    #[must_use]
    pub fn identity() -> Self {
        Self(std::array::from_fn(|i| u8::try_from(i).unwrap_or(u8::MAX)))
    }

    /// Each colour of `palette` (8 bits per channel, as in the file) transformed by `tint`, then
    /// matched to the nearest palette entry by the distance
    /// `(29·|Δr|)² + (65·|Δg|)² + (15·|Δb|)²`. On a tie the lowest index wins.
    #[must_use]
    pub fn nearest(palette: &Palette, tint: Tint) -> Self {
        Self(std::array::from_fn(|i| {
            let target = tint.apply(palette.0[i]);
            let distance = |colour: &[u8; 3]| {
                [29, 65, 15]
                    .iter()
                    .zip(colour)
                    .zip(target)
                    .map(|((&weight, &c), t)| (weight * (i32::from(c) - t)).pow(2))
                    .sum::<i32>()
            };
            (0..=u8::MAX)
                .min_by_key(|&j| (distance(&palette.0[usize::from(j)]), j))
                .unwrap_or_default()
        }))
    }

    /// The table that applies `self`, then `next`.
    #[must_use]
    pub fn then(&self, next: &Self) -> Self {
        Self(self.0.map(|index| next.get(index)))
    }

    /// Where index `index` goes.
    #[must_use]
    pub fn get(&self, index: u8) -> u8 {
        self.0[usize::from(index)]
    }

    /// The whole table.
    #[must_use]
    pub fn as_array(&self) -> &[u8; 256] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette(colours: &[[u8; 3]]) -> Palette {
        let mut palette = Palette::default();
        for (slot, colour) in palette.0.iter_mut().zip(colours) {
            *slot = *colour;
        }
        palette
    }

    #[test]
    fn shadows_pick_the_nearest_colour_at_three_quarters() {
        let p = palette(&[[0, 0, 0], [200, 200, 200], [150, 150, 150], [148, 148, 148]]);
        let table = Remap::nearest(&p, Tint::SHADOW);
        assert_eq!(table.get(1), 2, "200 at 75 % is 150");
        assert_eq!(table.get(0), 0);
        assert_eq!(table.get(3), 3, "111 is nearer 148 than black");
    }

    #[test]
    fn a_paused_view_darkens_twice_over() {
        let p = palette(&[[0, 0, 0], [240, 240, 240], [180, 180, 180], [135, 135, 135]]);
        let shadow = Remap::nearest(&p, Tint::SHADOW);
        let twice = shadow.then(&shadow);
        assert_eq!(shadow.get(1), 2, "240 at 75 % is 180");
        assert_eq!(twice.get(1), 3, "180 at 75 % is 135");
        assert_eq!(twice.get(0), 0, "black stays where it is");
    }

    #[test]
    fn ties_go_to_the_lowest_index() {
        let p = palette(&[[10, 10, 10], [40, 40, 40], [40, 40, 40]]);
        let table = Remap::nearest(
            &p,
            Tint {
                grey: false,
                scale: [256; 3],
            },
        );
        assert_eq!(table.get(2), 1);
        assert_eq!(table.get(200), 3, "black first appears at 3");
    }

    #[test]
    fn green_counts_most_in_the_distance() {
        let p = palette(&[[114, 100, 100], [100, 106, 100], [200, 200, 200]]);
        let half = Tint {
            grey: false,
            scale: [128; 3],
        };
        assert_eq!(
            Remap::nearest(&p, half).get(2),
            1,
            "6 off in green costs 390², 14 off in red 406²"
        );
    }

    #[test]
    fn the_grey_tint_averages_first() {
        let p = palette(&[[30, 60, 90], [45, 45, 45], [60, 60, 60]]);
        let table = Remap::nearest(&p, Tint::GREY_SHADOW);
        assert_eq!(table.get(0), 1, "mean 60 at 75 % is 45");
        assert_eq!(Remap::identity().get(77), 77);
    }
}
