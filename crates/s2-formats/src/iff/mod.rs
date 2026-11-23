//! IFF ILBM/PBM pictures and palettes (`.LBM`, `.BBM`).
//!
//! Every picture, texture sheet and palette the game ships is a Deluxe Paint `FORM PBM ` file:
//! 8-bit chunky pixels, rows padded to an even length, uncompressed or `ByteRun1`. Planar `ILBM`
//! is decoded too, though no shipped file uses it. `docs/formats/iff.md` lists the quirks found
//! in the data.

mod byterun1;
#[cfg(test)]
mod tests;

use std::fmt;

use crate::bytes::{Reader, Truncated, to_usize};
use crate::image::IndexedImage;
use crate::palette::{PALETTE_SIZE, Palette};
use byterun1::{ByteRun1, MAX_EXPANSION};

/// A four-character IFF chunk or form identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId(pub [u8; 4]);

impl ChunkId {
    /// The outer container.
    pub const FORM: Self = Self(*b"FORM");
    /// Chunky 8-bit form written by Deluxe Paint for PC.
    pub const PBM: Self = Self(*b"PBM ");
    /// Interleaved bitplane form.
    pub const ILBM: Self = Self(*b"ILBM");
    /// Bitmap header.
    pub const BMHD: Self = Self(*b"BMHD");
    /// Colour map.
    pub const CMAP: Self = Self(*b"CMAP");
    /// Pixel data.
    pub const BODY: Self = Self(*b"BODY");
    /// Colour cycling range.
    pub const CRNG: Self = Self(*b"CRNG");
    /// Brush hotspot.
    pub const GRAB: Self = Self(*b"GRAB");
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|&b| {
            let c = if b.is_ascii_graphic() || b == b' ' {
                char::from(b)
            } else {
                '?'
            };
            fmt::Write::write_char(f, c)
        })
    }
}

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkId(\"{self}\")")
    }
}

/// Why an IFF file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A chunk or the form ran past the end of the input.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The input does not start with `FORM`.
    #[error("not an IFF FORM file")]
    NotForm,
    /// The form is neither `PBM ` nor `ILBM`.
    #[error("unsupported FORM type {0}")]
    UnsupportedForm(ChunkId),
    /// A required chunk is absent.
    #[error("no {0} chunk")]
    MissingChunk(ChunkId),
    /// The bit depth is not 8 for `PBM `, or not 1 to 8 for `ILBM`.
    #[error("unsupported bit depth: {0} planes")]
    UnsupportedPlanes(u8),
    /// The `BODY` compression is neither none nor `ByteRun1`.
    #[error("unsupported compression method {0}")]
    UnsupportedCompression(u8),
    /// A `PBM ` header asks for a separate mask plane, which chunky pixels cannot carry.
    #[error("PBM picture with a mask plane")]
    UnsupportedMaskPlane,
    /// The `BODY` holds fewer row bytes than the header needs.
    #[error("BODY of {body} bytes cannot supply {expected} bytes of rows")]
    BodyTooShort {
        /// Row bytes the header calls for, padding included.
        expected: usize,
        /// Length of the `BODY` chunk.
        body: usize,
    },
}

/// Something unusual that decoding accepted. The shipped files produce none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// The `FORM` length runs past the end of the file; the form is clamped to the file.
    FormLength {
        /// Length the header declares.
        declared: usize,
        /// Bytes the file holds after the header.
        available: usize,
    },
}

/// The two picture forms this module decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Form {
    /// `PBM `: one byte per pixel, rows padded to an even length.
    Pbm,
    /// `ILBM`: one bitplane after another for each row, each padded to 16 bits.
    Ilbm,
}

impl Form {
    fn from_id(id: ChunkId) -> Result<Self, Error> {
        match id {
            ChunkId::PBM => Ok(Self::Pbm),
            ChunkId::ILBM => Ok(Self::Ilbm),
            other => Err(Error::UnsupportedForm(other)),
        }
    }
}

