//! The tables that say how each terrain type is textured and where borders are drawn.
//!
//! [`TerrainTables::ORIGINAL`] holds `S2.EXE`'s tables, copied from its data object. The
//! terrain table is at `0x1f9074`, the border table at `0x1f915c` and the texture table at
//! `0x1f93a8`; Gold has the same bytes at `0x1f934c`, `0x1f9434` and `0x1f9680`.
//! [`TerrainTables::new`] builds other tables, for tests or mods, without touching the original.

use std::borrow::Cow;
use std::fmt;

/// Rows and columns of a border table.
pub const BORDER_CLASSES: usize = 14;

/// Border strips in the texture sheet, 16 rows each from row 176.
pub const BORDER_STRIPS: u8 = 5;

/// Terrain sets the tables cover: greenland, wasteland and winter, as [`s2_assets::Landscape::set`]
/// numbers them.
pub const TERRAIN_SETS: usize = 3;

/// Terrain types a terrain byte can name.
pub const MAX_TYPES: usize = 64;

/// A terrain type: the low six bits of a terrain byte.
///
/// What a type looks like depends on the landscape's texture sheet, so types are plain numbers.
/// Type 5 is water in the greenland and winter sheets but mostly one flat colour in the wasteland
/// sheet. Which types are drawn is up to the [`TerrainTables`]; the original describes 0 to 22.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerrainType(u8);

impl TerrainType {
    /// The bits of a terrain byte that pick the type.
    pub const MASK: u8 = (1 << 6) - 1;

    /// The type a terrain byte names, ignoring its flag bits.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        Self(byte & Self::MASK)
    }

    /// The type's number.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

impl fmt::Display for TerrainType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#04x}", self.0)
    }
}

/// One row of the terrain table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeInfo {
    /// Row or column of the border table, below [`BORDER_CLASSES`].
    pub class: u8,
    /// Index into the texture table.
    pub texture: u8,
    /// Border strip (below [`BORDER_STRIPS`]) the type paints into its neighbours, by terrain set.
    pub strip: [u8; TERRAIN_SETS],
}

impl TypeInfo {
    /// A row of the terrain table.
    #[must_use]
    pub const fn new(class: u8, texture: u8, strip: [u8; TERRAIN_SETS]) -> Self {
        Self {
            class,
            texture,
            strip,
        }
    }
}

/// One row of the texture table: where a type's two triangles sample the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureEntry {
    /// `(u, v)` in 8.8 texels: the slot-1 triangle's node, BR and BL, then the slot-2 triangle's
    /// node, R and BR.
    pub uv: [(u16, u16); 6],
    /// Drawn unshaded: every vertex gets shading row 64.
    pub flat: bool,
    /// The flat colour the map screen paints a node of this terrain with (byte `+0x1a`). 0 takes
    /// the texel the first `(u, v)` pair names instead, and [`Self::MAP_TEXEL`] takes
    /// [`MAP_FLAT`].
    pub colour: u8,
}

impl TextureEntry {
    /// The colour that asks for [`MAP_FLAT`].
    pub const MAP_TEXEL: u8 = u8::MAX;

    /// A row of the texture table.
    #[must_use]
    pub const fn new(uv: [(u16, u16); 6], flat: bool, colour: u8) -> Self {
        Self { uv, flat, colour }
    }
}

/// Whether a border is drawn, by terrain set, then the receiving triangle's class, then the
/// neighbour's class.
pub type BorderTable = [[[bool; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS];

/// Why [`TerrainTables::new`] refused a set of tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TablesError {
    /// More types than a terrain byte can name.
    #[error("{0} terrain types, but a terrain byte names at most {MAX_TYPES}")]
    TooManyTypes(usize),
    /// A type's border class is outside the border table.
    #[error("terrain type {kind:#04x} has border class {class}, past {BORDER_CLASSES}")]
    Class {
        /// The type.
        kind: u8,
        /// Its class.
        class: u8,
    },
    /// A type points past the end of the texture table.
    #[error("terrain type {kind:#04x} uses texture entry {texture}, but there are {count}")]
    Texture {
        /// The type.
        kind: u8,
        /// The entry it names.
        texture: u8,
        /// Entries in the texture table.
        count: usize,
    },
    /// A type paints a border strip the sheet doesn't have.
    #[error("terrain type {kind:#04x} has border strip {strip}, past {BORDER_STRIPS}")]
    Strip {
        /// The type.
        kind: u8,
        /// The strip it names.
        strip: u8,
    },
}

/// The terrain, texture and border tables the view pass draws with.
///
/// Every type's references are checked when the tables are built, so lookups never index out of
/// range. Types past the end of the terrain table are not drawn, leaving the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainTables {
    types: Cow<'static, [TypeInfo]>,
    textures: Cow<'static, [TextureEntry]>,
    borders: BorderTable,
}

