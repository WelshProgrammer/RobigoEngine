use super::*;
use crate::terrain::testing::{count, flat, shading, terrain, texture, varied, view};
use s2_sim::{Node, NodeData};

use crate::terrain::BACKGROUND;

/// The colour the composed surface starts out with.
const UNTOUCHED: u8 = 238;

fn compose(cache: &mut TileCache, ground: &Terrain, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
    let mut pixels = vec![UNTOUCHED; (w * h) as usize];
    let mut surface = Surface::new(&mut pixels, w, h).unwrap();
    cache.compose(ground, x, y, &mut surface);
    pixels
}

fn rich() -> Terrain {
    terrain(varied(16, 16), texture(|u, v| u ^ v), shading(u8::wrapping_add))
}

#[test]
fn runs_split_at_tiles_and_wrap() {
    let run = |tile, from, at, len| Run { tile, from, at, len };
    assert_eq!(runs(250, 20, 600), [run(0, 250, 0, 6), run(1, 0, 6, 14)]);
    assert_eq!(
        runs(500, 200, 600),
        [run(1, 244, 0, 12), run(2, 0, 12, 88), run(0, 0, 100, 100)]
    );
    let found = runs(0, 0, 600);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_tile_is_the_middle_of_a_larger_direct_view() {
    let ground = rich();
    let mut cache = TileCache::new(&ground);
    let tiled = compose(&mut cache, &ground, 256, 256, TILE_SIZE, 192);
    let margin = TILE_MARGIN as usize;
    let outer_width = TILE_SIZE + 2 * TILE_MARGIN;
    let start = i32::try_from(256 - TILE_MARGIN).unwrap();
    let outer = view(&ground, start, start, outer_width, 192 + 2 * TILE_MARGIN);
    let middle: Vec<u8> = outer
        .chunks_exact(outer_width as usize)
        .skip(margin)
        .take(192)
        .flat_map(|line| &line[margin..margin + TILE_SIZE as usize])
        .copied()
        .collect();
    assert_eq!(tiled, middle);
}

#[test]
fn tiles_agree_with_a_direct_view_away_from_its_edges() {
    let ground = rich();
    let mut cache = TileCache::new(&ground);
    let (width, height) = (700, 400);
    let tiled = compose(&mut cache, &ground, 100, 50, width, height);
    let direct = view(&ground, 100, 50, width, height);
    let margin = TILE_MARGIN as usize;
    for y in margin..height as usize - margin {
        for x in margin..width as usize - margin {
            let i = y * width as usize + x;
            assert_eq!(tiled[i], direct[i], "({x}, {y})");
        }
    }
}

#[test]
fn views_across_tiles_and_the_map_edge_are_complete() {
    let ground = terrain(varied(16, 16), texture(|_, _| 7), shading(|_, t| t));
    let mut cache = TileCache::new(&ground);
    for (x, y) in [(0, 0), (700, 300), (-50, -20), (890, 440)] {
        let pixels = compose(&mut cache, &ground, x, y, 640, 480);
        assert_eq!(count(&pixels, 7), pixels.len(), "at ({x}, {y})");
    }
}

#[test]
fn views_one_period_apart_match() {
    let ground = rich();
    let mut cache = TileCache::new(&ground);
    let (px, py) = ground.world_size();
    let (px, py) = (i32::try_from(px).unwrap(), i32::try_from(py).unwrap());
    let here = compose(&mut cache, &ground, 100, 50, 900, 500);
    assert_eq!(here, compose(&mut cache, &ground, 100 + px, 50 - py, 900, 500));
    assert_ne!(count(&here, BACKGROUND), here.len());
}

#[test]
fn tiles_are_drawn_once_until_invalidated() {
    let mut ground = rich();
    let mut cache = TileCache::new(&ground);
    compose(&mut cache, &ground, 0, 0, 896, 448);
    assert_eq!(cache.painted(), 8, "16 x 16 nodes make 4 x 2 tiles");
    compose(&mut cache, &ground, 10, 10, 896, 448);
    assert_eq!(cache.painted(), 8);

    let node = Node { x: 1, y: 1 };
    let data = ground.map().get(node);
    ground.map_mut().set(
        node,
        NodeData {
            altitude: data.altitude + 20,
            ..data
        },
    );
    cache.invalidate_nodes(NodeRect {
        x: 1,
        y: 1,
        width: 1,
        height: 1,
    });
    let after = compose(&mut cache, &ground, 0, 0, 896, 448);
    let repainted = cache.painted() - 8;
    assert!((1..8).contains(&repainted), "{repainted} tiles redrawn");
    let mut fresh = TileCache::new(&ground);
    assert_eq!(after, compose(&mut fresh, &ground, 0, 0, 896, 448));
}

#[test]
fn invalidation_wraps_round_the_map() {
    let ground = rich();
    let mut cache = TileCache::new(&ground);
    compose(&mut cache, &ground, 0, 0, 896, 448);
    cache.invalidate_nodes(NodeRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    });
    assert!(
        cache.tiles[cache.index(3, 1)].is_none(),
        "the far corner is a neighbour"
    );
    cache.invalidate_all();
    assert_eq!(cache.cached(), 0);
}

#[test]
fn the_least_recently_used_tiles_go_first() {
    let ground = terrain(flat(32, 32, 8), texture(|_, _| 7), shading(|_, t| t));
    let mut cache = TileCache::with_budget(&ground, 2);
    compose(&mut cache, &ground, 0, 0, 300, 100);
    assert_eq!(cache.cached(), 2);
    compose(&mut cache, &ground, 1024, 0, 100, 100);
    assert_eq!(cache.cached(), 2, "one old tile dropped for the new one");
    assert!(cache.tiles[cache.index(4, 0)].is_some());
    assert!(cache.tiles[cache.index(0, 0)].is_none());
    compose(&mut cache, &ground, 0, 0, 600, 100);
    assert_eq!(cache.cached(), 3, "a view keeps every tile it needs");
}