/// The `BMHD` masking field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Masking {
    /// No transparency.
    None,
    /// `ILBM` only: each row carries an extra mask plane after its colour planes.
    HasMask,
    /// [`BitmapHeader::transparent_colour`] marks transparent pixels.
    TransparentColour,
    /// Transparency by lasso; decoded as opaque.
    Lasso,
    /// A value the IFF specification does not define.
    Other(u8),
}

impl From<u8> for Masking {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::HasMask,
            2 => Self::TransparentColour,
            3 => Self::Lasso,
            other => Self::Other(other),
        }
    }
}

/// How the `BODY` rows are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compression {
    /// Rows stored as they are.
    None,
    /// PackBits-style run-length encoding.
    ByteRun1,
}

/// The `BMHD` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BitmapHeader {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Horizontal position the image was saved from.
    pub x: i16,
    /// Vertical position the image was saved from.
    pub y: i16,
    /// Bits per pixel.
    pub planes: u8,
    /// Transparency mode.
    pub masking: Masking,
    /// How `BODY` is stored.
    pub compression: Compression,
    /// The transparent palette index when [`masking`](Self::masking) says so; editors fill it
    /// in regardless.
    pub transparent_colour: u16,
    /// Pixel aspect ratio numerator.
    pub x_aspect: u8,
    /// Pixel aspect ratio denominator.
    pub y_aspect: u8,
    /// Width of the editor page the image came from.
    pub page_width: i16,
    /// Height of the editor page the image came from.
    pub page_height: i16,
}

impl BitmapHeader {
    fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        let width = r.u16_be()?;
        let height = r.u16_be()?;
        let x = i16::from_be_bytes(r.array()?);
        let y = i16::from_be_bytes(r.array()?);
        let planes = r.u8()?;
        let masking = Masking::from(r.u8()?);
        let compression = match r.u8()? {
            0 => Compression::None,
            1 => Compression::ByteRun1,
            other => return Err(Error::UnsupportedCompression(other)),
        };
        r.skip(1)?;
        Ok(Self {
            width,
            height,
            x,
            y,
            planes,
            masking,
            compression,
            transparent_colour: r.u16_be()?,
            x_aspect: r.u8()?,
            y_aspect: r.u8()?,
            page_width: i16::from_be_bytes(r.array()?),
            page_height: i16::from_be_bytes(r.array()?),
        })
    }
}

/// A `CRNG` colour-cycling range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColourRange {
    /// Cycling speed; [`ColourRange::RATE_60HZ`] means one step per 1/60 s.
    pub rate: u16,
    /// Bit 0 enables cycling, bit 1 reverses it.
    pub flags: u16,
    /// First palette index in the range.
    pub low: u8,
    /// Last palette index in the range.
    pub high: u8,
}

impl ColourRange {
    /// The rate value for one step per 1/60 second.
    pub const RATE_60HZ: u16 = 16_384;

    fn parse(bytes: &[u8]) -> Result<Self, Truncated> {
        let mut r = Reader::new(bytes);
        r.skip(2)?;
        Ok(Self {
            rate: r.u16_be()?,
            flags: r.u16_be()?,
            low: r.u8()?,
            high: r.u8()?,
        })
    }

