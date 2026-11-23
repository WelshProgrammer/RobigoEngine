//! Synthetic sheets, tables and maps for the terrain tests.

use s2_assets::Landscape;
use s2_formats::gouraud::ShadingTable;

use super::filler::{Surface, TEXTURE_SIZE};
use s2_sim::{Node, NodeData, NodeMap};

use super::Terrain;

/// A texture sheet with texel `f(u, v)`.
pub(crate) fn texture(f: impl Fn(u8, u8) -> u8) -> Box<[u8; TEXTURE_SIZE]> {
    let texels: Vec<u8> = (0..TEXTURE_SIZE)
        .map(|i| {
            let [u, v] = u16::try_from(i).unwrap().to_le_bytes();
            f(u, v)
        })
        .collect();
    texels.into_boxed_slice().try_into().unwrap()
}

/// A shading table whose row `level` maps texel `t` to `f(level, t)`.
pub(crate) fn shading(f: impl Fn(u8, u8) -> u8) -> ShadingTable {
    let bytes: Vec<u8> = (0..TEXTURE_SIZE)
        .map(|i| {
            let [t, level] = u16::try_from(i).unwrap().to_le_bytes();
            f(level, t)
        })
        .collect();
    ShadingTable::parse(&bytes).unwrap()
}

/// Ground of terrain byte `kind` everywhere, flat, at neutral light.
pub(crate) fn flat(width: u16, height: u16, kind: u8) -> NodeMap {
    NodeMap::filled(
        width,
        height,
        NodeData {
            altitude: 0,
            terrain: [kind; 2],
            light: 64,
            ..NodeData::default()
        },
    )
    .unwrap()
}

/// A map of mixed types, heights and lights from a fixed seed. Neighbouring heights differ by at
/// most 4 (20 pixels), so no triangle folds over.
pub(crate) fn varied(width: u16, height: u16) -> NodeMap {
    let mut map = flat(width, height, 8);
    let mut seed = 0x853c_49e6_748f_ea9b_u64;
    let mut next = |range: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        u8::try_from(seed % range).unwrap()
    };
    for y in 0..height {
        for x in 0..width {
            let data = NodeData {
                altitude: next(5),
                terrain: [next(20), next(20)],
                light: 40 + next(50),
                ..NodeData::default()
            };
            map.set(Node { x, y }, data);
        }
    }
    map
}

/// `map` drawn with `texture` and `shading` as greenland.
pub(crate) fn terrain(map: NodeMap, texture: Box<[u8; TEXTURE_SIZE]>, shading: ShadingTable) -> Terrain {
    Terrain::new(map, Landscape::Greenland, texture, shading)
}

/// A `width` × `height` view of `terrain` from world pixel (`x`, `y`).
pub(crate) fn view(terrain: &Terrain, x: i32, y: i32, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0; (width * height) as usize];
    let mut surface = Surface::new(&mut pixels, width, height).unwrap();
    terrain.draw(x, y, &mut surface);
    pixels
}

/// How many pixels hold `index`.
pub(crate) fn count(pixels: &[u8], index: u8) -> usize {
    pixels.iter().map(|&p| usize::from(p == index)).sum()
}
