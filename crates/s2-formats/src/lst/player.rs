//! Type 4 bitmaps, whose player-colour runs are remapped to each player's shades when drawn.

use super::bitmap::{
    Budget, Header, Mask, Origin, RowReads, check_budget, nominal_input, rgba, row_quirk, row_table, rows,
};
use super::{Bitmap, Error, Quirk};
use crate::bytes::Reader;
use crate::{IndexedImage, Palette};

/// Shades in a player colour.
pub const PLAYER_SHADES: usize = 4;

/// A player-colour shade, 0 (brightest) to 3 (darkest).
///
/// Damaged or foreign data can store a larger shade byte. The decoder keeps it as found because
/// the engine draws it as the player's base index plus the byte. [`Shade::is_valid`] tells real
/// shades from these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shade(u8);

impl Shade {
    /// The shade numbered `value`, or `None` above 3.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 3 { Some(Self(value)) } else { None }
    }

    /// The shade byte as stored, whatever its value.
    #[must_use]
    pub const fn from_byte(value: u8) -> Self {
        Self(value)
    }

    /// The shade number: 0 to 3, or the stored byte for an out-of-range shade.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether the shade is one of the four a player colour has.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 <= 3
    }
}

/// The palette indices a player's four shades are drawn with.
///
/// Every palette shipped in the LST files holds a bright-to-dark blue ramp at indices 128 to
/// 131, which is [`PlayerColour::DEFAULT`]. Where the other players' ramps live and how the
/// engine swaps them in are not yet confirmed against S2.EXE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerColour(pub [u8; PLAYER_SHADES]);

impl PlayerColour {
    /// Indices 128 to 131.
    pub const DEFAULT: Self = Self([128, 129, 130, 131]);

    /// Four consecutive indices starting at `base`, or `None` if they would pass 255.
    #[must_use]
    pub const fn from_base(base: u8) -> Option<Self> {
        if base > u8::MAX - 3 {
            return None;
        }
        Some(Self([base, base + 1, base + 2, base + 3]))
    }

    /// The palette index for `shade`. An out-of-range shade `s` gives the first index plus `s`
    /// (wrapping past 255), matching the engine's base-plus-shade lookup.
    #[must_use]
    pub const fn index(self, shade: Shade) -> u8 {
        let [s0, s1, s2, s3] = self.0;
        match shade.0 {
            0 => s0,
            1 => s1,
            2 => s2,
            3 => s3,
            s => s0.wrapping_add(s),
        }
    }
}

