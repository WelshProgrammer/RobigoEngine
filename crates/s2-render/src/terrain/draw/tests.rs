use s2_assets::Landscape;

use super::*;
use crate::terrain::testing::{count, flat, shading, terrain, texture, varied, view};
use s2_sim::NodeData;

use crate::terrain::{BORDER_CLASSES, Fog, ScreenNode, TERRAIN_SETS};

/// Ground texels are 7; the border strips at rows 176 + 16·c are 40 + c.
fn banded() -> Box<[u8; TEXTURE_SIZE]> {
    texture(|_, v| if v >= 176 { 40 + (v - 176) / 16 } else { 7 })
}

fn identity() -> ShadingTable {
    shading(|_, t| t)
}

#[test]
fn uniform_ground_covers_the_view() {
    let ground = terrain(flat(8, 8, 8), banded(), identity());
    for (x, y) in [(0, 0), (1000, 500), (-37, -91)] {
        let pixels = view(&ground, x, y, 200, 150);
        assert_eq!(count(&pixels, 7), pixels.len(), "at ({x}, {y})");
    }
}

#[test]
fn the_view_repeats_with_the_map() {
    let ground = terrain(varied(6, 4), banded(), shading(|level, t| level ^ t));
    let (period_x, period_y) = ground.world_size();
    let period_x = i32::try_from(period_x).unwrap();
    let period_y = i32::try_from(period_y).unwrap();
    let here = view(&ground, 40, 30, 500, 300);
    assert_eq!(here, view(&ground, 40 + period_x, 30 - period_y, 500, 300));
    assert_eq!(
        here,
        view(&ground, 40 - 3 * period_x, 30 + 2 * period_y, 500, 300)
    );
}

#[test]
fn a_view_across_the_map_edges_is_drawn_in_parts() {
    let ground = terrain(varied(6, 4), banded(), shading(|level, t| level ^ t));
    let (period_x, period_y) = ground.world_size();
    assert_eq!((period_x, period_y), (336, 112));
    let whole = view(&ground, 300, 100, 100, 40);
    for (left, x, width) in [(0, 300, 36), (36, 0, 64)] {
        for (top, y, height) in [(0, 100, 12), (12, 0, 28)] {
            let part = view(&ground, x, y, width, height);
            for row in 0..height {
                let from = ((top + row) * 100 + left) as usize;
                let to = (row * width) as usize;
                assert_eq!(
                    whole[from..from + width as usize],
                    part[to..to + width as usize],
                    "part at ({left}, {top}), row {row}"
                );
            }
        }
    }
}

#[test]
fn varied_ground_leaves_no_gaps() {
    let ground = terrain(varied(10, 10), banded(), identity());
    let pixels = view(&ground, 0, 0, 640, 480);
    assert_eq!(count(&pixels, BACKGROUND), 0);
}

/// Left half type 8 (class 0), right half type 11 (class 3).
fn split_map() -> NodeMap {
    let mut map = flat(8, 8, 8);
    for y in 0..8 {
        for x in 4..8 {
            let mut node = map.get(Node { x, y });
            node.terrain = [11; 2];
            map.set(Node { x, y }, node);
        }
    }
    map
}

fn split(landscape: Landscape) -> Vec<u8> {
    let ground = Terrain::new(split_map(), landscape, banded(), identity());
    view(&ground, 0, 0, 448, 224)
}

#[test]
fn borders_follow_the_landscapes_table() {
    let greenland = split(Landscape::Greenland);
    assert!(count(&greenland, 41) > 0, "class 0 takes type 11's strip 1");
    assert_eq!(count(&greenland, 43), 0, "class 3 refuses type 8's strip 3");
    let wasteland = split(Landscape::Wasteland);
    assert_eq!(
        count(&wasteland, 43 - 2),
        0,
        "class 0 refuses class 3 in wasteland"
    );
    assert!(count(&wasteland, 43) > 0, "class 3 takes type 8's strip 3");
}

