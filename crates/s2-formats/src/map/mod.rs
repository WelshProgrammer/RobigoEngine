//! Maps: `WORLD_V1.0` `.WLD` files and the v1.02 `WORLDnnn`/`CONTInnn` pairs.
//!
//! Both kinds decode into the same [`Map`]: a grid size, up to 15 byte layers, the 250-entry
//! area table and the animal list. `.WLD` maps (and `.SWD`, which share the format) add a
//! [`Header`] with the title, author, terrain set and headquarters.
//!
//! S2.EXE keeps 15 layers per map in memory. v1.51 files store 14 of them and skip
//! [`LayerKind::Runtime13`]; the v1.02 `WORLDnnn.DAT` files store all 15. Layers are kept by
//! that in-memory slot, so a v1.02 map and its v1.51 counterpart line up layer for layer.
//!
//! S2.EXE or the shipped data backs every layer name except [`LayerKind::Runtime13`]'s, which is
//! inferred.

mod container;
mod header;
pub mod rle;
mod write;

use crate::bytes::{Reader, Truncated};

pub use header::{Header, Terrain};
pub use rle::RleError;
pub use write::BuildError;

/// The ten bytes every `.WLD` file starts with.
pub const SIGNATURE: &[u8; 10] = b"WORLD_V1.0";

/// Entries in the area table, used or not.
pub const AREA_COUNT: usize = 250;

/// Bytes in the area table, which is also the whole of a `CONTInnn.DAT` file.
pub const AREA_TABLE_LEN: usize = AREA_COUNT * AREA_ENTRY_LEN;

/// Players a map can hold.
pub const MAX_PLAYERS: usize = 7;

const AREA_ENTRY_LEN: usize = 9;
const HEADER_LEN: usize = 82;
const ANIMALS_END: u8 = u8::MAX;

/// Why a map could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The input ended early.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The file does not start with [`SIGNATURE`].
    #[error("not a WORLD_V1.0 map")]
    BadSignature,
    /// A serialised object carries the wrong class tag.
    #[error("expected class tag {expected:#06x} at offset {offset}, found {found:#06x}")]
    UnexpectedClass {
        /// Where the tag sits.
        offset: usize,
        /// The tag the container requires there.
        expected: u16,
        /// The tag in the file.
        found: u16,
    },
    /// The container gives a zero width or height.
    #[error("map has no area: {width}x{height}")]
    Empty {
        /// Width in nodes.
        width: u16,
        /// Height in nodes.
        height: u16,
    },
    /// A layer's own size disagrees with the container's.
    #[error("layer {layer} is {width}x{height}, map is {map_width}x{map_height}")]
    LayerSize {
        /// Position of the layer in the file.
        layer: usize,
        /// Width the layer claims.
        width: u16,
        /// Height the layer claims.
        height: u16,
        /// Width the container gives.
        map_width: u16,
        /// Height the container gives.
        map_height: u16,
    },
    /// A layer's byte count is not width times height.
    #[error("layer {layer} holds {len} bytes, expected {expected}")]
    LayerLength {
        /// Position of the layer in the file.
        layer: usize,
        /// Bytes the layer claims.
        len: usize,
        /// Width times height.
        expected: usize,
    },
    /// A compressed layer does not decode.
    #[error("layer {layer}: {source}")]
    Rle {
        /// Position of the layer in the file.
        layer: usize,
        /// What went wrong.
        source: RleError,
    },
    /// A `CONTInnn.DAT` file is not exactly one area table long.
    #[error("area table is {0} bytes, expected {AREA_TABLE_LEN}")]
    AreaTableLength(usize),
}

/// Something unusual that decoding accepted without failing. The shipped maps produce none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// A layer whose buffer is the null object (class 0, length 0), as map tools write for an
    /// unused layer. The layer is left out of [`Map::layers`].
    AbsentLayer {
        /// The slot it would fill.
        layer: LayerKind,
        /// Where its layer header starts.
        offset: usize,
    },
    /// Two zero bytes sit before the layer container's tag, as in the German "Das Dreieck"
    /// map, and are skipped.
    ExtraWord {
        /// Where the two bytes start.
        offset: usize,
    },
    /// A layer stored two bytes short of its length, with the next layer header starting where
    /// its last two bytes should be. The two bytes found ahead of the container (see
    /// [`Quirk::ExtraWord`]) fill in its missing tail.
    ShortLayer {
        /// The layer.
        layer: LayerKind,
        /// Bytes missing from its data.
        missing: usize,
    },
    /// A terrain set byte above 2, kept as [`Terrain::Other`].
    Terrain(u8),
    /// A non-zero byte at header offset 84; see [`Header::unknown_54`].
    Unknown54(u8),
    /// Bytes after the animal list; see [`Map::trailer`].
    Trailer {
        /// Where they start.
        offset: usize,
        /// How many there are.
        len: usize,
    },
}

/// A node position: `x` across, `y` down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    /// Column.
    pub x: u16,
    /// Row.
    pub y: u16,
}

