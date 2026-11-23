//! Blue Byte LST archives: bitmaps, palettes, fonts and sound items.
//!
//! An LST file is a numbered list of slots, each empty or holding one typed item. The same item
//! encoding appears without the archive around it in `FONT14.FNT` and in the `IO.DAT` and
//! `RESOURCE.DAT` files, so [`Item::parse`] and [`Item::parse_tagged`] decode a single item from
//! any byte offset. The bitmap decoders ([`Bitmap::decode_rows`], [`PlayerBitmap::decode_rows`],
//! [`Shadow::decode_rows`]) take explicit row starts so other containers can reuse them.
//!
//! See `docs/formats/lst.md` for the byte layout.

mod bitmap;
mod font;
mod player;
mod sound;

#[cfg(test)]
mod tests;

pub use bitmap::{Bitmap, MAX_EXPANSION, Mask, Origin, RawBitmap, Shadow};
pub(crate) use bitmap::{Budget, RowReads};
pub use font::{FIRST_GLYPH, Font, GLYPH_COUNT};
pub use player::{PLAYER_SHADES, Pixel, PlayerBitmap, PlayerColour, Shade};
pub use sound::Sound;

use crate::Palette;
use crate::bytes::{Reader, Truncated};
use crate::palette::PALETTE_SIZE;

/// The first two bytes of every LST archive, read little-endian.
pub const MAGIC: u16 = 20_000;

/// Everything that can go wrong while decoding an LST archive or one of its items.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The input ended inside a field.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The file does not start with [`MAGIC`].
    #[error("not an LST archive: magic {0:#06x}")]
    Magic(u16),
    /// An item type id this parser does not know.
    #[error("unknown item type {0}")]
    UnknownType(u16),
    /// A font glyph that is neither a bitmap (2) nor a player-colour bitmap (4).
    #[error("font glyph has item type {0}; only bitmaps (2) and player bitmaps (4) are supported")]
    GlyphType(u16),
    /// A file that should hold a font holds another item type.
    #[error("expected a font item (3), found type {0}")]
    NotAFont(u16),
    /// The declared size needs more pixels than the input can describe; see [`MAX_EXPANSION`].
    #[error("{width}x{height} bitmap is too large for {input} bytes of input")]
    TooLarge {
        /// Declared width.
        width: u16,
        /// Declared height.
        height: u16,
        /// Bytes available to describe it.
        input: usize,
    },
    /// A bitmap's rows read more than [`MAX_EXPANSION`] bytes per byte of its input in total.
    /// Only rows that start inside one another can do that, and no shipped bitmap's rows share
    /// data.
    #[error("row {row} reads past the limit for {input} bytes of bitmap input")]
    RowReads {
        /// The row that went past the limit.
        row: u16,
        /// Bytes available to describe the bitmap.
        input: usize,
    },
    /// Fewer row starts than rows were supplied to a `decode_rows` call.
    #[error("no start offset for row {row}")]
    MissingRow {
        /// The first row without a start.
        row: u16,
    },
    /// A row start lies past the end of the bitmap data.
    #[error("row {row} starts at {offset}, outside the bitmap data")]
    RowStart {
        /// Row index.
        row: u16,
        /// Offset into the bitmap data.
        offset: usize,
    },
    /// A raw bitmap whose pixel count does not match its dimensions.
    #[error("raw bitmap is {width}x{height} but holds {len} bytes")]
    RawSize {
        /// Declared width.
        width: u16,
        /// Declared height.
        height: u16,
        /// Pixel bytes present.
        len: usize,
    },
    /// An error inside one archive slot.
    #[error("item {index}: {source}")]
    Item {
        /// Slot index.
        index: u32,
        /// What went wrong.
        source: Box<Error>,
    },
}

/// Something unusual that decoding accepted without failing. The shipped S2 and Gold data
/// produce none; other releases and editor-written files can.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// A slot flag other than 0 (empty) or 1 (used). The slot reads as empty and the next slot
    /// starts right after the flag.
    SlotFlag {
        /// Slot index.
        slot: u32,
        /// Where the flag sits in the archive.
        offset: usize,
        /// The flag found.
        flag: u16,
    },
    /// Bytes after the last slot of an archive or after a stand-alone font.
    TrailingBytes {
        /// Where they start.
        offset: usize,
        /// How many there are.
        len: usize,
    },
    /// A type 2, 4 or 7 bitmap with a size but a data length of 0, decoded as fully
    /// transparent.
    NoData {
        /// Declared width.
        width: u16,
        /// Declared height.
        height: u16,
    },
    /// A row whose runs cover more pixels than the bitmap is wide; the excess is dropped.
    RowOverrun {
        /// Row index.
        row: u16,
        /// Pixels the row covered.
        decoded: usize,
        /// Declared width.
        width: u16,
    },
    /// A draw/skip row that ends before the width; the rest of it is transparent.
    RowShort {
        /// Row index.
        row: u16,
        /// Pixels the row covered.
        decoded: usize,
        /// Declared width.
        width: u16,
    },
    /// A player-colour run with a shade above 3, kept as found (see [`Shade`]).
    Shade {
        /// Row index.
        row: u16,
        /// Column the run starts at.
        x: u16,
        /// The shade byte.
        shade: u8,
    },
    /// A quirk inside one archive slot.
    Item {
        /// Slot index.
        slot: u32,
        /// What was unusual.
        quirk: Box<Quirk>,
    },
    /// A quirk inside one font glyph.
    Glyph {
        /// Character code.
        code: u8,
        /// What was unusual.
        quirk: Box<Quirk>,
    },
}

