//! Building maps in memory and writing them as `.WLD` files.

use super::container::{BUFFER_CLASS, LAYER_CLASS, MAP_CLASS, NULL_CLASS};
use super::header::{AUTHOR_LEN, NO_HEADQUARTERS, TITLE_LEN, TITLE_SPAN};
use super::{
    ANIMALS_END, AREA_COUNT, Animal, Area, AreaKind, Header, Layer, LayerKind, MAX_PLAYERS, Map, Position,
    SIGNATURE, Terrain,
};
use crate::text::cp437;

/// Why a map could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    /// A zero width or height.
    #[error("map has no area: {width}x{height}")]
    Empty {
        /// Width in nodes.
        width: u16,
        /// Height in nodes.
        height: u16,
    },
    /// A title or author that does not fit its field or has a character CP437 lacks.
    #[error("{0:?} does not fit a 19-character CP437 field")]
    Text(String),
    /// A layer whose length is not width times height.
    #[error("layer {kind:?} holds {len} bytes, expected {expected}")]
    LayerLength {
        /// The layer.
        kind: LayerKind,
        /// Bytes given.
        len: usize,
        /// Width times height.
        expected: usize,
    },
    /// The same layer given twice, or [`LayerKind::Runtime13`], which `.WLD` files do not store.
    #[error("layer {0:?} cannot be stored")]
    Layer(LayerKind),
    /// More than [`AREA_COUNT`] areas.
    #[error("{0} areas, at most {AREA_COUNT}")]
    Areas(usize),
}

impl Header {
    /// A header for a new map: CP437 `title` and `author` of at most 19 characters each, the
    /// landscape, the player count and each player's headquarters. The stored size is the
    /// map's, the byte at offset 84 and the leaders are 0.
    ///
    /// # Errors
    ///
    /// [`BuildError::Text`] when the title or author does not fit.
    pub fn new(
        title: &str,
        author: &str,
        terrain: Terrain,
        player_count: u8,
        headquarters: [Option<Position>; MAX_PLAYERS],
    ) -> Result<Self, BuildError> {
        let title = text(title)?;
        let mut author_field = [0; AUTHOR_LEN];
        let author = text(author)?;
        author_field[..author.len()].copy_from_slice(&author);
        let positions = [
            headquarters.map(|hq| hq.map_or(NO_HEADQUARTERS, |p| p.x)),
            headquarters.map(|hq| hq.map_or(NO_HEADQUARTERS, |p| p.y)),
        ];
        let mut title_span = [0; TITLE_SPAN];
        title_span[..title.len()].copy_from_slice(&title);
        Ok(Self {
            title,
            title_span,
            author: author_field,
            stored_width: 0,
            stored_height: 0,
            overwritten: false,
            terrain,
            player_count,
            headquarters,
            positions,
            unknown_54: 0,
            leaders: [0; MAX_PLAYERS],
        })
    }

    /// The same header with `leaders` as the face each player takes.
    #[must_use]
    pub const fn with_leaders(mut self, leaders: [u8; MAX_PLAYERS]) -> Self {
        self.leaders = leaders;
        self
    }

    /// A header with no title, author or headquarters, for a map that has none.
    fn blank() -> Self {
        Self {
            title: Vec::new(),
            title_span: [0; TITLE_SPAN],
            author: [0; AUTHOR_LEN],
            stored_width: 0,
            stored_height: 0,
            overwritten: false,
            terrain: Terrain::Greenland,
            player_count: 0,
            headquarters: [None; MAX_PLAYERS],
            positions: [[NO_HEADQUARTERS; MAX_PLAYERS]; 2],
            unknown_54: 0,
            leaders: [0; MAX_PLAYERS],
        }
    }

    /// Appends the 82 header bytes.
    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self.title_span);
        out.push(self.terrain.byte());
        out.push(self.player_count);
        out.extend(self.author);
        for word in self.positions.as_flattened() {
            out.extend(word.to_le_bytes());
        }
        out.push(self.unknown_54);
        out.extend(self.leaders);
    }
}

impl Layer {
    /// A layer of `kind` holding `data`, a byte per node row by row.
    #[must_use]
    pub fn new(kind: LayerKind, data: Vec<u8>) -> Self {
        Self { kind, data }
    }
}