    /// Whether the editor would cycle this range.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Whether the range cycles downwards.
    #[must_use]
    pub fn is_reversed(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// A decoded `.LBM` or `.BBM` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Which form the file used.
    pub form: Form,
    /// The `BMHD` fields as stored.
    pub header: BitmapHeader,
    /// The pixels, without row padding or mask plane.
    pub image: IndexedImage,
    /// The `CMAP` colours, padded with black to 256 entries.
    pub palette: Option<Palette>,
    /// Every `CRNG` chunk, in file order.
    pub ranges: Vec<ColourRange>,
    /// The `GRAB` hotspot, if present.
    pub hotspot: Option<(i16, i16)>,
    /// Chunks that were not interpreted (such as `DPPS` and `TINY`), and repeats of
    /// single-use chunks after the first, in file order.
    pub skipped: Vec<ChunkId>,
    quirks: Vec<Quirk>,
}

impl Picture {
    /// What was unusual about the file; empty for every shipped picture.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }
}

/// Decodes a `FORM PBM ` or `FORM ILBM` file. A `FORM` length past the end of the file is
/// clamped to the file and reported in [`Picture::quirks`].
///
/// # Errors
///
/// Any [`Error`]: a missing `BMHD` or `BODY`, an unsupported depth or compression, or input
/// that ends early.
pub fn parse(data: &[u8]) -> Result<Picture, Error> {
    let (form, chunks, quirk) = open_form(data)?;
    let mut header = None;
    let mut body = None;
    let mut palette = None;
    let mut ranges = Vec::new();
    let mut hotspot = None;
    let mut skipped = Vec::new();
    for chunk in chunks {
        let (id, bytes) = chunk?;
        match id {
            ChunkId::BMHD if header.is_none() => header = Some(BitmapHeader::parse(bytes)?),
            ChunkId::BODY if body.is_none() => body = Some(bytes),
            ChunkId::CMAP if palette.is_none() => palette = Some(palette_from_cmap(bytes)),
            ChunkId::CRNG => ranges.push(ColourRange::parse(bytes)?),
            ChunkId::GRAB if hotspot.is_none() => {
                let mut r = Reader::new(bytes);
                let x = i16::from_be_bytes(r.array()?);
                let y = i16::from_be_bytes(r.array()?);
                hotspot = Some((x, y));
            }
            other => skipped.push(other),
        }
    }
    let header = header.ok_or(Error::MissingChunk(ChunkId::BMHD))?;
    let body = body.ok_or(Error::MissingChunk(ChunkId::BODY))?;
    Ok(Picture {
        form,
        header,
        image: decode_body(form, &header, body)?,
        palette,
        ranges,
        hotspot,
        skipped,
        quirks: quirk.into_iter().collect(),
    })
}

/// Reads only the `CMAP` of a picture or palette file, without decoding `BODY`.
///
/// # Errors
///
/// [`Error::MissingChunk`] without a `CMAP`, or the form errors [`parse`] reports.
pub fn read_palette(data: &[u8]) -> Result<Palette, Error> {
    let (_, chunks, _) = open_form(data)?;
    for chunk in chunks {
        let (id, bytes) = chunk?;
        if id == ChunkId::CMAP {
            return Ok(palette_from_cmap(bytes));
        }
    }
    Err(Error::MissingChunk(ChunkId::CMAP))
}

fn open_form(data: &[u8]) -> Result<(Form, Chunks<'_>, Option<Quirk>), Error> {
    if data.get(..4) != Some(&ChunkId::FORM.0[..]) {
        return Err(Error::NotForm);
    }
    let mut r = Reader::new(data);
    r.skip(4)?;
    let declared = to_usize(r.u32_be()?);
    let available = r.remaining();
    let quirk = (declared > available).then_some(Quirk::FormLength { declared, available });
    let mut inner = Reader::new(r.bytes(declared.min(available))?);
    let form = Form::from_id(ChunkId(inner.array()?))?;
    Ok((form, Chunks(inner), quirk))
}

/// Walks the chunks inside a form, yielding each id and its data without padding.
struct Chunks<'a>(Reader<'a>);

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<(ChunkId, &'a [u8]), Truncated>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let item = self.read_chunk();
        if item.is_err() {
            let end = self.0.data().len();
            let _ = self.0.seek(end);
        }
        Some(item)
    }
}

impl<'a> Chunks<'a> {
    fn read_chunk(&mut self) -> Result<(ChunkId, &'a [u8]), Truncated> {
        let id = ChunkId(self.0.array()?);
        let len = to_usize(self.0.u32_be()?);
        let bytes = self.0.bytes(len)?;
        if len % 2 == 1 && !self.0.is_empty() {
            self.0.skip(1)?;
        }
        Ok((id, bytes))
    }
}