impl TerrainTables {
    /// The tables of `S2.EXE` (and Gold, which has the same bytes).
    pub const ORIGINAL: Self = Self {
        types: Cow::Borrowed(&TERRAIN),
        textures: Cow::Borrowed(&TEXTURES),
        borders: parse_borders(&BORDERS),
    };

    /// Custom tables: `types[t]` describes terrain type `t`.
    ///
    /// # Errors
    ///
    /// [`TablesError`] when there are more than [`MAX_TYPES`] types, or a type names a border
    /// class, texture entry or border strip that doesn't exist.
    pub fn new(
        types: Vec<TypeInfo>,
        textures: Vec<TextureEntry>,
        borders: BorderTable,
    ) -> Result<Self, TablesError> {
        if types.len() > MAX_TYPES {
            return Err(TablesError::TooManyTypes(types.len()));
        }
        for (kind, info) in (0..=u8::MAX).zip(&types) {
            if usize::from(info.class) >= BORDER_CLASSES {
                return Err(TablesError::Class {
                    kind,
                    class: info.class,
                });
            }
            if usize::from(info.texture) >= textures.len() {
                return Err(TablesError::Texture {
                    kind,
                    texture: info.texture,
                    count: textures.len(),
                });
            }
            if let Some(&strip) = info.strip.iter().find(|&&s| s >= BORDER_STRIPS) {
                return Err(TablesError::Strip { kind, strip });
            }
        }
        Ok(Self {
            types: Cow::Owned(types),
            textures: Cow::Owned(textures),
            borders,
        })
    }

    /// The terrain table, indexed by type.
    #[must_use]
    pub fn types(&self) -> &[TypeInfo] {
        &self.types
    }

    /// The texture table.
    #[must_use]
    pub fn textures(&self) -> &[TextureEntry] {
        &self.textures
    }

    /// The border table.
    #[must_use]
    pub fn borders(&self) -> &BorderTable {
        &self.borders
    }

    /// `kind`'s row of the terrain table; `None` past its end.
    #[must_use]
    pub fn info(&self, kind: TerrainType) -> Option<&TypeInfo> {
        self.types.get(usize::from(kind.0))
    }

    /// Where `kind`'s triangles sample the sheet; `None` past the end of the terrain table.
    #[must_use]
    pub fn texture(&self, kind: TerrainType) -> Option<&TextureEntry> {
        self.info(kind)
            .and_then(|info| self.textures.get(usize::from(info.texture)))
    }

    /// Whether a triangle of type `receiving` takes a border strip from a `neighbour` in terrain
    /// set `set`. The two types must also differ for a border to be drawn.
    #[must_use]
    pub fn border_drawn(&self, set: usize, receiving: &TypeInfo, neighbour: &TypeInfo) -> bool {
        self.borders
            .get(set)
            .is_some_and(|table| table[usize::from(receiving.class)][usize::from(neighbour.class)])
    }
}

