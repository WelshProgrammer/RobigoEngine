//! Bitmaps with a transparency mask (type 2), shadows (type 7) and raw bitmaps (type 14), plus
//! the header and row plumbing the player-colour bitmaps share.

use super::{Error, Quirk};
use crate::bytes::{Reader, to_usize};
use crate::{IndexedImage, Palette};

/// The most output pixels a decoder accepts per input byte before rejecting a bitmap as
/// [`Error::TooLarge`]. The densest legal encoding, a fully transparent row, gives about 127
/// pixels per byte, half this limit. The same factor caps the bytes a bitmap's rows may read in
/// total per input byte ([`Error::RowReads`]); shipped rows never share data and read each byte
/// once.
pub const MAX_EXPANSION: usize = 256;

/// The value that ends a row in the draw/skip encoding.
pub(super) const END_OF_ROW: u8 = u8::MAX;

/// A bitmap's hotspot: the pixel, measured from its top-left corner, that sits on the point
/// it is drawn at. Stored in the file as `nx`, `ny`; either may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Origin {
    /// Hotspot column.
    pub x: i16,
    /// Hotspot row.
    pub y: i16,
}

/// One flag per pixel, row by row: `true` where the bitmap draws (opaque or shadowed).
#[derive(Clone, PartialEq, Eq)]
pub struct Mask {
    width: u16,
    height: u16,
    bits: Vec<bool>,
}

impl Mask {
    /// Wraps `bits`, which must hold exactly `width * height` flags; `None` otherwise.
    #[must_use]
    pub fn new(width: u16, height: u16, bits: Vec<bool>) -> Option<Self> {
        (bits.len() == area(width, height)).then_some(Self { width, height, bits })
    }

    /// A mask of the given size with every flag set to `value`.
    #[must_use]
    pub fn filled(width: u16, height: u16, value: bool) -> Self {
        Self {
            width,
            height,
            bits: vec![value; area(width, height)],
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

    /// All flags, row by row.
    #[must_use]
    pub fn bits(&self) -> &[bool] {
        &self.bits
    }

    /// The flag at (`x`, `y`), or `None` outside the mask.
    #[must_use]
    pub fn get(&self, x: u16, y: u16) -> Option<bool> {
        if x >= self.width {
            return None;
        }
        self.bits
            .get(usize::from(y) * usize::from(self.width) + usize::from(x))
            .copied()
    }

    /// How many flags are set.
    #[must_use]
    pub fn count(&self) -> usize {
        self.bits.iter().filter(|&&set| set).count()
    }

    pub(super) fn bits_mut(&mut self) -> &mut [bool] {
        &mut self.bits
    }
}

impl std::fmt::Debug for Mask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mask")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// A type 2 bitmap: palette indices with a transparency mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    origin: Origin,
    image: IndexedImage,
    mask: Mask,
    quirks: Vec<Quirk>,
}

impl Bitmap {
    /// Decodes draw/skip run-length data in the LST layout: a `u16` offset per row, relative to
    /// the start of `data`, followed by the rows.
    ///
    /// Each row is a sequence of `n` (pixels to draw), `n` palette indices, `m` (pixels to
    /// skip), ending with 255 where the next `n` would be. A row that covers more than the
    /// width is clipped and one that covers less is transparent to the end; both are recorded
    /// in [`Bitmap::quirks`].
    ///
    /// # Errors
    ///
    /// [`Error::TooLarge`], [`Error::Truncated`] when the offset table or a row runs off the
    /// end, [`Error::RowStart`], or [`Error::RowReads`] when rows re-read the data too often.
    pub fn decode(origin: Origin, width: u16, height: u16, data: &[u8]) -> Result<Self, Error> {
        let starts = row_table(data, height)?;
        decode_draw_skip(width, height, data, starts, data.len(), true).map(|(image, mask, quirks)| Self {
            origin,
            image,
            mask,
            quirks,
        })
    }

