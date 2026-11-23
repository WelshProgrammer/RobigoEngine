//! The picture S2 shows while a game's map loads (`0x232ae` in the Gold Edition's executable):
//! the continent's own picture in the World Campaign, and otherwise one picked by the map's
//! landscape. A modern machine loads a map at once, so Robigo holds the picture for [`HOLD`]
//! unless the player passes it, and the setting `loading_pictures` leaves it out.

use std::time::Duration;

use s2_formats::map::{Map, Terrain};
use s2_mods::Sheets;
use s2_sim::Objective;
use s2_sim::replay::MapSource;
use s2_sim::save::Save;
use s2_ui::screens::world::Continents;

use crate::scene::Start;

/// How long the picture shows unless the player passes it.
pub(crate) const HOLD: Duration = Duration::from_secs(5);

/// The pictures by landscape (`0x1f99e0`): greenland picks one of five and winter one of two
/// at random, the wasteland always has the same one.
const GREENLAND: [u16; 5] = [801, 802, 803, 805, 806];
const WASTELAND: u16 = 804;
const WINTER: [u16; 2] = [810, 811];

/// The data path of the picture a game `start` names shows while it loads, `None` when nothing
/// says which. `random` picks among a landscape's pictures.
pub(crate) fn picture(start: &Start, sheets: &Sheets<'_>, random: usize) -> Option<String> {
    let continents = Continents::original();
    let mission_picture = |number: u8| {
        continents
            .get(number)
            .map(|continent| format!("GFX/PICS/MISSION/{}", continent.picture))
    };
    if let Some(number) = start.continent {
        return mission_picture(number);
    }
    let bytes = if let Some(path) = &start.load {
        let save = Save::from_bytes(&std::fs::read(path).ok()?).ok()?;
        match &save.replay.map {
            MapSource::Data(map) => {
                if save.replay.setup.objective == Objective::Headquarters
                    && let Some(number) = continents.by_map(map)
                {
                    return mission_picture(number);
                }
                map_bytes(sheets, map)?
            }
            MapSource::File(_) => save.map?,
        }
    } else {
        let map = start
            .map
            .clone()
            .or_else(|| start.mission.map(s2_sim::map_path))?;
        map_bytes(sheets, &map)?
    };
    let terrain = Map::parse_wld(&bytes)
        .ok()
        .and_then(|map| map.header().map(s2_formats::map::Header::terrain))
        .unwrap_or(Terrain::Greenland);
    let number = match terrain {
        Terrain::Wasteland => WASTELAND,
        Terrain::Winter => WINTER[random % WINTER.len()],
        _ => GREENLAND[random % GREENLAND.len()],
    };
    Some(format!("GFX/PICS/SETUP{number:03}.LBM"))
}

/// The bytes of the map at `path`: in the data or the mods, or a file on disk.
fn map_bytes(sheets: &Sheets<'_>, path: &str) -> Option<Vec<u8>> {
    if sheets.has_map_file(path) {
        sheets.map_file(path).ok()
    } else {
        std::fs::read(path).ok()
    }
}
