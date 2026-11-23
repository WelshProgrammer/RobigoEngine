//! Draws whole starting views of real maps, terrain and objects, from a player's copy of the
//! game. Each test runs only when its variable is set, and passes with a note otherwise:
//!
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only hashes and counts of the indexed output are asserted; no game data lives here.

mod common;

use common::{data, start};

use s2_render::terrain::ScreenNode;
use s2_render::{Clocks, Frame, Layers, MapView, Provenance};
use s2_sim::{Game, Node};

/// The main map view's size on the 640 × 480 screen.
const VIEW: (u32, u32) = (616, 456);

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn start_view(view: &MapView, game: &Game, hq: Node) -> (Frame, Provenance) {
    let height = view.terrain().map().get(hq).altitude;
    let scroll = hq.start_scroll(height, VIEW.0, VIEW.1);
    let mut frame = Frame::new(VIEW.0, VIEW.1, 0);
    let mut provenance = Provenance::new(VIEW.0, VIEW.1);
    let clocks = Clocks {
        anim: 0,
        step: game.clock().step(),
    };
    view.draw(game.world(), scroll, clocks, &mut frame, Some(&mut provenance));
    (frame, provenance)
}

/// Player 0's starting view of one map per landscape, plus two with animals: the hash of the
/// indexed frame, the animals created, and the pixels the figure mask covers. Pinned from this
/// implementation; `dosbox.rs` compares starting views with the original.
#[test]
fn gold_starting_views() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    let expected: [(&str, u64, usize, usize); 5] = [
        ("DATA/MAPS3/OMAP00.WLD", 9_901_721_657_929_297_341, 0, 0),
        ("DATA/MAPS/MISS204.WLD", 12_022_401_893_897_724_744, 0, 0),
        ("DATA/MAPS2/GREEN.WLD", 14_159_346_000_279_477_946, 134, 268),
        ("DATA/MAPS4/MAP07.WLD", 12_751_117_111_506_784_313, 175, 260),
        ("DATA/MAPS4/MAP02.WLD", 14_056_295_000_709_714_059, 18, 150),
    ];
    let found: Vec<(&str, u64, usize, usize)> = expected
        .iter()
        .map(|&(path, ..)| {
            let (view, game, hq) = start(&data, path, false);
            let (frame, provenance) = start_view(&view, &game, hq);
            let figures = provenance.mask(Layers::FIGURES).iter().filter(|&&m| m).count();
            (path, fnv(frame.pixels()), game.world().figures().count(), figures)
        })
        .collect();
    assert_eq!(found, expected);
}

/// With every layer but one, the picture differs from the full one only where that layer drew.
#[test]
fn gold_layers_only_change_their_own_pixels() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    let (mut view, game, hq) = start(&data, "DATA/MAPS4/MAP07.WLD", false);
    let (all, provenance) = start_view(&view, &game, hq);
    for (name, layer) in Layers::named().filter(|&(_, l)| l != Layers::FOG) {
        view.set_layers(Layers::ALL - layer);
        let (without, _) = start_view(&view, &game, hq);
        let stray = (0..VIEW.1)
            .flat_map(|y| (0..VIEW.0).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let i = (y * VIEW.0 + x) as usize;
                all.pixels()[i] != without.pixels()[i] && !provenance.at(x, y).contains(layer)
            })
            .count();
        assert_eq!(stray, 0, "hiding {name}");
    }
}

/// Every map the data set ships, in every map folder: a game starts on it and player 0's starting
/// view is drawn with the headquarters on screen. Returns the maps whose view has pixels no layer
/// wrote, with their counts, or `None` when `var` is unset.
fn every_map_draws(var: &str) -> Option<Vec<(String, usize)>> {
    let data = data(var)?;
    let mut drawn = 0;
    let mut gaps = Vec::new();
    for folder in data.list_dir("DATA").unwrap() {
        if !folder.name.to_ascii_uppercase().starts_with("MAPS") {
            continue;
        }
        for file in data.list_dir(&format!("DATA/{}", folder.name)).unwrap() {
            let map = std::path::Path::new(&file.name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wld") || e.eq_ignore_ascii_case("swd"));
            if !map {
                continue;
            }
            let path = format!("DATA/{}/{}", folder.name, file.name);
            let (view, game, hq) = start(&data, &path, false);
            let (_, provenance) = start_view(&view, &game, hq);
            let buildings = provenance.mask(Layers::BUILDINGS).iter().filter(|&&m| m).count();
            assert!(buildings > 0, "{path}: no headquarters in the starting view");
            let empty = provenance.mask(Layers::ALL).iter().filter(|&&m| !m).count();
            if empty > 0 {
                gaps.push((path, empty));
            }
            drawn += 1;
        }
    }
    eprintln!("{var}: {drawn} maps drawn, gaps {gaps:?}");
    assert!(drawn > 0, "{var}: no maps found");
    Some(gaps)
}

/// The starting views with pixels no triangle covers. S2.EXE's terrain pass leaves the same
/// pixels at the background colour.
const GAPS: [(&str, usize); 7] = [
    ("DATA/MAPS/MISS200.WLD", 1),
    ("DATA/MAPS3/OMAP06.WLD", 2),
    ("DATA/MAPS3/OMAP08.WLD", 1),
    ("DATA/MAPS3/OMAP09.WLD", 1),
    ("DATA/MAPS3/OMAP11.WLD", 1),
    ("DATA/MAPS3/OMAP15.WLD", 1),
    ("DATA/MAPS3/OMAP16.WLD", 1),
];

fn assert_gaps(found: Option<Vec<(String, usize)>>) {
    if let Some(found) = found {
        let found: Vec<(&str, usize)> = found.iter().map(|(p, n)| (p.as_str(), *n)).collect();
        assert_eq!(found, GAPS);
    }
}

#[test]
fn gold_every_map_draws() {
    assert_gaps(every_map_draws("S2_DATA_GOLD"));
}

#[test]
fn plain_every_map_draws() {
    assert_gaps(every_map_draws("S2_DATA_PLAIN"));
}