    /// Decodes the row encoding of [`Bitmap::decode`] for containers that store row offsets
    /// elsewhere. Row `y` starts at the `y`th offset given, measured into `data`.
    ///
    /// # Errors
    ///
    /// As [`Bitmap::decode`], plus [`Error::MissingRow`] when `row_starts` runs out early.
    pub fn decode_rows(
        origin: Origin,
        width: u16,
        height: u16,
        data: &[u8],
        row_starts: impl IntoIterator<Item = usize>,
    ) -> Result<Self, Error> {
        let budget = nominal_input(data, height);
        decode_draw_skip(width, height, data, row_starts, budget, true).map(|(image, mask, quirks)| Self {
            origin,
            image,
            mask,
            quirks,
        })
    }

    pub(super) fn read(r: &mut Reader<'_>, budget: &mut Budget) -> Result<Self, Error> {
        let h = Header::read(r)?;
        if let Some(quirk) = h.no_data(budget)? {
            return Ok(Self {
                origin: h.origin,
                image: IndexedImage::filled(h.width, h.height, 0),
                mask: Mask::filled(h.width, h.height, false),
                quirks: vec![quirk],
            });
        }
        Self::decode(h.origin, h.width, h.height, h.data)
    }

    /// The hotspot.
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.image.width()
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.image.height()
    }

    /// Palette indices; transparent pixels hold 0.
    #[must_use]
    pub fn image(&self) -> &IndexedImage {
        &self.image
    }

    /// Which pixels are opaque.
    #[must_use]
    pub fn mask(&self) -> &Mask {
        &self.mask
    }

    /// What was unusual about the encoding; empty for every shipped bitmap.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Converts to 8-bit RGBA through `palette`, with alpha 0 where the mask is clear.
    #[must_use]
    pub fn to_rgba(&self, palette: &Palette) -> Vec<u8> {
        self.image
            .pixels()
            .iter()
            .zip(&self.mask.bits)
            .flat_map(|(&index, &opaque)| rgba(palette, opaque.then_some(index)))
            .collect()
    }

    pub(super) fn into_parts(self) -> (Origin, IndexedImage, Mask, Vec<Quirk>) {
        (self.origin, self.image, self.mask, self.quirks)
    }
}

/// A type 7 shadow: a mask of the pixels that darken what lies beneath, with no colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadow {
    origin: Origin,
    mask: Mask,
    quirks: Vec<Quirk>,
}

impl Shadow {
    /// Decodes shadow data in the LST layout: the row offset table and draw/skip rows of
    /// [`Bitmap::decode`], with no index bytes after each draw count.
    ///
    /// # Errors
    ///
    /// As [`Bitmap::decode`].
    pub fn decode(origin: Origin, width: u16, height: u16, data: &[u8]) -> Result<Self, Error> {
        let starts = row_table(data, height)?;
        decode_draw_skip(width, height, data, starts, data.len(), false).map(|(_, mask, quirks)| Self {
            origin,
            mask,
            quirks,
        })
    }

    /// Decodes shadow rows that start at the given offsets into `data`.
    ///
    /// # Errors
    ///
    /// As [`Bitmap::decode_rows`].
    pub fn decode_rows(
        origin: Origin,
        width: u16,
        height: u16,
        data: &[u8],
        row_starts: impl IntoIterator<Item = usize>,
    ) -> Result<Self, Error> {
        let budget = nominal_input(data, height);
        decode_draw_skip(width, height, data, row_starts, budget, false).map(|(_, mask, quirks)| Self {
            origin,
            mask,
            quirks,
        })
    }

    pub(super) fn read(r: &mut Reader<'_>, budget: &mut Budget) -> Result<Self, Error> {
        let h = Header::read(r)?;
        if let Some(quirk) = h.no_data(budget)? {
            return Ok(Self {
                origin: h.origin,
                mask: Mask::filled(h.width, h.height, false),
                quirks: vec![quirk],
            });
        }
        Self::decode(h.origin, h.width, h.height, h.data)
    }

    /// The hotspot.
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.mask.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.mask.height
    }

    /// Which pixels are in shadow.
    #[must_use]
    pub fn mask(&self) -> &Mask {
        &self.mask
    }

    /// What was unusual about the encoding; empty for every shipped shadow.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }
}

/// A type 14 bitmap: uncompressed palette indices, every pixel opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBitmap {
    origin: Origin,
    image: IndexedImage,
}

