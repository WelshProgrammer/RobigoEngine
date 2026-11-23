//! Starting views compared with screenshots of the original game, palette index by palette index.
//!
//! Runs when `S2_DATA_GOLD` names the GOG Gold Edition folder and `S2_CAPTURES` a folder of raw
//! `DOSBox` screenshots with a `captures.txt` in it. Each line of that file names a map and the
//! screenshot of its starting view, such as `DATA/MAPS4/MAP01.WLD ring.png`; `#` starts a comment.
//! The screenshots are raw 8-bit indexed PNGs from `DOSBox` or DOSBox-X, taken in Unlimited Play
//! with default settings, the mouse in the top-left corner, within a second of the map appearing.
//! The pixels that all captures of one size share are the game's interface and are left out, so
//! every size needs at least two captures of different maps.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::{data, start};
use s2_render::capture::{self, Capture, PHASES};
use s2_render::terrain::ScreenNode;
use s2_render::{Clocks, Frame, Layers, Provenance};

struct Entry {
    map: String,
    file: String,
    capture: Capture,
}

fn captures() -> Option<Vec<Entry>> {
    let Some(dir) = std::env::var_os("S2_CAPTURES").map(PathBuf::from) else {
        eprintln!("skipped: set S2_CAPTURES to run this test");
        return None;
    };
    let list = std::fs::read_to_string(dir.join("captures.txt")).unwrap();
    let entries = list
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (map, file) = line.split_once(char::is_whitespace).unwrap();
            let bytes = std::fs::read(dir.join(file.trim())).unwrap();
            Entry {
                map: map.to_owned(),
                file: file.trim().to_owned(),
                capture: Capture::from_png(&bytes).unwrap(),
            }
        })
        .collect();
    Some(entries)
}

/// Every listed starting view matches its capture exactly at one animation phase, apart from the
/// interface and the pixels Robigo drew animals on.
#[test]
fn starting_views_match_the_original() {
    let (Some(data), Some(entries)) = (data("S2_DATA_GOLD"), captures()) else {
        return;
    };
    let mut by_size: BTreeMap<(u32, u32), Vec<Vec<u8>>> = BTreeMap::new();
    for entry in &entries {
        let size = (entry.capture.width(), entry.capture.height());
        by_size.entry(size).or_default().push(entry.capture.view());
    }
    let mut failures = Vec::new();
    for entry in &entries {
        let size = (entry.capture.width(), entry.capture.height());
        let views: Vec<&[u8]> = by_size[&size].iter().map(Vec::as_slice).collect();
        let interface = capture::interface_mask(&views)
            .unwrap_or_else(|| panic!("{}: no other capture at {size:?}", entry.file));
        let rect = entry.capture.view_rect().unwrap();
        let theirs = entry.capture.view();
        let (view, game, hq) = start(&data, &entry.map, true);
        let height = view.terrain().map().get(hq).altitude;
        let scroll = hq.start_scroll(height, rect.width, rect.height);
        let best = (0..PHASES)
            .map(|phase| {
                let mut frame = Frame::new(rect.width, rect.height, 0);
                let mut provenance = Provenance::new(rect.width, rect.height);
                let clocks = Clocks {
                    anim: phase,
                    step: game.clock().step(),
                };
                view.draw(game.world(), scroll, clocks, &mut frame, Some(&mut provenance));
                let skip: Vec<bool> = interface
                    .iter()
                    .zip(provenance.mask(Layers::FIGURES))
                    .map(|(&ui, figure)| ui || figure)
                    .collect();
                let result = capture::compare(frame.pixels(), &theirs, rect.width, &skip).unwrap();
                (phase, result)
            })
            .min_by_key(|(_, result)| result.differing)
            .unwrap();
        let (phase, result) = best;
        eprintln!(
            "{} ({}): phase {phase}, {} of {} pixels differ",
            entry.map, entry.file, result.differing, result.compared
        );
        if result.differing > 0 {
            failures.push(format!(
                "{} ({}): {} pixels differ at phase {phase}, first group {:?}",
                entry.map, entry.file, result.differing, result.regions[0]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