impl Map {
    /// A new map of `width` by `height` nodes with `header`, the given layers (any left out are
    /// stored as absent), the areas (padded with unused entries to [`AREA_COUNT`]) and the
    /// animals.
    ///
    /// # Errors
    ///
    /// [`BuildError`] for an empty map, a layer of the wrong length, a repeated or unstorable
    /// layer, or too many areas.
    pub fn new(
        mut header: Header,
        width: u16,
        height: u16,
        layers: Vec<Layer>,
        mut areas: Vec<Area>,
        animals: Vec<Animal>,
    ) -> Result<Self, BuildError> {
        if width == 0 || height == 0 {
            return Err(BuildError::Empty { width, height });
        }
        let expected = usize::from(width) * usize::from(height);
        let mut sorted: Vec<Layer> = Vec::with_capacity(layers.len());
        for layer in layers {
            if layer.kind == LayerKind::Runtime13 || sorted.iter().any(|l| l.kind == layer.kind) {
                return Err(BuildError::Layer(layer.kind));
            }
            if layer.data.len() != expected {
                return Err(BuildError::LayerLength {
                    kind: layer.kind,
                    len: layer.data.len(),
                    expected,
                });
            }
            sorted.push(layer);
        }
        sorted.sort_by_key(|layer| layer.kind);
        if areas.len() > AREA_COUNT {
            return Err(BuildError::Areas(areas.len()));
        }
        areas.resize(
            AREA_COUNT,
            Area {
                kind: AreaKind::Unused,
                seed: Position::default(),
                size: 0,
            },
        );
        header.stored_width = width;
        header.stored_height = height;
        header.overwritten = false;
        header.title_span[TITLE_LEN..TITLE_LEN + 2].copy_from_slice(&width.to_le_bytes());
        header.title_span[TITLE_LEN + 2..].copy_from_slice(&height.to_le_bytes());
        Ok(Self {
            header: Some(header),
            width,
            height,
            layers: sorted,
            areas,
            animals,
            trailer: Vec::new(),
            quirks: Vec::new(),
        })
    }

    /// The map as a `.WLD` file: the header (a blank greenland one, with no size, for a v1.02
    /// map), the area
    /// table, the 14 stored layers in the tagged container, the animal list and any trailer.
    /// Decoding the result gives back the map, quirks aside.
    #[must_use]
    pub fn to_wld(&self) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        let blank = Header::blank();
        let header = self.header.as_ref().unwrap_or(&blank);
        header.write(&mut out);
        for area in &self.areas {
            out.push(area_byte(area.kind));
            out.extend(area.seed.x.to_le_bytes());
            out.extend(area.seed.y.to_le_bytes());
            out.extend(area.size.to_le_bytes());
        }
        out.extend(MAP_CLASS.to_le_bytes());
        out.extend([0; 4]);
        out.extend(self.width.to_le_bytes());
        out.extend(self.height.to_le_bytes());
        for kind in LayerKind::STORED {
            out.extend(LAYER_CLASS.to_le_bytes());
            out.extend([0; 4]);
            out.extend(self.width.to_le_bytes());
            out.extend(self.height.to_le_bytes());
            if let Some(layer) = self.layer(kind) {
                out.extend(BUFFER_CLASS.to_le_bytes());
                out.extend(u32::try_from(layer.data.len()).unwrap_or(u32::MAX).to_le_bytes());
                out.extend(&layer.data);
            } else {
                out.extend(NULL_CLASS.to_le_bytes());
                out.extend([0; 4]);
            }
        }
        for animal in &self.animals {
            out.push(animal.species);
            out.extend(animal.position.x.to_le_bytes());
            out.extend(animal.position.y.to_le_bytes());
        }
        out.push(ANIMALS_END);
        out.extend(&self.trailer);
        out
    }
}

fn area_byte(kind: AreaKind) -> u8 {
    match kind {
        AreaKind::Unused => 0,
        AreaKind::Land => 1,
        AreaKind::Water => 2,
        AreaKind::Other(byte) => byte,
    }
}

/// `text` in CP437, if it fits a field with room for its terminator.
fn text(text: &str) -> Result<Vec<u8>, BuildError> {
    let bytes: Option<Vec<u8>> = text.chars().map(cp437::encode_char).collect();
    bytes
        .filter(|bytes| bytes.len() < TITLE_LEN && !bytes.contains(&0))
        .ok_or_else(|| BuildError::Text(text.to_owned()))
}