impl RawBitmap {
    /// Reads a type 14 body: `u16` (always 1), `u32` length, the pixels, `i16` nx and ny,
    /// `u16` width and height, then 8 bytes that are identical in every shipped item.
    pub(super) fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.u16()?;
        let len = to_usize(r.u32()?);
        let pixels = r.bytes(len)?;
        let origin = Origin {
            x: r.i16()?,
            y: r.i16()?,
        };
        let (width, height) = (r.u16()?, r.u16()?);
        r.skip(8)?;
        let image =
            IndexedImage::new(width, height, pixels.to_vec()).ok_or(Error::RawSize { width, height, len })?;
        Ok(Self { origin, image })
    }

    /// The hotspot.
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.image.width()
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.image.height()
    }

    /// The pixels.
    #[must_use]
    pub fn image(&self) -> &IndexedImage {
        &self.image
    }

    /// Converts to opaque 8-bit RGBA through `palette`.
    #[must_use]
    pub fn to_rgba(&self, palette: &Palette) -> Vec<u8> {
        self.image.to_rgba(palette)
    }
}

/// The header shared by types 2, 4 and 7.
pub(super) struct Header<'a> {
    pub origin: Origin,
    pub width: u16,
    pub height: u16,
    pub data: &'a [u8],
}

impl<'a> Header<'a> {
    /// Reads `i16` nx, `i16` ny, a `u32` that is always 0, `u16` width, `u16` height, a `u16`
    /// that is always 1, a `u32` data length and that many bytes of data.
    pub fn read(r: &mut Reader<'a>) -> Result<Self, Error> {
        let origin = Origin {
            x: r.i16()?,
            y: r.i16()?,
        };
        r.u32()?;
        let (width, height) = (r.u16()?, r.u16()?);
        r.u16()?;
        let len = to_usize(r.u32()?);
        Ok(Self {
            origin,
            width,
            height,
            data: r.bytes(len)?,
        })
    }

    /// A [`Quirk::NoData`] when the bitmap has rows but a data length of 0, after charging its
    /// pixels to `budget`.
    pub fn no_data(&self, budget: &mut Budget) -> Result<Option<Quirk>, Error> {
        if !self.data.is_empty() || self.height == 0 {
            return Ok(None);
        }
        budget.take(self.width, self.height)?;
        Ok(Some(Quirk::NoData {
            width: self.width,
            height: self.height,
        }))
    }
}

/// Pixels a container may still spend on bitmaps that carry no data of their own, so that a
/// small input cannot claim many large transparent bitmaps.
#[derive(Debug)]
pub(crate) struct Budget(usize);

impl Budget {
    /// [`MAX_EXPANSION`] pixels per byte of the container.
    pub(crate) fn for_input(len: usize) -> Self {
        Self(len.saturating_mul(MAX_EXPANSION))
    }

    fn take(&mut self, width: u16, height: u16) -> Result<(), Error> {
        let pixels = area(width, height);
        self.0 = self.0.checked_sub(pixels).ok_or(Error::TooLarge {
            width,
            height,
            input: self.0 / MAX_EXPANSION,
        })?;
        Ok(())
    }
}

/// The row starts from an LST offset table at the front of `data`.
pub(super) fn row_table(data: &[u8], height: u16) -> Result<impl Iterator<Item = usize>, Error> {
    let table = crate::bytes::slice(data, 0, usize::from(height) * 2)?;
    Ok(table
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| usize::from(u16::from_le_bytes(pair))))
}

/// The input size a `decode_rows` call is judged against: the data plus the offset table the
/// caller must have read from somewhere.
pub(super) fn nominal_input(data: &[u8], height: u16) -> usize {
    data.len().saturating_add(usize::from(height) * 2)
}

/// Rejects sizes that `input` bytes could not describe, before anything is allocated.
pub(super) fn check_budget(width: u16, height: u16, input: usize) -> Result<usize, Error> {
    let pixels = area(width, height);
    if pixels > input.saturating_mul(MAX_EXPANSION) {
        return Err(Error::TooLarge { width, height, input });
    }
    Ok(pixels)
}

/// Pairs each row index with its start, failing when the iterator runs short.
pub(super) fn rows(
    height: u16,
    starts: impl IntoIterator<Item = usize>,
) -> impl Iterator<Item = Result<(u16, usize), Error>> {
    let mut starts = starts.into_iter();
    (0..height).map(move |row| {
        starts
            .next()
            .map(|start| (row, start))
            .ok_or(Error::MissingRow { row })
    })
}

