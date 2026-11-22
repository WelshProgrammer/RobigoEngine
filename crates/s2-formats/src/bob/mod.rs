//! BOB composite sprites (`CARRIER.BOB`, `CARRIER2.BOB`, `JOBS.BOB`) and `ROM_ANIM.DAT`.
//!
//! Walking settlers are drawn in two layers, a body and an overlay on top of it. For carriers the
//! overlay is the head plus the carried ware; for job figures it is the head plus the tool. A BOB
//! file holds 96 bodies (two body types, six directions, eight walking frames), a list of
//! overlays and a link table. The link table picks the overlay and its vertical nudge for every
//! item, body type, direction and frame. Every part is a 32-pixel-wide [`PlayerBitmap`] whose
//! rows live in a shared pixel block. The bodies share one block, and the overlays have one per
//! direction.
//!
//! [`RomAnim`] decodes `ROM_ANIM.DAT`, the Roman animation script: frames that draw a
//! `ROM_BOBS.LST` bitmap or a BOB figure, plus call, return, end and sound commands.
//!
//! See `docs/formats/bob.md` for the byte layout.

mod anim;

#[cfg(test)]
mod tests;

pub use anim::{Record, RomAnim, Sprite, Step};

use crate::bytes::{Reader, Truncated};
use crate::lst::{self, Origin, PlayerBitmap, RowReads};

/// Class tag that opens a BOB file.
pub const BOB_TAG: u16 = 502;
/// Class tag of a pixel block: a `u16` length and that many bytes.
pub const BLOCK_TAG: u16 = 501;
/// Class tag of a part: height, row offsets into a pixel block, hotspot row.
pub const PART_TAG: u16 = 500;
/// Width of every body and overlay part.
pub const PART_WIDTH: u16 = 32;
/// Hotspot column of every part; the engine subtracts it from the draw position.
pub const PART_HOTSPOT_X: i16 = 16;
/// Directions a figure can face.
pub const DIRECTIONS: usize = 6;
/// Frames in a walking cycle.
pub const FRAMES: usize = 8;
/// Body types; see [`BodyType`].
pub const BODY_TYPES: usize = 2;
/// Body parts in every BOB file: one per body type, direction and frame.
pub const BODY_PARTS: usize = BODY_TYPES * DIRECTIONS * FRAMES;
/// Links per item: one per frame, body type and direction.
pub const LINKS_PER_ITEM: usize = FRAMES * BODY_TYPES * DIRECTIONS;

/// Bytes in the smallest possible part: tag, height and hotspot row.
const MIN_PART_LEN: usize = 4;
/// Bytes per link.
const LINK_LEN: usize = 4;

/// Something unusual that decoding accepted without failing. The shipped files produce none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// A row quirk inside a body or overlay.
    Part {
        /// Which list the part belongs to.
        kind: PartKind,
        /// Index in that list.
        index: usize,
        /// What was unusual.
        quirk: lst::Quirk,
    },
    /// An overlay no link refers to, so nothing says which pixel block it reads. The parser
    /// keeps it undecoded as [`Overlay::Unlinked`].
    UnusedOverlay(u16),
    /// An overlay linked from directions whose pixel blocks decode it differently. The parser
    /// keeps the decode from the direction of its first link.
    OverlayDirections {
        /// Overlay index.
        overlay: u16,
        /// The direction of the first link, whose decode is kept.
        first: usize,
        /// Another direction linking it whose block gives other pixels or does not decode.
        second: usize,
    },
    /// The link table does not hold a whole number of items; the links past the last whole
    /// item are kept but belong to no item.
    LinkCount(usize),
    /// Bytes after the last field, ignored.
    TrailingBytes {
        /// Where they start.
        offset: usize,
        /// How many there are.
        len: usize,
    },
}

