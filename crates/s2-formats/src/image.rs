//! Palette-indexed images.

use crate::palette::Palette;

/// A rectangle of palette indices, stored row by row with no padding.
#[derive(Clone, PartialEq, Eq)]
pub struct IndexedImage {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

impl IndexedImage {
    /// Wraps `pixels`, which must hold exactly `width * height` indices.
    ///
    /// Returns `None` when the length does not match.
    #[must_use]
    pub fn new(width: u16, height: u16, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() == usize::from(width) * usize::from(height)).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// An image of the given size filled with `index`.
    #[must_use]
    pub fn filled(width: u16, height: u16, index: u8) -> Self {
        Self {
            width,
            height,
            pixels: vec![index; usize::from(width) * usize::from(height)],
        }
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// All indices, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Mutable access to the indices, row by row.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// The index at (`x`, `y`), or `None` outside the image.
    #[must_use]
    pub fn get(&self, x: u16, y: u16) -> Option<u8> {
        (x < self.width && y < self.height)
            .then(|| self.pixels[usize::from(y) * usize::from(self.width) + usize::from(x)])
    }

    /// Converts to 8-bit RGBA through `palette`; every pixel is opaque.
    #[must_use]
    pub fn to_rgba(&self, palette: &Palette) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&index| {
                let [r, g, b] = palette.colour(index);
                [r, g, b, u8::MAX]
            })
            .collect()
    }
}

impl std::fmt::Debug for IndexedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_length() {
        assert!(IndexedImage::new(2, 2, vec![0; 3]).is_none());
        let image = IndexedImage::new(2, 1, vec![4, 5]).unwrap();
        assert_eq!(image.get(1, 0), Some(5));
        assert_eq!(image.get(2, 0), None);
    }
}