/// Row bytes a bitmap, or every bitmap of a container that shares pixel data, may still read.
/// Rows may start anywhere in the data, and without this limit each row could read all of it.
pub(crate) struct RowReads {
    left: usize,
    input: usize,
}

impl RowReads {
    /// [`MAX_EXPANSION`] bytes per byte of `input`.
    pub(crate) fn for_input(input: usize) -> Self {
        Self {
            left: input.saturating_mul(MAX_EXPANSION),
            input,
        }
    }

    /// Runs `decode` on a reader placed at the row's start, then charges the bytes it read.
    pub(super) fn row<T>(
        &mut self,
        data: &[u8],
        row: u16,
        start: usize,
        decode: impl FnOnce(&mut Reader<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut r = Reader::new(data);
        r.seek(start)
            .map_err(|_| Error::RowStart { row, offset: start })?;
        let decoded = decode(&mut r)?;
        let read = r.position() - start;
        self.left = self.left.checked_sub(read).ok_or(Error::RowReads {
            row,
            input: self.input,
        })?;
        Ok(decoded)
    }
}

fn decode_draw_skip(
    width: u16,
    height: u16,
    data: &[u8],
    starts: impl IntoIterator<Item = usize>,
    input: usize,
    with_indices: bool,
) -> Result<(IndexedImage, Mask, Vec<Quirk>), Error> {
    check_budget(width, height, input)?;
    let mut reads = RowReads::for_input(input);
    let mut image = IndexedImage::filled(if with_indices { width } else { 0 }, height, 0);
    let mut mask = Mask::filled(width, height, false);
    let mut quirks = Vec::new();
    let w = usize::from(width);
    for entry in rows(height, starts) {
        let (row, start) = entry?;
        let at = usize::from(row) * w;
        let mask_row = mask.bits.get_mut(at..at + w).unwrap_or_default();
        let index_row = if with_indices {
            Some(image.pixels_mut().get_mut(at..at + w).unwrap_or_default())
        } else {
            None
        };
        let decoded = reads.row(data, row, start, |r| draw_skip_row(r, mask_row, index_row))?;
        quirks.extend(row_quirk(row, decoded, width, true));
    }
    Ok((image, mask, quirks))
}

/// Decodes one draw/skip row, clipping runs at the width, and returns how many pixels its runs
/// covered.
fn draw_skip_row(
    r: &mut Reader<'_>,
    mask: &mut [bool],
    mut indices: Option<&mut [u8]>,
) -> Result<usize, Error> {
    let width = mask.len();
    let mut x = 0usize;
    loop {
        let draw = r.u8()?;
        if draw == END_OF_ROW {
            return Ok(x);
        }
        let end = x + usize::from(draw);
        let kept = x.min(width)..end.min(width);
        mask.get_mut(kept.clone()).unwrap_or_default().fill(true);
        if let Some(indices) = indices.as_deref_mut() {
            let src = r.bytes(usize::from(draw))?;
            let dst = indices.get_mut(kept).unwrap_or_default();
            dst.iter_mut().zip(src).for_each(|(d, &s)| *d = s);
        }
        x = end + usize::from(r.u8()?);
    }
}

/// The quirk for a row that covered `decoded` pixels of a `width`-pixel bitmap, if any. Short
/// rows only count where the encoding has an end-of-row marker.
pub(super) fn row_quirk(row: u16, decoded: usize, width: u16, terminated: bool) -> Option<Quirk> {
    let w = usize::from(width);
    if decoded > w {
        Some(Quirk::RowOverrun { row, decoded, width })
    } else if decoded < w && terminated {
        Some(Quirk::RowShort { row, decoded, width })
    } else {
        None
    }
}

pub(super) fn rgba(palette: &Palette, index: Option<u8>) -> [u8; 4] {
    match index {
        Some(index) => {
            let [r, g, b] = palette.colour(index);
            [r, g, b, u8::MAX]
        }
        None => [0; 4],
    }
}

pub(super) fn area(width: u16, height: u16) -> usize {
    usize::from(width) * usize::from(height)
}