impl Default for PlayerColour {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One pixel of a [`PlayerBitmap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Pixel {
    /// Not drawn.
    #[default]
    Transparent,
    /// A fixed palette index.
    Colour(u8),
    /// A shade of the owning player's colour.
    Player(Shade),
}

impl Pixel {
    /// The palette index this pixel draws with for `player`, or `None` when transparent.
    #[must_use]
    pub const fn resolve(self, player: PlayerColour) -> Option<u8> {
        match self {
            Self::Transparent => None,
            Self::Colour(index) => Some(index),
            Self::Player(shade) => Some(player.index(shade)),
        }
    }
}

/// A type 4 bitmap: fixed colours, player-colour shades and transparency.
#[derive(Clone, PartialEq, Eq)]
pub struct PlayerBitmap {
    origin: Origin,
    width: u16,
    height: u16,
    pixels: Vec<Pixel>,
    quirks: Vec<Quirk>,
}

impl PlayerBitmap {
    /// Decodes player-colour run-length data in the LST layout: a `u16` offset per row,
    /// relative to the start of `data`, followed by the rows.
    ///
    /// Each row is a sequence of commands until the width is filled. The top two bits of the
    /// command byte pick the kind and the low six bits give a count `n`:
    ///
    /// - `0b00`: `n` transparent pixels.
    /// - `0b01`: `n` palette indices follow.
    /// - `0b10`: one shade byte (0 to 3) follows, drawn `n` times in the player's colour.
    /// - `0b11`: one palette index follows, drawn `n` times.
    ///
    /// A command that runs past the width is clipped, and a shade above 3 is kept as found;
    /// both are recorded in [`PlayerBitmap::quirks`].
    ///
    /// # Errors
    ///
    /// [`Error::TooLarge`], [`Error::Truncated`], [`Error::RowStart`] or [`Error::RowReads`].
    pub fn decode(origin: Origin, width: u16, height: u16, data: &[u8]) -> Result<Self, Error> {
        let starts = row_table(data, height)?;
        let input = data.len();
        Self::decode_with(
            origin,
            width,
            height,
            data,
            starts,
            input,
            &mut RowReads::for_input(input),
        )
    }

    /// Decodes the row encoding of [`PlayerBitmap::decode`] for containers that store row
    /// offsets elsewhere. Row `y` starts at the `y`th offset given, measured into `data`.
    ///
    /// # Errors
    ///
    /// As [`PlayerBitmap::decode`], plus [`Error::MissingRow`] when `row_starts` runs out.
    pub fn decode_rows(
        origin: Origin,
        width: u16,
        height: u16,
        data: &[u8],
        row_starts: impl IntoIterator<Item = usize>,
    ) -> Result<Self, Error> {
        let reads = &mut RowReads::for_input(nominal_input(data, height));
        Self::decode_rows_within(origin, width, height, data, row_starts, reads)
    }

    pub(super) fn read(r: &mut Reader<'_>, budget: &mut Budget) -> Result<Self, Error> {
        let h = Header::read(r)?;
        if let Some(quirk) = h.no_data(budget)? {
            return Ok(Self {
                origin: h.origin,
                width: h.width,
                height: h.height,
                pixels: vec![Pixel::Transparent; super::bitmap::area(h.width, h.height)],
                quirks: vec![quirk],
            });
        }
        Self::decode(h.origin, h.width, h.height, h.data)
    }

    /// As [`PlayerBitmap::decode_rows`], charging row reads to `reads`, shared with the other
    /// bitmaps of a container whose rows index the same pixel data.
    pub(crate) fn decode_rows_within(
        origin: Origin,
        width: u16,
        height: u16,
        data: &[u8],
        row_starts: impl IntoIterator<Item = usize>,
        reads: &mut RowReads,
    ) -> Result<Self, Error> {
        let budget = nominal_input(data, height);
        Self::decode_with(origin, width, height, data, row_starts, budget, reads)
    }

    fn decode_with(
        origin: Origin,
        width: u16,
        height: u16,
        data: &[u8],
        starts: impl IntoIterator<Item = usize>,
        input: usize,
        reads: &mut RowReads,
    ) -> Result<Self, Error> {
        let mut pixels = vec![Pixel::Transparent; check_budget(width, height, input)?];
        let mut quirks = Vec::new();
        let w = usize::from(width);
        for entry in rows(height, starts) {
            let (row, start) = entry?;
            let at = usize::from(row) * w;
            let out = pixels.get_mut(at..at + w).unwrap_or_default();
            let decoded = reads.row(data, row, start, |r| decode_row(r, row, out, &mut quirks))?;
            quirks.extend(row_quirk(row, decoded, width, false));
        }
        Ok(Self {
            origin,
            width,
            height,
            pixels,
            quirks,
        })
    }

    /// The hotspot.
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.origin
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

    /// All pixels, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[Pixel] {
        &self.pixels
    }

    /// The pixel at (`x`, `y`), or `None` outside the bitmap.
    #[must_use]
    pub fn get(&self, x: u16, y: u16) -> Option<Pixel> {
        if x >= self.width {
            return None;
        }
        self.pixels
            .get(usize::from(y) * usize::from(self.width) + usize::from(x))
            .copied()
    }

    /// What was unusual about the encoding; empty for every shipped bitmap.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Resolves every pixel for `player`: palette indices (0 where transparent) and the mask
    /// of drawn pixels.
    #[must_use]
    pub fn to_indexed(&self, player: PlayerColour) -> (IndexedImage, Mask) {
        let mut image = IndexedImage::filled(self.width, self.height, 0);
        let mut mask = Mask::filled(self.width, self.height, false);
        let targets = image.pixels_mut().iter_mut().zip(mask.bits_mut());
        for (pixel, (index, drawn)) in self.pixels.iter().zip(targets) {
            if let Some(resolved) = pixel.resolve(player) {
                *index = resolved;
                *drawn = true;
            }
        }
        (image, mask)
    }

    /// Converts to 8-bit RGBA through `palette` with `player`'s shades, alpha 0 where
    /// transparent.
    #[must_use]
    pub fn to_rgba(&self, palette: &Palette, player: PlayerColour) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|p| rgba(palette, p.resolve(player)))
            .collect()
    }
}

impl std::fmt::Debug for PlayerBitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerBitmap")
            .field("origin", &self.origin)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl From<Bitmap> for PlayerBitmap {
    /// Converts a type 2 bitmap to fixed colours and transparency; fonts may store glyphs that
    /// way.
    fn from(bitmap: Bitmap) -> Self {
        let (origin, image, mask, quirks) = bitmap.into_parts();
        let pixels = image
            .pixels()
            .iter()
            .zip(mask.bits())
            .map(|(&index, &opaque)| {
                if opaque {
                    Pixel::Colour(index)
                } else {
                    Pixel::Transparent
                }
            })
            .collect();
        Self {
            origin,
            width: image.width(),
            height: image.height(),
            pixels,
            quirks,
        }
    }
}

/// Decodes one row, clipping commands at the width, and returns how many pixels they covered.
fn decode_row(
    r: &mut Reader<'_>,
    row: u16,
    out: &mut [Pixel],
    quirks: &mut Vec<Quirk>,
) -> Result<usize, Error> {
    let width = out.len();
    let mut x = 0;
    while x < width {
        let command = r.u8()?;
        // Top two bits pick the command, the low six give its count; see PlayerBitmap::decode.
        let count = usize::from(command % 64);
        let end = x + count;
        let span = out.get_mut(x..end.min(width)).unwrap_or_default();
        match command >> 6 {
            0 => {}
            1 => {
                let indices = r.bytes(count)?;
                for (pixel, &index) in span.iter_mut().zip(indices) {
                    *pixel = Pixel::Colour(index);
                }
            }
            2 => {
                let shade = Shade::from_byte(r.u8()?);
                if !shade.is_valid() {
                    quirks.push(Quirk::Shade {
                        row,
                        x: u16::try_from(x).unwrap_or(u16::MAX),
                        shade: shade.get(),
                    });
                }
                span.fill(Pixel::Player(shade));
            }
            _ => span.fill(Pixel::Colour(r.u8()?)),
        }
        x = end;
    }
    Ok(x)
}
