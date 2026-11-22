//! The whole map's terrain cached as indexed tiles, for interactive views.

use rayon::prelude::*;

use s2_sim::NodeRect;

use super::draw::Terrain;
use super::filler::Surface;
use super::geometry::{HEIGHT_STEP, MAX_ALTITUDE, NODE_WIDTH, ROW_HEIGHT};
use crate::Layers;

/// Side of a tile in world pixels. Tiles in the last column and row are cut to the map's size.
pub const TILE_SIZE: u32 = 256;

/// Pixels drawn beyond each edge of a tile and thrown away, so that only triangles larger than
/// this are cut where the tile ends.
pub const TILE_MARGIN: u32 = 64;

/// Tiles kept by default before the least recently used are dropped: 64 MiB.
pub const DEFAULT_BUDGET: usize = 1024;

#[derive(Debug)]
struct Tile {
    pixels: Vec<u8>,
    last_used: u64,
}

/// The map's terrain as tiles of [`TILE_SIZE`] world pixels that wrap with the map, each drawn
/// on first use and kept until something under it changes.
///
/// [`Terrain::draw`] draws each tile as the middle of a view [`TILE_MARGIN`] pixels larger on
/// every side, so a tile clips triangles in other places than a direct view does. The direct view
/// cuts triangles at its own edges; a tile cuts only those reaching past its margin (tall
/// slopes). Clipping moves texture coordinates by up to a texel, which on the noisy terrain
/// sheets changes most pixels of the triangles concerned. The original's own terrain cache
/// redraws strips as the view scrolls and has the same kind of seams. For output that must match
/// the original's full redraw pixel for pixel, draw the view directly with [`Terrain::draw`].
#[derive(Debug)]
pub struct TileCache {
    period: (u32, u32),
    columns: u32,
    tiles: Vec<Option<Tile>>,
    clock: u64,
    budget: usize,
    painted: u64,
    layers: Layers,
}

/// A run of pixels along one axis: `len` pixels of tile `tile` from offset `from`, placed at
/// `at` in the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    tile: u32,
    from: u32,
    at: u32,
    len: u32,
}

/// Splits `len` pixels from `start` (inside `0..period`) into runs within single tiles,
/// wrapping at `period`.
fn runs(start: u32, len: u32, period: u32) -> Vec<Run> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut pos = start;
    while at < len && period > 0 {
        let tile = pos / TILE_SIZE;
        let from = pos % TILE_SIZE;
        let tile_len = TILE_SIZE.min(period - tile * TILE_SIZE);
        let run = (tile_len - from).min(len - at);
        out.push(Run {
            tile,
            from,
            at,
            len: run,
        });
        at += run;
        pos = (pos + run) % period;
    }
    out
}

fn wrap(value: i64, period: u32) -> u32 {
    u32::try_from(value.rem_euclid(i64::from(period.max(1)))).unwrap_or_default()
}

impl TileCache {
    /// An empty cache for `terrain`'s map, keeping up to [`DEFAULT_BUDGET`] tiles.
    #[must_use]
    pub fn new(terrain: &Terrain) -> Self {
        Self::with_budget(terrain, DEFAULT_BUDGET)
    }

    /// An empty cache keeping up to `budget` tiles, beyond those the latest view needs.
    #[must_use]
    pub fn with_budget(terrain: &Terrain, budget: usize) -> Self {
        let period = terrain.world_size();
        let columns = period.0.div_ceil(TILE_SIZE);
        let rows = period.1.div_ceil(TILE_SIZE);
        Self {
            period,
            columns,
            tiles: std::iter::repeat_with(|| None)
                .take(columns as usize * rows as usize)
                .collect(),
            clock: 0,
            budget,
            painted: 0,
            layers: terrain.layers() & Layers::TERRAIN,
        }
    }

    /// Tiles currently held.
    #[must_use]
    pub fn cached(&self) -> usize {
        self.tiles.iter().flatten().count()
    }

    /// Tiles drawn since the cache was made.
    #[must_use]
    pub fn painted(&self) -> u64 {
        self.painted
    }