/// Everything that can go wrong while decoding a BOB file or `ROM_ANIM.DAT`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The input ended inside a field.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// A class tag other than the one the layout requires.
    #[error("expected class tag {expected:#06x} at offset {offset}, found {found:#06x}")]
    Tag {
        /// Where the tag starts.
        offset: usize,
        /// The tag the layout requires there.
        expected: u16,
        /// The tag in the file.
        found: u16,
    },
    /// A link names an overlay past the end of the overlay list.
    #[error("link {link} names overlay {overlay}; the file has {count}")]
    OverlayRange {
        /// Link index.
        link: usize,
        /// The overlay it names.
        overlay: u16,
        /// Overlays in the file.
        count: usize,
    },
    /// A body or overlay whose rows fail to decode.
    #[error("{kind} {index}: {source}")]
    Part {
        /// Which list the part belongs to.
        kind: PartKind,
        /// Index in that list.
        index: usize,
        /// What went wrong.
        source: lst::Error,
    },
}

/// The two part lists in a BOB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartKind {
    /// The 96 bodies.
    Body,
    /// The linked overlays.
    Overlay,
}

impl std::fmt::Display for PartKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Body => "body",
            Self::Overlay => "overlay",
        })
    }
}

/// The way a figure faces, in the order BOB files store them.
///
/// The compass names come from looking at the decoded figures, not from S2.EXE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    /// Facing right.
    East = 0,
    /// Facing down and right.
    SouthEast = 1,
    /// Facing down and left.
    SouthWest = 2,
    /// Facing left.
    West = 3,
    /// Facing up and left.
    NorthWest = 4,
    /// Facing up and right.
    NorthEast = 5,
}

impl Direction {
    /// Every direction in stored order.
    pub const ALL: [Self; DIRECTIONS] = [
        Self::East,
        Self::SouthEast,
        Self::SouthWest,
        Self::West,
        Self::NorthWest,
        Self::NorthEast,
    ];

    /// The direction stored at `index`, or `None` above 5.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        Some(match index {
            0 => Self::East,
            1 => Self::SouthEast,
            2 => Self::SouthWest,
            3 => Self::West,
            4 => Self::NorthWest,
            5 => Self::NorthEast,
            _ => return None,
        })
    }

    /// The stored index, 0 to 5.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// The two bodies every item can be drawn on. Which one a job uses comes from an engine table
/// outside the BOB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BodyType {
    /// The first 48 bodies.
    Slim = 0,
    /// The last 48 bodies, drawn visibly wider.
    Fat = 1,
}

impl BodyType {
    /// Both body types in stored order.
    pub const ALL: [Self; BODY_TYPES] = [Self::Slim, Self::Fat];

    /// The stored index, 0 or 1.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Which overlay to draw for one item, body type, direction and frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Link {
    /// Index into [`Bob::overlays`].
    pub overlay: u16,
    /// Added to the draw row of the overlay; -1, 0 or 1 in the shipped files.
    pub dy: i16,
}

/// An overlay part, decoded when a link says which direction's pixel block it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    /// Decoded from the block of the direction it is first linked in.
    Linked(PlayerBitmap),
    /// Never linked, so kept as stored.
    Unlinked(RawPart),
}

impl Overlay {
    /// The decoded bitmap, or `None` for an unlinked overlay.
    #[must_use]
    pub fn bitmap(&self) -> Option<&PlayerBitmap> {
        match self {
            Self::Linked(bitmap) => Some(bitmap),
            Self::Unlinked(_) => None,
        }
    }
}

/// A part as stored, without the pixel block its rows point into.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawPart {
    hotspot_y: u8,
    row_starts: Vec<u16>,
}

impl RawPart {
    /// Rows in the part.
    #[must_use]
    pub fn height(&self) -> usize {
        self.row_starts.len()
    }

    /// The hotspot row.
    #[must_use]
    pub fn hotspot_y(&self) -> u8 {
        self.hotspot_y
    }

    /// Each row's offset into the pixel block it would be drawn from.
    #[must_use]
    pub fn row_starts(&self) -> &[u16] {
        &self.row_starts
    }
}

