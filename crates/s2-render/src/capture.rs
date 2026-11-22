//! Screenshots of the original game, compared with Robigo's frames palette index by palette index.
//!
//! A raw `DOSBox` screenshot of the game keeps its palette indices, so the comparison needs no
//! palette and palette cycling cannot upset it. The game draws its interface over the edges of the
//! map view: the frame, the corner figures, the button bar and the mouse cursor. Robigo does not draw
//! those yet. [`interface_mask`] finds them as the pixels that hold the same index in captures of
//! different maps, and [`compare`] leaves them out.

/// The map view's distance from each edge of the screen, at every resolution the game offers.
pub const VIEW_MARGIN: u32 = 12;

/// How many animation counter values give different pictures: every object's phase is the
/// counter plus an offset, modulo 8.
pub const PHASES: u32 = 8;

/// A rectangle of pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    /// Left column.
    pub x: u32,
    /// Top row.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Why a screenshot could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// The PNG did not decode.
    #[error("not a readable PNG: {0}")]
    Png(#[from] png::DecodingError),
    /// The PNG holds colours, not palette indices; `DOSBox`'s raw screenshots hold indices.
    #[error("the PNG is {0:?} at {1} bits; a raw DOSBox screenshot is 8-bit indexed")]
    NotIndexed(png::ColorType, u8),
}

/// A screenshot of the original game: one palette index per pixel, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Capture {
    /// Wraps `pixels` as a `width` × `height` screen; `None` if the lengths disagree.
    #[must_use]
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() as u64 == u64::from(width) * u64::from(height)).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Reads an 8-bit indexed PNG, such as a raw `DOSBox` screenshot, keeping its indices.
    ///
    /// # Errors
    ///
    /// [`CaptureError`] if the bytes are not a PNG or not 8-bit indexed.
    pub fn from_png(bytes: &[u8]) -> Result<Self, CaptureError> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::IDENTITY);
        let mut reader = decoder.read_info()?;
        let (colour, depth) = reader.output_color_type();
        if colour != png::ColorType::Indexed || depth != png::BitDepth::Eight {
            return Err(CaptureError::NotIndexed(colour, depth as u8));
        }
        let mut pixels = vec![0; reader.output_buffer_size().unwrap_or_default()];
        let frame = reader.next_frame(&mut pixels)?;
        pixels.truncate(frame.buffer_size());
        Ok(Self {
            width: frame.width,
            height: frame.height,
            pixels,
        })
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

    /// Palette indices, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Where the map view sits on this screen; `None` if the screen is too small to hold one.
    #[must_use]
    pub fn view_rect(&self) -> Option<Rect> {
        let inner = |n: u32| n.checked_sub(2 * VIEW_MARGIN).filter(|&n| n > 0);
        Some(Rect {
            x: VIEW_MARGIN,
            y: VIEW_MARGIN,
            width: inner(self.width)?,
            height: inner(self.height)?,
        })
    }

    /// The map view's pixels, row by row; empty if the screen holds no view.
    #[must_use]
    pub fn view(&self) -> Vec<u8> {
        let Some(rect) = self.view_rect() else {
            return Vec::new();
        };
        let (stride, left, width) = (self.width as usize, rect.x as usize, rect.width as usize);
        (rect.y as usize..(rect.y + rect.height) as usize)
            .flat_map(|row| {
                let start = row * stride + left;
                self.pixels[start..start + width].iter().copied()
            })
            .collect()
    }
}

/// The pixels that hold the same index in every one of `views`: the game's interface, when the
/// views show different maps. `None` for fewer than two views or views of different lengths.
#[must_use]
pub fn interface_mask(views: &[&[u8]]) -> Option<Vec<bool>> {
    let (first, rest) = views.split_first()?;
    if rest.is_empty() || rest.iter().any(|v| v.len() != first.len()) {
        return None;
    }
    Some(
        (0..first.len())
            .map(|i| rest.iter().all(|v| v[i] == first[i]))
            .collect(),
    )
}

/// A group of differing pixels that touch, diagonals included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// The smallest rectangle holding the group.
    pub rect: Rect,
    /// How many pixels differ in it.
    pub pixels: usize,
}

/// How two pictures of one view differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// Pixels compared: all those not skipped.
    pub compared: usize,
    /// Compared pixels whose indices differ.
    pub differing: usize,
    /// The differing pixels in groups, largest first.
    pub regions: Vec<Region>,
}