/// What a layer holds, named by its slot in S2.EXE's in-memory map (`slot()` gives the index).
///
/// See `docs/formats/map.md` for the evidence and confidence behind each name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum LayerKind {
    /// Node altitude in bits 0–6; bit 7 is a run-time flag, 0 in maps.
    Heights = 0,
    /// Terrain of the node's first triangle in the low six bits; bit 6 marks a harbour site.
    Terrain1 = 1,
    /// Terrain of the node's second triangle in the low six bits; bits 6 and 7 are run-time flags.
    Terrain2 = 2,
    /// Roads leaving the node; always zero in shipped maps.
    Roads = 3,
    /// Low byte of the object word: which tree, stone pile or decoration.
    ObjectIndex = 4,
    /// High byte of the object word, paired with [`Self::ObjectIndex`].
    ObjectType = 5,
    /// Animal species placed at the node, 1 to 6; the animal list repeats these.
    Animals = 6,
    /// High byte of the node's figure list head at run time; zero in v1.51 maps, 255 in
    /// v1.02 files, overwritten when a map loads.
    FigureHigh = 7,
    /// Building site size and blocking flags; S2.EXE recomputes them around each change once a
    /// game runs.
    BuildingSites = 8,
    /// Owner and per-triangle seen flags; 7 (nobody, nothing seen) in shipped maps.
    Ownership = 9,
    /// Territory strength and border edges at run time; reset when a game starts.
    Territory = 10,
    /// Resource kind, mineral and amount under the node.
    Resources = 11,
    /// Light level per node for Gouraud shading.
    Shading = 12,
    /// Runtime state v1.51 never writes; only v1.02 world files carry it. The name and meaning
    /// are inferred.
    Runtime13 = 13,
    /// Index into the area table of the land or water area the node belongs to.
    Areas = 14,
}

impl LayerKind {
    /// Every slot, in order.
    pub const ALL: [Self; 15] = [
        Self::Heights,
        Self::Terrain1,
        Self::Terrain2,
        Self::Roads,
        Self::ObjectIndex,
        Self::ObjectType,
        Self::Animals,
        Self::FigureHigh,
        Self::BuildingSites,
        Self::Ownership,
        Self::Territory,
        Self::Resources,
        Self::Shading,
        Self::Runtime13,
        Self::Areas,
    ];

    /// The 14 slots v1.51 stores, in file order.
    pub const STORED: [Self; 14] = [
        Self::Heights,
        Self::Terrain1,
        Self::Terrain2,
        Self::Roads,
        Self::ObjectIndex,
        Self::ObjectType,
        Self::Animals,
        Self::FigureHigh,
        Self::BuildingSites,
        Self::Ownership,
        Self::Territory,
        Self::Resources,
        Self::Shading,
        Self::Areas,
    ];

    /// The slot index, 0 to 14.
    #[must_use]
    pub fn slot(self) -> u8 {
        self as u8
    }

    /// The kind for a slot index, or `None` past 14.
    #[must_use]
    pub fn from_slot(slot: u8) -> Option<Self> {
        Self::ALL.get(usize::from(slot)).copied()
    }
}

/// One layer: a byte per node, row by row.
#[derive(Clone, PartialEq, Eq)]
pub struct Layer {
    kind: LayerKind,
    data: Vec<u8>,
}

impl Layer {
    /// Which slot this is.
    #[must_use]
    pub fn kind(&self) -> LayerKind {
        self.kind
    }

    /// The node values, `width * height` of them, row by row.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("kind", &self.kind)
            .field("len", &self.data.len())
            .finish()
    }
}

/// What kind of region an area table entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AreaKind {
    /// A free slot; its other fields may hold stale values.
    Unused,
    /// A connected land region.
    Land,
    /// A connected body of water.
    Water,
    /// Any other value, kept as found.
    Other(u8),
}

impl AreaKind {
    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Unused,
            1 => Self::Land,
            2 => Self::Water,
            other => Self::Other(other),
        }
    }
}

/// An entry in the area table. The [`LayerKind::Areas`] layer holds each node's entry index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Area {
    /// Land, water or unused.
    pub kind: AreaKind,
    /// A node inside the area.
    pub seed: Position,
    /// How many nodes belong to it.
    pub size: u32,
}

/// An animal placed on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Animal {
    /// Species code, 1 to 6 in shipped maps.
    pub species: u8,
    /// Where it starts.
    pub position: Position,
}

/// A decoded map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    header: Option<Header>,
    width: u16,
    height: u16,
    layers: Vec<Layer>,
    areas: Vec<Area>,
    animals: Vec<Animal>,
    trailer: Vec<u8>,
    quirks: Vec<Quirk>,
}