impl Default for TerrainTables {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// Turns the `x`/`.` rows of [`BORDERS`] into a [`BorderTable`].
const fn parse_borders(rows: &[[&[u8; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS]) -> BorderTable {
    let mut table = [[[false; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS];
    let mut set = 0;
    while set < TERRAIN_SETS {
        let mut receiving = 0;
        while receiving < BORDER_CLASSES {
            let mut neighbour = 0;
            while neighbour < BORDER_CLASSES {
                table[set][receiving][neighbour] = rows[set][receiving][neighbour] == b'x';
                neighbour += 1;
            }
            receiving += 1;
        }
        set += 1;
    }
    table
}

const TERRAIN: [TypeInfo; 23] = [
    TypeInfo::new(1, 5, [3, 3, 3]),
    TypeInfo::new(2, 0, [1, 3, 1]),
    TypeInfo::new(4, 10, [0, 0, 4]),
    TypeInfo::new(5, 9, [3, 0, 4]),
    TypeInfo::new(6, 14, [2, 2, 2]),
    TypeInfo::new(7, 12, [4, 1, 4]),
    TypeInfo::new(8, 13, [4, 4, 4]),
    TypeInfo::new(9, 14, [2, 2, 2]),
    TypeInfo::new(0, 6, [3, 3, 3]),
    TypeInfo::new(0, 7, [3, 3, 3]),
    TypeInfo::new(0, 8, [3, 3, 3]),
    TypeInfo::new(3, 1, [1, 3, 1]),
    TypeInfo::new(2, 2, [1, 3, 1]),
    TypeInfo::new(3, 3, [1, 3, 1]),
    TypeInfo::new(10, 16, [2, 2, 3]),
    TypeInfo::new(0, 15, [3, 3, 3]),
    TypeInfo::new(11, 17, [2, 4, 0]),
    TypeInfo::new(12, 18, [2, 2, 2]),
    TypeInfo::new(8, 19, [1, 0, 0]),
    TypeInfo::new(13, 20, [4, 4, 4]),
    TypeInfo::new(11, 21, [2, 2, 0]),
    TypeInfo::new(11, 22, [2, 2, 0]),
    TypeInfo::new(11, 23, [2, 2, 0]),
];

const TEXTURES: [TextureEntry; 24] = [
    TextureEntry::new(
        [
            (4096, 12304),
            (8175, 20463),
            (16, 20463),
            (16, 12304),
            (8175, 12304),
            (4096, 20463),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (16384, 12304),
            (20463, 20463),
            (12304, 20463),
            (12304, 12304),
            (20463, 12304),
            (16384, 20463),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (28672, 12304),
            (32751, 20463),
            (24592, 20463),
            (24592, 12304),
            (32751, 12304),
            (28672, 20463),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (40960, 12304),
            (45039, 20463),
            (36880, 20463),
            (36880, 12304),
            (45039, 12304),
            (40960, 20463),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (53248, 16),
            (57327, 8175),
            (49168, 8175),
            (49168, 16),
            (57327, 16),
            (53248, 8175),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (4096, 24592),
            (8175, 32751),
            (16, 32751),
            (16, 24592),
            (8175, 24592),
            (4096, 32751),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (16384, 24592),
            (20463, 32751),
            (12304, 32751),
            (12304, 24592),
            (20463, 24592),
            (16384, 32751),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (28672, 24592),
            (32751, 32751),
            (24592, 32751),
            (24592, 24592),
            (32751, 24592),
            (28672, 32751),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (40960, 24592),
            (45039, 32751),
            (36880, 32751),
            (36880, 24592),
            (45039, 24592),
            (40960, 32751),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (28672, 16),
            (32751, 8175),
            (24592, 8175),
            (24592, 16),
            (32751, 16),
            (28672, 8175),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (4096, 16),
            (8175, 8175),
            (16, 8175),
            (16, 16),
            (8175, 16),
            (4096, 8175),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (53248, 4112),
            (57327, 12271),
            (49168, 12271),
            (49168, 4112),
            (57327, 4112),
            (53248, 12271),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (49535, 19327),
            (56191, 12671),
            (62847, 19327),
            (56191, 26239),
            (49535, 19583),
            (62847, 19583),
        ],
        true,
        TextureEntry::MAP_TEXEL,
    ),
    TextureEntry::new(
        [
            (49535, 19327),
            (56191, 12671),
            (62847, 19327),
            (56191, 26239),
            (49535, 19583),
            (62847, 19583),
        ],
        true,
        TextureEntry::MAP_TEXEL,
    ),
    TextureEntry::new(
        [
            (16384, 16),
            (20463, 8175),
            (12304, 8175),
            (12304, 16),
            (20463, 16),
            (16384, 8175),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (40960, 16),
            (45039, 8175),
            (36880, 8175),
            (36880, 16),
            (45039, 16),
            (40960, 8175),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (4096, 36880),
            (8175, 45039),
            (16, 45039),
            (16, 36880),
            (8175, 36880),
            (4096, 45039),
        ],
        false,
        236,
    ),
    TextureEntry::new(
        [
            (49535, 33663),
            (56191, 27007),
            (62847, 33663),
            (56191, 40575),
            (49535, 33919),
            (62847, 33919),
        ],
        true,
        57,
    ),
    TextureEntry::new(
        [
            (256, 65024),
            (0, 65024),
            (0, 65280),
            (256, 65024),
            (0, 65024),
            (0, 65280),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (16384, 36880),
            (20463, 45039),
            (12304, 45039),
            (12304, 36880),
            (20463, 36880),
            (16384, 45039),
        ],
        false,
        0,
    ),
    TextureEntry::new(
        [
            (49535, 19327),
            (56191, 12671),
            (62847, 19327),
            (56191, 26239),
            (49535, 19583),
            (62847, 19583),
        ],
        true,
        TextureEntry::MAP_TEXEL,
    ),
    TextureEntry::new(
        [
            (20736, 57104),
            (24559, 60911),
            (16912, 60911),
            (16912, 61200),
            (24559, 61200),
            (20736, 65007),
        ],
        true,
        57,
    ),
    TextureEntry::new(
        [
            (29184, 57104),
            (33007, 60911),
            (25360, 60911),
            (25360, 61200),
            (33007, 61200),
            (29184, 65007),
        ],
        true,
        57,
    ),
    TextureEntry::new(
        [
            (37632, 57104),
            (41455, 60911),
            (33808, 60911),
            (33808, 61200),
            (41455, 61200),
            (37632, 65007),
        ],
        true,
        57,
    ),
];

/// `x` where a border is drawn: set, then receiving class, then neighbour class.
const BORDERS: [[&[u8; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS] = [
    [
        b"xxxxx.x.xxx..x",
        b"xxxxx.x.xxx..x",
        b"..xxx.x.xxx..x",
        b"..xxx.x.xxx..x",
        b"....x........x",
        b"xxxxxxx.xxx..x",
        b"....x.x.xxx...",
        b"xxxxxxxxxxx..x",
        b"....x...x.....",
        b"....x.x.xxx...",
        b"....x.x.xxx.xx",
        b"xxxxxxxxxxxx.x",
        b"........xx....",
        b"......x.xxx...",
    ],
    [
        b".....xxxxxx..x",
        b".....xxxxxx..x",
        b"xx...x..x....x",
        b"xx...x..x....x",
        b"xxxxxxxxxxx..x",
        b".....x..x....x",
        b"..xx.xxxxxx...",
        b"..xx.x.xx....x",
        b"........x.....",
        b"..xx.xxxxxx...",
        b"..xx.xxxxxx..x",
        b"xxxx.xxxxxxx..",
        b"........xx....",
        b"......x.xxx...",
    ],
    [
        b"xxxxxxxxxx...x",
        b"xxxxxxxxxx...x",
        b"..xx....x....x",
        b"..xx....x....x",
        b"..xxx........x",
        b"..xx.x.......x",
        b"..xxxxxx......",
        b"..xx...x.x....",
        b"....xxxxx.....",
        b"xxxxxxxxxxx...",
        b"xxxxxxxxxxx...",
        b"xxxxxxxxxxxx..",
        b"........xx....",
        b"......x.xxx...",
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: TerrainTables = TerrainTables::ORIGINAL;

    #[test]
    fn types_ignore_flag_bits() {
        assert_eq!(TerrainType::from_byte(64 | 5).index(), 5);
        assert_eq!(TerrainType::from_byte(u8::MAX).index(), TerrainType::MASK);
    }

    #[test]
    fn the_original_describes_types_up_to_22() {
        assert!(ORIGINAL.info(TerrainType::from_byte(22)).is_some());
        assert!(ORIGINAL.info(TerrainType::from_byte(23)).is_none());
        assert!(
            ORIGINAL
                .texture(TerrainType::from_byte(TerrainType::MASK))
                .is_none()
        );
    }

    #[test]
    fn water_and_lava_are_unshaded() {
        let flat: Vec<u8> = (0..=TerrainType::MASK)
            .filter(|&t| ORIGINAL.texture(TerrainType(t)).is_some_and(|e| e.flat))
            .collect();
        assert_eq!(flat, [5, 6, 16, 19, 20, 21, 22]);
    }

    #[test]
    fn the_original_passes_its_own_checks() {
        let copy = TerrainTables::new(
            ORIGINAL.types().to_vec(),
            ORIGINAL.textures().to_vec(),
            *ORIGINAL.borders(),
        );
        assert_eq!(copy, Ok(ORIGINAL));
    }

    #[test]
    fn bad_references_are_refused() {
        let textures = vec![TextureEntry::new([(0, 0); 6], false, 0)];
        let borders = [[[false; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS];
        let build = |info| TerrainTables::new(vec![info], textures.clone(), borders);
        assert!(build(TypeInfo::new(13, 0, [4; 3])).is_ok());
        assert_eq!(
            build(TypeInfo::new(14, 0, [0; 3])),
            Err(TablesError::Class { kind: 0, class: 14 })
        );
        assert_eq!(
            build(TypeInfo::new(0, 1, [0; 3])),
            Err(TablesError::Texture {
                kind: 0,
                texture: 1,
                count: 1
            })
        );
        assert_eq!(
            build(TypeInfo::new(0, 0, [0, 5, 0])),
            Err(TablesError::Strip { kind: 0, strip: 5 })
        );
        let many = vec![TypeInfo::new(0, 0, [0; 3]); MAX_TYPES + 1];
        assert_eq!(
            TerrainTables::new(many, textures.clone(), borders),
            Err(TablesError::TooManyTypes(MAX_TYPES + 1))
        );
    }

    #[test]
    fn borders_are_one_way() {
        let class = |class| TypeInfo::new(class, 0, [0; 3]);
        assert!(ORIGINAL.border_drawn(0, &class(0), &class(13)));
        assert!(!ORIGINAL.border_drawn(0, &class(13), &class(0)));
        assert!(ORIGINAL.border_drawn(1, &class(0), &class(5)));
        assert!(!ORIGINAL.border_drawn(1, &class(0), &class(0)));
        assert!(ORIGINAL.border_drawn(2, &class(0), &class(0)));
        assert!(!ORIGINAL.border_drawn(3, &class(0), &class(0)), "no such set");
    }
}