/// Compares `ours` with `theirs`, both `width` pixels wide, leaving out the pixels `skip` marks.
/// `None` if the three lengths disagree or `width` does not divide them.
#[must_use]
pub fn compare(ours: &[u8], theirs: &[u8], width: u32, skip: &[bool]) -> Option<Comparison> {
    let width = width as usize;
    if ours.len() != theirs.len()
        || skip.len() != ours.len()
        || width == 0
        || !ours.len().is_multiple_of(width)
    {
        return None;
    }
    let differs: Vec<bool> = (0..ours.len())
        .map(|i| !skip[i] && ours[i] != theirs[i])
        .collect();
    Some(Comparison {
        compared: skip.iter().filter(|&&s| !s).count(),
        differing: differs.iter().filter(|&&d| d).count(),
        regions: regions(&differs, width),
    })
}

fn regions(marked: &[bool], width: usize) -> Vec<Region> {
    let height = marked.len() / width;
    let mut seen = vec![false; marked.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..marked.len() {
        if !marked[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
        let mut pixels = 0;
        while let Some(at) = stack.pop() {
            let (x, y) = (at % width, at / width);
            pixels += 1;
            (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let next = ny * width + nx;
                    if marked[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        let size = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        out.push(Region {
            rect: Rect {
                x: size(left),
                y: size(top),
                width: size(right - left + 1),
                height: size(bottom - top + 1),
            },
            pixels,
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.pixels));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_sits_inside_a_twelve_pixel_margin() {
        let pixels = (0..30 * 28_u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
        let capture = Capture::new(30, 28, pixels).unwrap();
        let rect = capture.view_rect().unwrap();
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (12, 12, 6, 4));
        let view = capture.view();
        assert_eq!(view.len(), 24);
        assert_eq!(view[0], capture.pixels()[12 * 30 + 12]);
        assert_eq!(view[23], capture.pixels()[15 * 30 + 17]);
        assert!(Capture::new(24, 40, vec![0; 960]).unwrap().view_rect().is_none());
        assert!(Capture::new(2, 2, vec![0; 3]).is_none());
    }

    fn png(colour: png::ColorType, width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(colour);
        encoder.set_depth(png::BitDepth::Eight);
        if colour == png::ColorType::Indexed {
            encoder.set_palette(vec![0; 3 * 256]);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    #[test]
    fn indexed_pngs_keep_their_indices() {
        let data: Vec<u8> = (0..12).map(|i| 250 - i).collect();
        let capture = Capture::from_png(&png(png::ColorType::Indexed, 4, 3, &data)).unwrap();
        assert_eq!((capture.width(), capture.height()), (4, 3));
        assert_eq!(capture.pixels(), data);
        let rgb = png(png::ColorType::Rgb, 2, 2, &[0; 12]);
        assert!(matches!(
            Capture::from_png(&rgb),
            Err(CaptureError::NotIndexed(png::ColorType::Rgb, 8))
        ));
        assert!(matches!(Capture::from_png(b"nope"), Err(CaptureError::Png(_))));
    }

    #[test]
    fn the_interface_is_what_every_capture_shares() {
        let a = [1, 2, 3, 4];
        let b = [1, 9, 3, 8];
        let c = [1, 2, 3, 7];
        assert_eq!(
            interface_mask(&[&a, &b, &c]),
            Some(vec![true, false, true, false])
        );
        assert_eq!(interface_mask(&[&a]), None);
        assert_eq!(interface_mask(&[&a, &[1, 2]]), None);
    }

    #[test]
    fn differences_are_grouped_and_skipped_pixels_ignored() {
        #[rustfmt::skip]
        let ours = [
            0, 0, 0, 0, 0,
            0, 1, 0, 0, 0,
            0, 0, 1, 0, 1,
            0, 0, 0, 0, 1,
        ];
        let theirs = [0; 20];
        let mut skip = [false; 20];
        skip[19] = true;
        let result = compare(&ours, &theirs, 5, &skip).unwrap();
        assert_eq!((result.compared, result.differing), (19, 3));
        assert_eq!(
            result.regions,
            [
                Region {
                    rect: Rect {
                        x: 1,
                        y: 1,
                        width: 2,
                        height: 2
                    },
                    pixels: 2
                },
                Region {
                    rect: Rect {
                        x: 4,
                        y: 2,
                        width: 1,
                        height: 1
                    },
                    pixels: 1
                },
            ]
        );
        assert!(compare(&ours, &theirs[..19], 5, &skip).is_none());
    }
}
