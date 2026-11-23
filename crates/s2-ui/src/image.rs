//! Pictures a window builds for one frame.
//!
//! An [`Op`](crate::Op) is `Copy`, so a picture a window works out itself goes into an arena on
//! the painter and the operation carries its index. The map screen is the first user: it colours
//! one pixel per node from the world and hands the result over whole.

/// Which picture a draw operation shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(pub u16);

/// One picture's pixels and shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image<'a> {
    /// The palette indices, row by row.
    pub pixels: &'a [u8],
    /// How many pixels a row holds.
    pub width: u32,
    /// How many rows there are.
    pub height: u32,
}

/// The pictures a frame's windows built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Images {
    pixels: Vec<u8>,
    spans: Vec<(usize, u32, u32)>,
}

impl Images {
    /// No pictures.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pixels: Vec::new(),
            spans: Vec::new(),
        }
    }

    /// Keeps `pixels` as a `width` by `height` picture and gives back the identifier that draws
    /// it. Past `u16::MAX` pictures the identifier names nothing. A short or long `pixels` is
    /// kept as it stands, and [`Images::get`] gives back only the rows that are whole.
    pub fn add(&mut self, pixels: &[u8], width: u32, height: u32) -> ImageId {
        let at = self.pixels.len();
        self.pixels.extend_from_slice(pixels);
        self.spans.push((at, width, height));
        ImageId(u16::try_from(self.spans.len() - 1).unwrap_or(u16::MAX))
    }

    /// Picture `image`, or nothing where there is no such picture.
    #[must_use]
    pub fn get(&self, image: ImageId) -> Option<Image<'_>> {
        let &(at, width, height) = self.spans.get(usize::from(image.0))?;
        let rows = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?;
        let pixels = self.pixels.get(at..at.checked_add(rows)?)?;
        Some(Image {
            pixels,
            width,
            height,
        })
    }

    /// How many pictures are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// Whether no picture is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Drops every picture, keeping the buffers.
    pub fn clear(&mut self) {
        self.pixels.clear();
        self.spans.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_comes_back_with_its_shape() {
        let mut images = Images::new();
        assert!(images.is_empty());
        let first = images.add(&[1, 2, 3, 4, 5, 6], 3, 2);
        let second = images.add(&[7, 8], 2, 1);
        assert_eq!(first, ImageId(0));
        assert_eq!(images.len(), 2);
        let picture = images.get(first).expect("the picture was kept");
        assert_eq!(picture.pixels, [1, 2, 3, 4, 5, 6]);
        assert_eq!((picture.width, picture.height), (3, 2));
        assert_eq!(images.get(second).map(|image| image.pixels), Some(&[7, 8][..]));
        assert_eq!(images.get(ImageId(9)), None);
    }

    #[test]
    fn a_picture_shorter_than_its_shape_gives_back_nothing() {
        let mut images = Images::new();
        let short = images.add(&[1, 2], 3, 2);
        assert_eq!(images.get(short), None);
        images.clear();
        assert!(images.is_empty());
        assert_eq!(images.get(short), None);
    }
}