#[test]
fn tables_can_be_swapped() {
    let original = TerrainTables::ORIGINAL;
    let no_borders = TerrainTables::new(
        original.types().to_vec(),
        original.textures().to_vec(),
        [[[false; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS],
    )
    .unwrap();
    let ground = Terrain::new(split_map(), Landscape::Greenland, banded(), identity())
        .with_tables(Arc::new(no_borders));
    let pixels = view(&ground, 0, 0, 448, 224);
    assert_eq!(count(&pixels, 7), pixels.len(), "only ground, no strips");
    assert!(
        count(&split(Landscape::Greenland), 41) > 0,
        "the original still has them"
    );
}

#[test]
fn equal_types_draw_no_border() {
    let ground = terrain(flat(8, 8, 11), banded(), identity());
    let pixels = view(&ground, 0, 0, 300, 200);
    assert_eq!(count(&pixels, 7), pixels.len());
}

#[test]
fn strip_index_zero_is_transparent() {
    let sheet = texture(|_, v| if v >= 176 { 0 } else { 7 });
    let mut map = flat(8, 8, 8);
    let mut node = map.get(Node { x: 3, y: 3 });
    node.terrain = [11; 2];
    map.set(Node { x: 3, y: 3 }, node);
    let pixels = view(&terrain(map, sheet, identity()), 0, 0, 448, 224);
    assert_eq!(count(&pixels, 7), pixels.len());
}

#[test]
fn shading_follows_the_light_layer_but_not_on_water() {
    let mut map = flat(8, 8, 8);
    for y in 0..8 {
        for x in 0..8 {
            let mut node = map.get(Node { x, y });
            node.light = 100;
            map.set(Node { x, y }, node);
        }
    }
    let rows = shading(|level, _| level);
    let ground = terrain(map.clone(), banded(), rows.clone());
    let pixels = view(&ground, 0, 0, 300, 200);
    assert_eq!(count(&pixels, 100), pixels.len());

    for y in 0..8 {
        for x in 0..8 {
            let mut node = map.get(Node { x, y });
            node.terrain = [5; 2];
            map.set(Node { x, y }, node);
        }
    }
    let water = terrain(map, banded(), rows);
    let pixels = view(&water, 0, 0, 300, 200);
    assert_eq!(count(&pixels, 64), pixels.len(), "water is drawn at row 64");
}

#[test]
fn fog_hides_and_dims() {
    let mut ground = terrain(flat(32, 32, 8), banded(), shading(|level, _| level));
    ground.set_fog(Some(Fog::new(32, 32)));
    let pixels = view(&ground, 0, 0, 600, 400);
    assert_eq!(count(&pixels, BACKGROUND), pixels.len(), "nothing seen yet");

    let centre = Node { x: 6, y: 8 };
    ground.fog_mut().unwrap().reveal(centre, 2);
    assert_eq!(ground.sight(centre), [Sight::Visible; 2]);
    let pixels = view(&ground, 0, 0, 600, 400);
    let (lit, dim, dark) = (count(&pixels, 64), count(&pixels, 16), count(&pixels, BACKGROUND));
    assert!(
        lit > 0 && dim > 0 && dark > 0,
        "{lit} lit, {dim} dimmed, {dark} dark"
    );
    assert_eq!(lit + dim + dark, pixels.len());
    let at = |x: usize, y: usize| pixels[y * 600 + x];
    assert_eq!(at(6 * 56 + 28, 8 * 28 + 5), 64, "below the centre node");
    assert_eq!(at(590, 390), BACKGROUND);

    ground.set_fog(None);
    let pixels = view(&ground, 0, 0, 600, 400);
    assert_eq!(count(&pixels, 64), pixels.len());
}

#[test]
fn unknown_types_leave_the_background() {
    let ground = terrain(flat(8, 8, 32), banded(), identity());
    let pixels = view(&ground, 0, 0, 300, 200);
    assert_eq!(count(&pixels, BACKGROUND), pixels.len());
}

#[test]
fn raised_ground_moves_up_the_screen() {
    let mut map = flat(8, 8, 8);
    let sheet = texture(|_, v| if v >= 176 { 40 } else { v });
    let flat_view = view(&terrain(map.clone(), sheet.clone(), identity()), 0, 0, 448, 224);
    let mut node = map.get(Node { x: 3, y: 4 });
    node.altitude = 10;
    map.set(Node { x: 3, y: 4 }, node);
    let raised = view(&terrain(map, sheet, identity()), 0, 0, 448, 224);
    let changed: Vec<usize> = (0..raised.len()).filter(|&i| raised[i] != flat_view[i]).collect();
    let rows = changed.iter().map(|i| i / 448);
    let (top, bottom) = (rows.clone().min().unwrap(), rows.max().unwrap());
    let node_y = 4 * 28;
    assert!(top < node_y - 40 && bottom <= node_y + 28, "{top}..{bottom}");
}

#[test]
fn map_data_wraps_and_reads_back() {
    let mut map = flat(4, 3, 1);
    let data = NodeData {
        altitude: 9,
        terrain: [2, 3],
        light: 70,
        ..NodeData::default()
    };
    map.set(Node { x: 1, y: 2 }, data);
    assert_eq!(map.get(Node { x: 1, y: 2 }), data);
    assert_eq!(map.get(Node { x: 5, y: 5 }), data, "wraps");
    assert!(NodeMap::filled(0, 3, data).is_none());
}

#[test]
fn a_node_is_picked_where_it_is_drawn_heights_and_wrapping_included() {
    let mut map = varied(16, 12);
    let high = s2_sim::Node { x: 5, y: 7 };
    let mut data = map.get(high);
    data.altitude = 60;
    map.set(high, data);
    let terrain = terrain(map, banded(), identity());
    for y in 0..12 {
        for x in 0..16 {
            let node = s2_sim::Node { x, y };
            let at = node.world_position(terrain.map().get(node).altitude);
            assert_eq!(terrain.node_at(at), node, "{node:?}");
        }
    }
    let (period_x, period_y) = terrain.world_size();
    let corner =
        s2_sim::Node { x: 0, y: 0 }.world_position(terrain.map().get(s2_sim::Node::default()).altitude);
    let wrapped = Point::new(
        corner.x - period_x.cast_signed(),
        corner.y + period_y.cast_signed(),
    );
    assert_eq!(terrain.node_at(wrapped), s2_sim::Node::default());
}