/// A bitmap placed relative to a figure's anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer<'a> {
    /// The bitmap, whose own hotspot sits on the placed point.
    pub bitmap: &'a PlayerBitmap,
    /// Columns right of the anchor.
    pub x: i16,
    /// Rows below the anchor.
    pub y: i16,
}

/// A decoded BOB file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bob {
    bodies: Vec<PlayerBitmap>,
    overlays: Vec<Overlay>,
    links: Vec<Link>,
    quirks: Vec<Quirk>,
}

impl Bob {
    /// Decodes a whole BOB file and every part in it.
    ///
    /// The engine reads an overlay's rows from the pixel block of the direction it is drawn
    /// in. An overlay linked from several directions (one in `JOBS.BOB`) is decoded in each.
    /// The decode in its first link's direction is kept, and any disagreement is a quirk. An
    /// overlay no link names stays undecoded. A link table that is not a whole number of items
    /// is a quirk, as are bytes after it. Parts share their pixel blocks, so all their rows
    /// together may read at most [`lst::MAX_EXPANSION`] bytes per byte of the file.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`], [`Error::Tag`], [`Error::OverlayRange`], or [`Error::Part`]
    /// wrapping the decoder's error when a part's rows cannot be read or pass that limit
    /// ([`lst::Error::RowReads`]).
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        expect_tag(&mut r, BOB_TAG)?;
        let body_block = read_block(&mut r)?;
        let body_parts = (0..BODY_PARTS)
            .map(|_| PartRef::read(&mut r))
            .collect::<Result<Vec<_>, _>>()?;
        let mut overlay_blocks: [&[u8]; DIRECTIONS] = [&[]; DIRECTIONS];
        for block in &mut overlay_blocks {
            *block = read_block(&mut r)?;
        }
        let count = usize::from(r.u16()?);
        let mut overlay_parts = Vec::with_capacity(count.min(r.remaining() / MIN_PART_LEN));
        for _ in 0..count {
            overlay_parts.push(PartRef::read(&mut r)?);
        }
        let links = read_links(&mut r)?;
        let mut quirks = Vec::new();
        let mut reads = RowReads::for_input(data.len());