/// The type ids an item can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ItemType {
    /// Sound effect or XMIDI song.
    Sound = 1,
    /// Run-length encoded bitmap.
    Bitmap = 2,
    /// Font of player-colour glyphs.
    Font = 3,
    /// Bitmap with player-colour runs.
    PlayerBitmap = 4,
    /// 256-colour palette.
    Palette = 5,
    /// Shadow mask.
    Shadow = 7,
    /// Uncompressed bitmap.
    RawBitmap = 14,
}

impl ItemType {
    /// The id as stored in the file.
    #[must_use]
    pub const fn id(self) -> u16 {
        self as u16
    }

    /// The type for a stored id, if it is one this parser knows.
    #[must_use]
    pub const fn from_id(id: u16) -> Option<Self> {
        Some(match id {
            1 => Self::Sound,
            2 => Self::Bitmap,
            3 => Self::Font,
            4 => Self::PlayerBitmap,
            5 => Self::Palette,
            7 => Self::Shadow,
            14 => Self::RawBitmap,
            _ => return None,
        })
    }
}

impl TryFrom<u16> for ItemType {
    type Error = Error;

    fn try_from(id: u16) -> Result<Self, Error> {
        Self::from_id(id).ok_or(Error::UnknownType(id))
    }
}

/// One decoded archive slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// An unused slot.
    Empty,
    /// Type 1.
    Sound(Sound),
    /// Type 2.
    Bitmap(Bitmap),
    /// Type 3.
    Font(Font),
    /// Type 4.
    PlayerBitmap(PlayerBitmap),
    /// Type 5.
    Palette(Box<Palette>),
    /// Type 7.
    Shadow(Shadow),
    /// Type 14.
    RawBitmap(RawBitmap),
}

impl Item {
    /// Decodes the body of one item of type `kind` from the start of `data`. Returns the item
    /// and how many bytes it used. For containers that store the type id apart from the body.
    ///
    /// # Errors
    ///
    /// Any [`Error`] the item's encoding can raise; [`Error::Truncated`] offsets are relative
    /// to `data` or, inside a bitmap, to that bitmap's pixel data.
    pub fn parse(kind: ItemType, data: &[u8]) -> Result<(Self, usize), Error> {
        let mut r = Reader::new(data);
        let item = Self::read(&mut r, kind, &mut Budget::for_input(data.len()))?;
        Ok((item, r.position()))
    }

    /// Decodes a `u16` type id followed by that item's body, the layout of `FONT14.FNT` and of
    /// each entry in `IO.DAT` and `RESOURCE.DAT`. Type id 0 decodes as [`Item::Empty`].
    ///
    /// # Errors
    ///
    /// [`Error::UnknownType`] for an unknown id, otherwise as [`Item::parse`].
    pub fn parse_tagged(data: &[u8]) -> Result<(Self, usize), Error> {
        Self::parse_tagged_within(data, &mut Budget::for_input(data.len()))
    }

    /// As [`Item::parse_tagged`], but bitmaps without data draw on a budget shared with the
    /// rest of a container.
    pub(crate) fn parse_tagged_within(data: &[u8], budget: &mut Budget) -> Result<(Self, usize), Error> {
        let mut r = Reader::new(data);
        let item = match r.u16()? {
            0 => Self::Empty,
            id => Self::read(&mut r, ItemType::try_from(id)?, budget)?,
        };
        Ok((item, r.position()))
    }

    /// The stored type, or `None` for an empty slot.
    #[must_use]
    pub fn kind(&self) -> Option<ItemType> {
        Some(match self {
            Self::Empty => return None,
            Self::Sound(_) => ItemType::Sound,
            Self::Bitmap(_) => ItemType::Bitmap,
            Self::Font(_) => ItemType::Font,
            Self::PlayerBitmap(_) => ItemType::PlayerBitmap,
            Self::Palette(_) => ItemType::Palette,
            Self::Shadow(_) => ItemType::Shadow,
            Self::RawBitmap(_) => ItemType::RawBitmap,
        })
    }