    /// Fills `target` with the map seen from world pixel (`x`, `y`), drawing missing tiles in
    /// parallel. Tiles drawn with terrain [`Layers`] other than those `terrain` now has are
    /// dropped before that.
    pub fn compose(&mut self, terrain: &Terrain, x: i32, y: i32, target: &mut Surface<'_>) {
        let layers = terrain.layers() & Layers::TERRAIN;
        if layers != self.layers {
            self.invalidate_all();
            self.layers = layers;
        }
        self.clock += 1;
        let across = runs(wrap(x.into(), self.period.0), target.width(), self.period.0);
        let down = runs(wrap(y.into(), self.period.1), target.height(), self.period.1);
        let columns = self.columns as usize;
        let mut needed: Vec<usize> = down
            .iter()
            .flat_map(|r| {
                across
                    .iter()
                    .map(move |c| r.tile as usize * columns + c.tile as usize)
            })
            .collect();
        needed.sort_unstable();
        needed.dedup();

        let missing: Vec<usize> = needed
            .iter()
            .copied()
            .filter(|&i| self.tiles[i].is_none())
            .collect();
        let drawn: Vec<(usize, Vec<u8>)> = missing.par_iter().map(|&i| (i, self.paint(terrain, i))).collect();
        self.painted += drawn.len() as u64;
        for (i, pixels) in drawn {
            self.tiles[i] = Some(Tile { pixels, last_used: 0 });
        }
        for &i in &needed {
            if let Some(tile) = &mut self.tiles[i] {
                tile.last_used = self.clock;
            }
        }

        for row in &down {
            for column in &across {
                let index = self.index(column.tile, row.tile);
                let (tile_width, _) = self.extent(index);
                let Some(tile) = &self.tiles[index] else {
                    continue;
                };
                for line in 0..row.len {
                    let src = ((row.from + line) * tile_width + column.from) as usize;
                    let Some(dst) = target.row_mut(row.at + line) else {
                        continue;
                    };
                    let at = column.at as usize;
                    let len = column.len as usize;
                    dst[at..at + len].copy_from_slice(&tile.pixels[src..src + len]);
                }
            }
        }
        self.evict();
    }

    /// Drops every tile that something at the nodes in `nodes` can reach: their triangles, their
    /// neighbours' triangles and borders, and anything lifted over them by high ground.
    pub fn invalidate_nodes(&mut self, nodes: NodeRect) {
        let margin = 2;
        let left = i64::from(nodes.x - margin) * i64::from(NODE_WIDTH);
        let right = i64::from(nodes.x + margin + 1) * i64::from(NODE_WIDTH)
            + i64::from(nodes.width) * i64::from(NODE_WIDTH)
            + i64::from(NODE_WIDTH);
        let rise = i64::from(HEIGHT_STEP) * i64::from(MAX_ALTITUDE) + 2;
        let top = i64::from(nodes.y - margin) * i64::from(ROW_HEIGHT) - rise;
        let bottom = i64::from(nodes.y + margin + 1) * i64::from(ROW_HEIGHT)
            + i64::from(nodes.height) * i64::from(ROW_HEIGHT)
            + 2;
        let span = |lo: i64, hi: i64, period: u32| {
            let len = u32::try_from(hi - lo).unwrap_or(u32::MAX).min(period);
            runs(wrap(lo, period), len, period)
        };
        for row in span(top, bottom, self.period.1) {
            for column in span(left, right, self.period.0) {
                let index = self.index(column.tile, row.tile);
                self.tiles[index] = None;
            }
        }
    }

    /// Drops every tile.
    pub fn invalidate_all(&mut self) {
        self.tiles.iter_mut().for_each(|tile| *tile = None);
    }

    fn index(&self, column: u32, row: u32) -> usize {
        row as usize * self.columns as usize + column as usize
    }

    fn extent(&self, index: usize) -> (u32, u32) {
        let column = u32::try_from(index % self.columns as usize).unwrap_or_default();
        let row = u32::try_from(index / self.columns as usize).unwrap_or_default();
        (
            TILE_SIZE.min(self.period.0 - column * TILE_SIZE),
            TILE_SIZE.min(self.period.1 - row * TILE_SIZE),
        )
    }

    /// Draws tile `index` as the middle of a view [`TILE_MARGIN`] larger on every side, so
    /// triangles crossing the tile's edges are drawn whole.
    fn paint(&self, terrain: &Terrain, index: usize) -> Vec<u8> {
        let (width, height) = self.extent(index);
        let column = u32::try_from(index % self.columns as usize).unwrap_or_default();
        let row = u32::try_from(index / self.columns as usize).unwrap_or_default();
        let (outer_width, outer_height) = (width + 2 * TILE_MARGIN, height + 2 * TILE_MARGIN);
        let mut outer = vec![0; outer_width as usize * outer_height as usize];
        if let Some(mut surface) = Surface::new(&mut outer, outer_width, outer_height) {
            let origin = |tile: u32| i64::from(tile * TILE_SIZE) - i64::from(TILE_MARGIN);
            let x = i32::try_from(origin(column)).unwrap_or_default();
            let y = i32::try_from(origin(row)).unwrap_or_default();
            terrain.draw(x, y, &mut surface);
        }
        let margin = TILE_MARGIN as usize;
        outer
            .chunks_exact(outer_width as usize)
            .skip(margin)
            .take(height as usize)
            .flat_map(|line| &line[margin..margin + width as usize])
            .copied()
            .collect()
    }

    fn evict(&mut self) {
        let mut held: Vec<(u64, usize)> = self
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(i, tile)| tile.as_ref().map(|t| (t.last_used, i)))
            .filter(|&(used, _)| used != self.clock)
            .collect();
        let in_use = self.cached() - held.len();
        let excess = (in_use + held.len()).saturating_sub(self.budget.max(in_use));
        held.sort_unstable();
        for &(_, i) in held.iter().take(excess) {
            self.tiles[i] = None;
        }
    }
}

#[cfg(test)]
mod tests;