        let mut bodies = Vec::with_capacity(BODY_PARTS);
        for (index, part) in body_parts.iter().enumerate() {
            bodies.push(part.decode(body_block, PartKind::Body, index, &mut reads, &mut quirks)?);
        }
        let directions = overlay_directions(&links, count)?;
        let mut overlays = Vec::with_capacity(overlay_parts.len());
        for (index, (part, linked)) in overlay_parts.iter().zip(directions).enumerate() {
            overlays.push(decode_overlay(
                part,
                index,
                linked,
                &overlay_blocks,
                &mut reads,
                &mut quirks,
            )?);
        }
        if links.len() % LINKS_PER_ITEM != 0 {
            quirks.push(Quirk::LinkCount(links.len()));
        }
        if !r.is_empty() {
            quirks.push(Quirk::TrailingBytes {
                offset: r.position(),
                len: r.remaining(),
            });
        }
        Ok(Self {
            bodies,
            overlays,
            links,
            quirks,
        })
    }

    /// What was unusual about the file; empty for every shipped BOB.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Whole items the link table covers: wares in the carrier files, figure types in
    /// `JOBS.BOB`.
    #[must_use]
    pub fn items(&self) -> usize {
        self.links.len() / LINKS_PER_ITEM
    }

    /// All 96 bodies, ordered by body type, then direction, then frame.
    #[must_use]
    pub fn bodies(&self) -> &[PlayerBitmap] {
        &self.bodies
    }

    /// Every overlay in file order, linked or not.
    #[must_use]
    pub fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }

    /// The decoded overlay at `index`, or `None` past the end or for an unlinked overlay.
    #[must_use]
    pub fn overlay(&self, index: usize) -> Option<&PlayerBitmap> {
        self.overlays.get(index)?.bitmap()
    }

    /// The link table, ordered by item, then frame, then body type, then direction.
    #[must_use]
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// The body for `body`, `direction` and `frame`, or `None` when `frame` is 8 or more.
    #[must_use]
    pub fn body(&self, body: BodyType, direction: Direction, frame: u8) -> Option<&PlayerBitmap> {
        self.bodies.get(Self::body_index(body, direction, frame)?)
    }

    /// Where the body for `body`, `direction` and `frame` sits in [`Self::bodies`]:
    /// `48·body + 8·direction + frame`, or `None` when `frame` is 8 or more.
    #[must_use]
    pub const fn body_index(body: BodyType, direction: Direction, frame: u8) -> Option<usize> {
        let frame = frame as usize;
        if frame >= FRAMES {
            return None;
        }
        Some((body.index() * DIRECTIONS + direction.index()) * FRAMES + frame)
    }

    /// The link for one item, body type, direction and frame, or `None` when `item` or
    /// `frame` is out of range or the link table stops short of it.
    #[must_use]
    pub fn link(&self, item: usize, body: BodyType, direction: Direction, frame: u8) -> Option<Link> {
        self.links
            .get(Self::link_index(item, body, direction, frame)?)
            .copied()
    }

    /// Where the link for one item, body type, direction and frame sits in [`Self::links`]:
    /// `96·item + 12·frame + 6·body + direction`, or `None` when `frame` is 8 or more.
    #[must_use]
    pub const fn link_index(item: usize, body: BodyType, direction: Direction, frame: u8) -> Option<usize> {
        let frame = frame as usize;
        if frame >= FRAMES {
            return None;
        }
        let within = (frame * BODY_TYPES + body.index()) * DIRECTIONS + direction.index();
        match item.checked_mul(LINKS_PER_ITEM) {
            Some(first) => first.checked_add(within),
            None => None,
        }
    }

    /// The layers to draw, body first, for one item, body type, direction and frame, or `None`
    /// when `item` or `frame` is out of range. Each layer's hotspot goes on the anchor point
    /// shifted by the layer's offset.
    #[must_use]
    pub fn composite(
        &self,
        item: usize,
        body: BodyType,
        direction: Direction,
        frame: u8,
    ) -> Option<[Layer<'_>; 2]> {
        let link = self.link(item, body, direction, frame)?;
        let body = self.body(body, direction, frame)?;
        let overlay = self.overlay(usize::from(link.overlay))?;
        Some([
            Layer {
                bitmap: body,
                x: 0,
                y: 0,
            },
            Layer {
                bitmap: overlay,
                x: 0,
                y: link.dy,
            },
        ])
    }
}

/// A part before decoding: its height, hotspot row and raw `u16` row offsets.
#[derive(Debug, Clone, Copy)]
struct PartRef<'a> {
    height: u8,
    hotspot_y: u8,
    offsets: &'a [u8],
}

impl<'a> PartRef<'a> {
    fn read(r: &mut Reader<'a>) -> Result<Self, Error> {
        expect_tag(r, PART_TAG)?;
        let height = r.u8()?;
        let offsets = r.bytes(usize::from(height) * 2)?;
        let hotspot_y = r.u8()?;
        Ok(Self {
            height,
            hotspot_y,
            offsets,
        })
    }

    fn row_starts(&self) -> impl Iterator<Item = u16> {
        let (pairs, _) = self.offsets.as_chunks::<2>();
        pairs.iter().map(|&pair| u16::from_le_bytes(pair))
    }

    fn bitmap(
        &self,
        block: &[u8],
        kind: PartKind,
        index: usize,
        reads: &mut RowReads,
    ) -> Result<PlayerBitmap, Error> {
        let origin = Origin {
            x: PART_HOTSPOT_X,
            y: i16::from(self.hotspot_y),
        };
        let starts = self.row_starts().map(usize::from);
        PlayerBitmap::decode_rows_within(origin, PART_WIDTH, u16::from(self.height), block, starts, reads)
            .map_err(|source| Error::Part { kind, index, source })
    }

