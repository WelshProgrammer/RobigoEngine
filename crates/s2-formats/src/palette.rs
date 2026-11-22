//! 256-colour palettes.

/// Number of entries in every Settlers II palette.
pub const PALETTE_SIZE: usize = 256;

/// A 256-entry palette of 8-bit RGB colours.
#[derive(Clone, PartialEq, Eq)]
pub struct Palette(pub [[u8; 3]; PALETTE_SIZE]);

impl Palette {
    /// Builds a palette from 768 bytes of 8-bit RGB triples.
    #[must_use]
    pub fn from_rgb8(bytes: &[u8; PALETTE_SIZE * 3]) -> Self {
        let mut colours = [[0; 3]; PALETTE_SIZE];
        for (colour, rgb) in colours.iter_mut().zip(bytes.as_chunks::<3>().0) {
            colour.copy_from_slice(rgb);
        }
        Self(colours)
    }

    /// Builds a palette from 768 bytes of 6-bit VGA DAC values, widened to 8 bits.
    ///
    /// The top two bits of each byte are ignored, as the DAC did.
    #[must_use]
    pub fn from_vga6(bytes: &[u8; PALETTE_SIZE * 3]) -> Self {
        let mut colours = [[0; 3]; PALETTE_SIZE];
        for (colour, rgb) in colours.iter_mut().zip(bytes.as_chunks::<3>().0) {
            for (channel, &value) in colour.iter_mut().zip(rgb) {
                let six = value % 64;
                // Repeats the top two bits at the bottom, so 63 widens to 255 and 0 stays 0.
                *channel = (six << 2) | (six >> 4);
            }
        }
        Self(colours)
    }

    /// The colour at `index`.
    #[must_use]
    pub fn colour(&self, index: u8) -> [u8; 3] {
        self.0[usize::from(index)]
    }

    /// The index of the colour nearest `colour`, by the squares of the three channels'
    /// differences added up. Of two as near the lower index wins.
    #[must_use]
    pub fn nearest(&self, colour: [u8; 3]) -> u8 {
        let apart = |other: &[u8; 3]| -> u32 {
            other
                .iter()
                .zip(colour)
                .map(|(&a, b)| u32::from(a.abs_diff(b)).pow(2))
                .sum()
        };
        (0..=u8::MAX)
            .zip(&self.0)
            .min_by_key(|(_, other)| apart(other))
            .map_or(0, |(index, _)| index)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self([[0; 3]; PALETTE_SIZE])
    }
}

impl std::fmt::Debug for Palette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Palette")
            .field("first", &self.0[0])
            .field("last", &self.0[PALETTE_SIZE - 1])
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vga6_widens_to_full_range() {
        let mut bytes = [0; PALETTE_SIZE * 3];
        bytes[0] = 63;
        bytes[1] = 32;
        let palette = Palette::from_vga6(&bytes);
        assert_eq!(palette.colour(0), [255, 130, 0]);
    }
}
