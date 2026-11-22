//! Draws the starting view of real maps from a player's copy of the game. Each test runs only
//! when its variable is set, and passes with a note otherwise:
//!
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only hashes and counts of the indexed output are asserted; no game data lives here.

use std::path::PathBuf;

use s2_assets::{Landscape, LandscapeAssets};
use s2_data::{DataSet, EntryKind};
use s2_formats::map::Map;
use s2_render::Frame;
use s2_render::terrain::{Fog, ScreenNode, TILE_MARGIN, Terrain, TileCache};
use s2_sim::{Node, NodeMap};

/// The main map view's size on the 640 × 480 screen.
const VIEW: (u32, u32) = (616, 456);

fn data(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

struct Start {
    terrain: Terrain,
    hq: Node,
}

fn start(data: &DataSet, path: &str) -> Start {
    let map = Map::parse_wld(&data.read(path).unwrap()).unwrap();
    let header = map.header().unwrap();
    let landscape = Landscape::from_terrain(header.terrain()).unwrap();
    let assets = LandscapeAssets::load(data, landscape).unwrap();
    let hq = header.headquarters()[0].unwrap();
    Start {
        terrain: Terrain::with_assets(NodeMap::from_map(&map).unwrap(), &assets),
        hq: Node { x: hq.x, y: hq.y },
    }
}

fn start_view(start: &Start) -> Frame {
    let height = start.terrain.map().get(start.hq).altitude;
    let scroll = start.hq.start_scroll(height, VIEW.0, VIEW.1);
    let mut frame = Frame::new(VIEW.0, VIEW.1, 0);
    start.terrain.draw(scroll.x, scroll.y, &mut frame.surface());
    frame
}

/// Hashes of player 0's starting view without fog, one map per landscape. Pinned from this
/// implementation; comparing with the original running in `DOSBox` is a separate step.
#[test]
fn gold_starting_views() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    let expected = [
        ("DATA/MAPS3/OMAP00.WLD", 4_464_188_669_506_933_108),
        ("DATA/MAPS/MISS204.WLD", 8_070_810_733_418_131_704),
        ("DATA/MAPS2/GREEN.WLD", 7_000_233_591_092_063_573),
    ];
    let found: Vec<(&str, u64)> = expected
        .iter()
        .map(|&(path, _)| (path, fnv(start_view(&start(&data, path)).pixels())))
        .collect();
    assert_eq!(found, expected);
}

/// With the fog as at game start, a headquarters' sight covers the whole starting view.
#[test]
fn gold_starting_fog_covers_the_view() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    let mut start = start(&data, "DATA/MAPS3/OMAP00.WLD");
    let clear = start_view(&start);
    let mut fog = Fog::new(start.terrain.map().width(), start.terrain.map().height());
    fog.reveal(start.hq, 12);
    start.terrain.set_fog(Some(fog));
    assert_eq!(start_view(&start), clear);
}

/// Every Gold map's starting view draws. Away from the view's edges, where the direct view clips
/// triangles and the tiles do not, the tiled path draws exactly the same pixels.
#[test]
fn gold_tiles_match_direct_views() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    let mut maps = 0;
    let mut worst = (0, String::new());
    for folder in ["DATA/MAPS", "DATA/MAPS2", "DATA/MAPS3", "DATA/MAPS4"] {
        for entry in data.list_dir(folder).unwrap() {
            if entry.kind != EntryKind::File || !entry.name.to_ascii_uppercase().ends_with(".WLD") {
                continue;
            }
            let start = start(&data, &format!("{folder}/{}", entry.name));
            let direct = start_view(&start);
            let height = start.terrain.map().get(start.hq).altitude;
            let scroll = start.hq.start_scroll(height, VIEW.0, VIEW.1);
            let mut tiled = Frame::new(VIEW.0, VIEW.1, 0);
            let mut cache = TileCache::new(&start.terrain);
            cache.compose(&start.terrain, scroll.x, scroll.y, &mut tiled.surface());
            let margin = TILE_MARGIN as usize;
            let (width, height) = (VIEW.0 as usize, VIEW.1 as usize);
            let differ = (margin..height - margin)
                .flat_map(|y| (margin..width - margin).map(move |x| y * width + x))
                .filter(|&i| direct.pixels()[i] != tiled.pixels()[i])
                .count();
            if differ > worst.0 {
                worst = (differ, entry.name.clone());
            }
            maps += 1;
        }
    }
    assert_eq!(maps, 49);
    assert_eq!(worst.0, 0, "pixels differ inside {}", worst.1);
}