fn palette_from_cmap(bytes: &[u8]) -> Palette {
    let mut rgb = [0; PALETTE_SIZE * 3];
    let len = (bytes.len() / 3 * 3).min(rgb.len());
    if let (Some(dst), Some(src)) = (rgb.get_mut(..len), bytes.get(..len)) {
        dst.copy_from_slice(src);
    }
    Palette::from_rgb8(&rgb)
}

/// How one stored row is laid out.
struct RowLayout {
    form: Form,
    stride: usize,
    plane_bytes: usize,
    planes: usize,
}

impl RowLayout {
    fn new(form: Form, header: &BitmapHeader) -> Result<Self, Error> {
        let width = usize::from(header.width);
        let has_mask = header.masking == Masking::HasMask;
        match form {
            Form::Pbm => {
                if header.planes != 8 {
                    return Err(Error::UnsupportedPlanes(header.planes));
                }
                if has_mask {
                    return Err(Error::UnsupportedMaskPlane);
                }
                let stride = width + width % 2;
                Ok(Self {
                    form,
                    stride,
                    plane_bytes: stride,
                    planes: 1,
                })
            }
            Form::Ilbm => {
                if !(1..=8).contains(&header.planes) {
                    return Err(Error::UnsupportedPlanes(header.planes));
                }
                let planes = usize::from(header.planes);
                let plane_bytes = width.div_ceil(16) * 2;
                Ok(Self {
                    form,
                    stride: plane_bytes * (planes + usize::from(has_mask)),
                    plane_bytes,
                    planes,
                })
            }
        }
    }

    fn convert(&self, row: &[u8], out: &mut [u8]) {
        match self.form {
            Form::Pbm => out.iter_mut().zip(row).for_each(|(o, &i)| *o = i),
            Form::Ilbm => {
                out.fill(0);
                for (plane, bits) in row.chunks_exact(self.plane_bytes).take(self.planes).enumerate() {
                    let bit = 1u8 << plane;
                    for (x, o) in out.iter_mut().enumerate() {
                        // Bit 7 of each plane byte is the leftmost of its eight pixels.
                        let set = bits.get(x / 8).is_some_and(|byte| (byte >> (7 - x % 8)) & 1 != 0);
                        if set {
                            *o |= bit;
                        }
                    }
                }
            }
        }
    }
}

/// Supplies stored rows from either an uncompressed or a `ByteRun1` body.
enum Rows<'a> {
    Raw(Reader<'a>),
    Packed(ByteRun1<'a>),
}

impl Rows<'_> {
    fn fill(&mut self, row: &mut [u8]) -> bool {
        match self {
            Self::Raw(r) => r.bytes(row.len()).map(|src| row.copy_from_slice(src)).is_ok(),
            Self::Packed(decoder) => decoder.fill(row),
        }
    }
}

fn decode_body(form: Form, header: &BitmapHeader, body: &[u8]) -> Result<IndexedImage, Error> {
    let layout = RowLayout::new(form, header)?;
    let width = usize::from(header.width);
    if width == 0 || header.height == 0 {
        return Ok(IndexedImage::filled(header.width, header.height, 0));
    }
    let expected = layout.stride.saturating_mul(usize::from(header.height));
    let too_short = || Error::BodyTooShort {
        expected,
        body: body.len(),
    };
    let (capacity, mut rows) = match header.compression {
        Compression::None => (body.len(), Rows::Raw(Reader::new(body))),
        Compression::ByteRun1 => (
            body.len().saturating_mul(MAX_EXPANSION),
            Rows::Packed(ByteRun1::new(body)),
        ),
    };
    if expected > capacity {
        return Err(too_short());
    }
    let mut image = IndexedImage::filled(header.width, header.height, 0);
    let mut row = vec![0; layout.stride];
    for out in image.pixels_mut().chunks_exact_mut(width) {
        if !rows.fill(&mut row) {
            return Err(too_short());
        }
        layout.convert(&row, out);
    }
    Ok(image)
}