    /// What was unusual about the item; empty for every shipped item.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        match self {
            Self::Bitmap(b) => b.quirks(),
            Self::Font(f) => f.quirks(),
            Self::PlayerBitmap(p) => p.quirks(),
            Self::Shadow(s) => s.quirks(),
            Self::Empty | Self::Sound(_) | Self::Palette(_) | Self::RawBitmap(_) => &[],
        }
    }

    fn read(r: &mut Reader<'_>, kind: ItemType, budget: &mut Budget) -> Result<Self, Error> {
        Ok(match kind {
            ItemType::Sound => Self::Sound(Sound::read(r)?),
            ItemType::Bitmap => Self::Bitmap(Bitmap::read(r, budget)?),
            ItemType::Font => Self::Font(Font::read(r, budget)?),
            ItemType::PlayerBitmap => Self::PlayerBitmap(PlayerBitmap::read(r, budget)?),
            ItemType::Palette => Self::Palette(Box::new(read_palette(r)?)),
            ItemType::Shadow => Self::Shadow(Shadow::read(r, budget)?),
            ItemType::RawBitmap => Self::RawBitmap(RawBitmap::read(r)?),
        })
    }
}

/// Reads a type 5 body: a `u16` that is always 256 in the shipped data, then 256 RGB
/// triples with 8 bits per channel.
fn read_palette(r: &mut Reader<'_>) -> Result<Palette, Error> {
    r.u16()?;
    Ok(Palette::from_rgb8(&r.array::<{ PALETTE_SIZE * 3 }>()?))
}

/// A decoded LST archive: every slot in file order, empty ones included.
///
/// Decoding accepts anything the game's data and editors produce, beyond what the shipped files
/// contain. [`Archive::quirks`] lists anything unusual it let through.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Archive {
    items: Vec<Item>,
    trailing: Vec<u8>,
    quirks: Vec<Quirk>,
}

impl Archive {
    /// Decodes a whole LST file. A slot flag other than 0 or 1 reads as an empty slot, and
    /// bytes after the last slot are kept (see [`Archive::trailing`]); both are reported as
    /// quirks.
    ///
    /// # Errors
    ///
    /// [`Error::Magic`] when the file is not an LST archive, and [`Error::Item`] wrapping the
    /// cause when a slot fails.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let magic = r.u16()?;
        if magic != MAGIC {
            return Err(Error::Magic(magic));
        }
        let count = r.u32()?;
        let mut items = Vec::with_capacity(crate::bytes::to_usize(count).min(r.remaining() / 2));
        let mut quirks = Vec::new();
        let mut budget = Budget::for_input(data.len());
        for slot in 0..count {
            let item = read_slot(&mut r, slot, &mut budget, &mut quirks).map_err(|source| Error::Item {
                index: slot,
                source: Box::new(source),
            })?;
            quirks.extend(item.quirks().iter().map(|quirk| Quirk::Item {
                slot,
                quirk: Box::new(quirk.clone()),
            }));
            items.push(item);
        }
        let offset = r.position();
        let trailing = data.get(offset..).unwrap_or_default().to_vec();
        if !trailing.is_empty() {
            quirks.push(Quirk::TrailingBytes {
                offset,
                len: trailing.len(),
            });
        }
        Ok(Self {
            items,
            trailing,
            quirks,
        })
    }

    /// Bytes after the last slot; empty in every shipped archive.
    #[must_use]
    pub fn trailing(&self) -> &[u8] {
        &self.trailing
    }

    /// What was unusual about the archive or its items, in file order.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Every slot in order.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The slot at `index`, or `None` past the end.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Item> {
        self.items.get(index)
    }

    /// Number of slots, empty ones included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the archive has no slots at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterates over the slots in order.
    pub fn iter(&self) -> std::slice::Iter<'_, Item> {
        self.items.iter()
    }

    /// Takes the slots out of the archive.
    #[must_use]
    pub fn into_items(self) -> Vec<Item> {
        self.items
    }
}

impl<'a> IntoIterator for &'a Archive {
    type Item = &'a Item;
    type IntoIter = std::slice::Iter<'a, Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

fn read_slot(
    r: &mut Reader<'_>,
    slot: u32,
    budget: &mut Budget,
    quirks: &mut Vec<Quirk>,
) -> Result<Item, Error> {
    let offset = r.position();
    match r.u16()? {
        0 => Ok(Item::Empty),
        1 => {
            let kind = ItemType::try_from(r.u16()?)?;
            Item::read(r, kind, budget)
        }
        flag => {
            quirks.push(Quirk::SlotFlag { slot, offset, flag });
            Ok(Item::Empty)
        }
    }
}