impl Map {
    /// Decodes a `.WLD` or `.SWD` file.
    ///
    /// Absent layers, the extra-word fault of "Das Dreieck", an unknown terrain set, a
    /// non-zero byte at offset 84 and bytes after the animal list are accepted and reported in
    /// [`Map::quirks`].
    ///
    /// # Errors
    ///
    /// [`Error`] when the signature is wrong, the input is cut short, or the layers disagree
    /// with each other on size.
    pub fn parse_wld(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        if r.bytes(SIGNATURE.len())? != SIGNATURE {
            return Err(Error::BadSignature);
        }
        let header = r.bytes(HEADER_LEN)?;
        let areas = parse_areas(r.bytes(AREA_TABLE_LEN)?)?;
        let extra = extra_word(&mut r)?;
        let grid = container::read(&mut r, container::Version::V151, extra.map(|(_, bytes)| bytes))?;
        let mut quirks = Vec::new();
        let header = Header::parse(header, grid.width, grid.height, &mut quirks)?;
        quirks.extend(extra.map(|(offset, _)| Quirk::ExtraWord { offset }));
        quirks.extend(grid.quirks);
        let animals = parse_animals(&mut r)?;
        let trailer = rest(&r);
        quirks.extend(trailer_quirk(&r));
        Ok(Self {
            header: Some(header),
            width: grid.width,
            height: grid.height,
            layers: grid.layers,
            areas,
            animals,
            trailer,
            quirks,
        })
    }

    /// Decodes a v1.02 `WORLDnnn.DAT` with its `CONTInnn.DAT` area table.
    ///
    /// These pairs have no header, so [`Map::header`] is `None`.
    ///
    /// # Errors
    ///
    /// [`Error`] when the world file is malformed or `conti` is not exactly
    /// [`AREA_TABLE_LEN`] bytes.
    pub fn parse_v102(world: &[u8], conti: &[u8]) -> Result<Self, Error> {
        if conti.len() != AREA_TABLE_LEN {
            return Err(Error::AreaTableLength(conti.len()));
        }
        let mut r = Reader::new(world);
        let grid = container::read(&mut r, container::Version::V102, None)?;
        let mut quirks = grid.quirks;
        quirks.extend(trailer_quirk(&r));
        Ok(Self {
            header: None,
            width: grid.width,
            height: grid.height,
            layers: grid.layers,
            areas: parse_areas(conti)?,
            animals: Vec::new(),
            trailer: rest(&r),
            quirks,
        })
    }

    /// What was unusual about the file, in file order; empty for every shipped map.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Title, author, terrain and players; `None` for v1.02 world files.
    #[must_use]
    pub fn header(&self) -> Option<&Header> {
        self.header.as_ref()
    }

    /// Width in nodes, as every layer records it.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height in nodes, as every layer records it.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The stored layers in slot order: 14 for v1.51 files, 15 for v1.02 ones, fewer when the
    /// file marks some absent.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The layer in `kind`'s slot, if the file stored it.
    #[must_use]
    pub fn layer(&self, kind: LayerKind) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.kind == kind)
    }

    /// The value of `kind` at (`x`, `y`), or `None` outside the map or for a missing layer.
    #[must_use]
    pub fn get(&self, kind: LayerKind, x: u16, y: u16) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = usize::from(y) * usize::from(self.width) + usize::from(x);
        self.layer(kind)?.data.get(index).copied()
    }

    /// All [`AREA_COUNT`] area table entries, unused ones included.
    #[must_use]
    pub fn areas(&self) -> &[Area] {
        &self.areas
    }

    /// The animal list that follows the layers; empty for v1.02 world files.
    #[must_use]
    pub fn animals(&self) -> &[Animal] {
        &self.animals
    }

    /// Bytes after the last recognised structure; empty in every shipped map.
    #[must_use]
    pub fn trailer(&self) -> &[u8] {
        &self.trailer
    }
}

fn parse_areas(table: &[u8]) -> Result<Vec<Area>, Error> {
    let mut r = Reader::new(table);
    (0..AREA_COUNT)
        .map(|_| {
            Ok(Area {
                kind: AreaKind::from_byte(r.u8()?),
                seed: read_position(&mut r)?,
                size: r.u32()?,
            })
        })
        .collect()
}

fn parse_animals(r: &mut Reader<'_>) -> Result<Vec<Animal>, Error> {
    let mut animals = Vec::new();
    while !r.is_empty() {
        let species = r.u8()?;
        if species == ANIMALS_END {
            break;
        }
        animals.push(Animal {
            species,
            position: read_position(r)?,
        });
    }
    Ok(animals)
}

fn read_position(r: &mut Reader<'_>) -> Result<Position, Truncated> {
    Ok(Position {
        x: r.u16()?,
        y: r.u16()?,
    })
}

/// Skips the two zero bytes some maps carry before the container tag, returning their offset
/// and value.
fn extra_word(r: &mut Reader<'_>) -> Result<Option<(usize, [u8; 2])>, Truncated> {
    let offset = r.position();
    let ahead = r.data().get(offset..).unwrap_or_default();
    let tag = container::MAP_CLASS.to_le_bytes();
    if ahead.get(..4) != Some(&[0, 0, tag[0], tag[1]]) {
        return Ok(None);
    }
    Ok(Some((offset, r.array()?)))
}

fn trailer_quirk(r: &Reader<'_>) -> Option<Quirk> {
    (!r.is_empty()).then(|| Quirk::Trailer {
        offset: r.position(),
        len: r.remaining(),
    })
}

fn rest(r: &Reader<'_>) -> Vec<u8> {
    r.data().get(r.position()..).unwrap_or_default().to_vec()
}

#[cfg(test)]
mod tests;