    fn decode(
        &self,
        block: &[u8],
        kind: PartKind,
        index: usize,
        reads: &mut RowReads,
        quirks: &mut Vec<Quirk>,
    ) -> Result<PlayerBitmap, Error> {
        let bitmap = self.bitmap(block, kind, index, reads)?;
        quirks.extend(bitmap.quirks().iter().map(|quirk| Quirk::Part {
            kind,
            index,
            quirk: quirk.clone(),
        }));
        Ok(bitmap)
    }

    fn to_raw(self) -> RawPart {
        RawPart {
            hotspot_y: self.hotspot_y,
            row_starts: self.row_starts().collect(),
        }
    }
}

/// Which directions link an overlay, and the direction of its first link.
#[derive(Debug, Clone, Copy, Default)]
struct Linked {
    first: Option<usize>,
    /// Bit `d` is set when a link draws the overlay in direction `d`.
    mask: u8,
}

/// Decodes overlay `index` in the direction of its first link, checking the other linked
/// directions give the same pixels.
fn decode_overlay(
    part: &PartRef<'_>,
    index: usize,
    linked: Linked,
    blocks: &[&[u8]; DIRECTIONS],
    reads: &mut RowReads,
    quirks: &mut Vec<Quirk>,
) -> Result<Overlay, Error> {
    let overlay = u16::try_from(index).unwrap_or(u16::MAX);
    let Some((first, block)) = linked.first.and_then(|d| Some((d, *blocks.get(d)?))) else {
        quirks.push(Quirk::UnusedOverlay(overlay));
        return Ok(Overlay::Unlinked(part.to_raw()));
    };
    let bitmap = part.decode(block, PartKind::Overlay, index, reads, quirks)?;
    for (second, block) in blocks.iter().enumerate() {
        if second == first || linked.mask & (1 << second) == 0 {
            continue;
        }
        let same = match part.bitmap(block, PartKind::Overlay, index, reads) {
            Ok(other) => other.pixels() == bitmap.pixels(),
            Err(
                error @ Error::Part {
                    source: lst::Error::RowReads { .. },
                    ..
                },
            ) => return Err(error),
            Err(_) => false,
        };
        if !same {
            quirks.push(Quirk::OverlayDirections {
                overlay,
                first,
                second,
            });
        }
    }
    Ok(Overlay::Linked(bitmap))
}

fn expect_tag(r: &mut Reader<'_>, expected: u16) -> Result<(), Error> {
    let offset = r.position();
    let found = r.u16()?;
    if found == expected {
        Ok(())
    } else {
        Err(Error::Tag {
            offset,
            expected,
            found,
        })
    }
}

fn read_block<'a>(r: &mut Reader<'a>) -> Result<&'a [u8], Error> {
    expect_tag(r, BLOCK_TAG)?;
    let len = usize::from(r.u16()?);
    Ok(r.bytes(len)?)
}

fn read_links(r: &mut Reader<'_>) -> Result<Vec<Link>, Error> {
    let count = usize::from(r.u16()?);
    let (raw, _) = r.bytes(count * LINK_LEN)?.as_chunks::<LINK_LEN>();
    Ok(raw
        .iter()
        .map(|&[o0, o1, d0, d1]| Link {
            overlay: u16::from_le_bytes([o0, o1]),
            dy: i16::from_le_bytes([d0, d1]),
        })
        .collect())
}

/// For each overlay, the directions links draw it in and the direction of the first such link.
fn overlay_directions(links: &[Link], count: usize) -> Result<Vec<Linked>, Error> {
    let mut linked = vec![Linked::default(); count];
    for (index, link) in links.iter().enumerate() {
        let entry = linked
            .get_mut(usize::from(link.overlay))
            .ok_or(Error::OverlayRange {
                link: index,
                overlay: link.overlay,
                count,
            })?;
        // Direction is the innermost index of the link table; see `Bob::link`.
        let direction = index % DIRECTIONS;
        entry.first.get_or_insert(direction);
        entry.mask |= 1 << direction;
    }
    Ok(linked)
}
