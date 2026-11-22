//! Landscape drawing on the CPU, matching `S2.EXE`'s map view pixel for pixel.
//!
//! [`Terrain`] draws a map's triangles, the borders between terrain types, roads and border
//! stones into an indexed surface through the polygon [`filler`], as the original's view pass
//! does. That direct path is the reference output. [`TileCache`] keeps the whole map as wrapping
//! indexed tiles for interactive views and redraws a tile only when something under it changes.
//! Palette effects (water and lava cycling) happen later, when indices resolve to colours, so
//! they never touch the cache. Nothing here uses the GPU.

mod draw;
pub mod filler;
mod fog;
mod geometry;
mod tables;
#[cfg(test)]
pub(crate) mod testing;
mod tiles;
mod units;

pub use draw::Terrain;
pub use filler::{Material, Surface, Vertex};
pub use fog::{Fog, Sight};
pub use geometry::{
    BACKGROUND, EXTRA_COLUMNS, EXTRA_ROWS, HEIGHT_STEP, MAX_ALTITUDE, NODE_WIDTH, OBJECT_EXTRA_COLUMNS,
    OBJECT_EXTRA_ROWS, ODD_ROW_SHIFT, ORIGIN_X, ROW_HEIGHT, ScreenNode, ScreenOffset, Walk, lift, world_size,
};
pub use tables::{
    BORDER_CLASSES, BORDER_STRIPS, BorderTable, MAX_TYPES, TERRAIN_SETS, TablesError, TerrainTables,
    TerrainType, TextureEntry, TypeInfo,
};
pub use tiles::{DEFAULT_BUDGET, TILE_MARGIN, TILE_SIZE, TileCache};
pub use units::{EdgeEntry, Fixed8, Fixed16, Packed, Point, Subpixel};
