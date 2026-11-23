//! Frames of palette indices and the RGBA images they resolve to.

use s2_assets::{Palette, Remap};

use crate::Provenance;
use crate::terrain::Surface;

/// A frame of palette indices, row by row: what the CPU draws and the GPU resolves.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Frame {
    /// A `width` × `height` frame of index `fill`.
    #[must_use]
    pub fn new(width: u32, height: u32, fill: u8) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; width as usize * height as usize],
        }
    }

    /// Copies `pixels`, row after row, over the frame. Gives false, leaving the frame as it was,
    /// when they are not one index a pixel.
    pub fn fill_from(&mut self, pixels: &[u8]) -> bool {
        if pixels.len() != self.pixels.len() {
            return false;
        }
        self.pixels.copy_from_slice(pixels);
        true
    }

    /// Puts every pixel back to index `fill`.
    pub fn clear(&mut self, fill: u8) {
        self.pixels.fill(fill);
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The indices, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Sets the pixel at (`x`, `y`) to `index`; nothing outside the frame.
    pub fn set(&mut self, x: i32, y: i32, index: u8) {
        if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y))
            && x < self.width
            && y < self.height
        {
            self.pixels[y as usize * self.width as usize + x as usize] = index;
        }
    }

    pub(crate) fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// The whole frame as a surface to draw into.
    pub fn surface(&mut self) -> Surface<'_> {
        Surface::of_parts(&mut self.pixels, None, self.width, self.height)
    }

    /// The whole frame as a surface that records which layers write each pixel into
    /// `provenance`; `None` unless `provenance` is the frame's size.
    pub fn traced_surface<'a>(&'a mut self, provenance: &'a mut Provenance) -> Option<Surface<'a>> {
        ((provenance.width(), provenance.height()) == (self.width, self.height)).then(|| {
            Surface::of_parts(
                &mut self.pixels,
                Some(provenance.marks_mut()),
                self.width,
                self.height,
            )
        })
    }

    /// Copies `other` in with its top-left corner at (`x`, `y`), cutting what falls outside.
    pub fn paste(&mut self, other: &Self, x: u32, y: u32) {
        let width = other.width.min(self.width.saturating_sub(x)) as usize;
        for row in 0..other.height.min(self.height.saturating_sub(y)) {
            let from = row as usize * other.width as usize;
            let to = (y + row) as usize * self.width as usize + x as usize;
            self.pixels[to..to + width].copy_from_slice(&other.pixels[from..from + width]);
        }
    }

    /// Sends every pixel through `table`, which a paused view does with
    /// [`s2_assets::LandscapeAssets::paused`].
    pub fn remap(&mut self, table: &Remap) {
        for pixel in &mut self.pixels {
            *pixel = table.get(*pixel);
        }
    }

    /// The frame resolved through `palette` on the CPU, one image pixel per index.
    #[must_use]
    pub fn to_image(&self, palette: &Palette) -> Image {
        let rgba = self
            .pixels
            .iter()
            .flat_map(|&i| {
                let [r, g, b] = palette.colour(i);
                [r, g, b, u8::MAX]
            })
            .collect();
        Image::new(self.width, self.height, rgba)
    }
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// An 8-bit RGBA image, row by row.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Image {
    pub(crate) fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self { width, height, rgba }
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Four bytes per pixel, row by row; alpha is always 255.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The pixel at (`x`, `y`), or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba.get(at..at + 4)?.try_into().ok()
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_cuts_at_the_edges() {
        let mut screen = Frame::new(4, 3, 0);
        let view = Frame::new(3, 2, 9);
        screen.paste(&view, 2, 2);
        assert_eq!(screen.pixels(), [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 9]);
    }

    #[test]
    fn resolves_through_the_palette() {
        let mut palette = Palette::default();
        palette.0[5] = [1, 2, 3];
        let image = Frame::new(2, 1, 5).to_image(&palette);
        assert_eq!(image.pixel(1, 0), Some([1, 2, 3, 255]));
        assert_eq!(image.pixel(2, 0), None);
    }
}
