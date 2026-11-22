//! What the Gold Edition adds to the base game, which bakes into the `gold` mod instead of the
//! base mod (`docs/design/mods.md`, "The Gold Edition's extras").
//!
//! The lists name data paths only. They come from comparing the original CD and the official
//! patch with a Gold Edition install: every file and interface item here is in the Gold Edition
//! and in neither of the others.

/// The Gold Edition's mod: its id and its folder's name under the mods folder.
pub const GOLD: &str = "gold";

/// The name the Gold Edition's mod gives itself, which the player sees.
pub const GOLD_NAME: &str = "Settlers 2 - Gold";

/// Folders only the Gold Edition has: the World Campaign's maps, texts and pictures, and the
/// twelve new maps.
const FOLDERS: [&str; 4] = ["DATA/MAPS2/", "DATA/MAPS4/", "DATA/TXT3/", "GFX/PICS/MISSION/"];

/// Files only the Gold Edition has: the winter landscape's objects and buildings, the map
/// editor's graphics, the World Campaign's map of the world and the screens it adds.
const FILES: [&str; 34] = [
    "DATA/MAP_2_Y.LST",
    "DATA/MAP_2_Z.LST",
    "DATA/MBOB/WAFR_Y.LST",
    "DATA/MBOB/WAFR_Z.LST",
    "DATA/MBOB/WJAP_Y.LST",
    "DATA/MBOB/WJAP_Z.LST",
    "DATA/MBOB/WROM_Y.LST",
    "DATA/MBOB/WROM_Z.LST",
    "DATA/MBOB/WVIK_Y.LST",
    "DATA/MBOB/WVIK_Z.LST",
    "DATA/MBOB/WROM_BOB.LST",
    "DATA/MBOB/AFR_BOBS.LST",
    "DATA/MBOB/JAP_BOBS.LST",
    "DATA/MBOB/ROM_BOBS.LST",
    "DATA/MBOB/VIK_BOBS.LST",
    "DATA/EDITBOB.LST",
    "DATA/MAP00.LST",
    "DATA/MAP01.LST",
    "DATA/MAP02.LST",
    "DATA/MAPBOBS.LST",
    "DATA/MAPBOBS0.LST",
    "DATA/MAPBOBS1.LST",
    "DATA/EDITRES",
    "DATA/IO/EDITIO",
    "GFX/PICS/WORLD.LBM",
    "GFX/PICS/WORLDMSK.LBM",
    "GFX/PICS/SETUP666.LBM",
    "GFX/PICS/SETUP667.LBM",
    "GFX/PICS/SETUP810.LBM",
    "GFX/PICS/SETUP811.LBM",
    "GFX/PICS/SETUP899.LBM",
    "GFX/PICS/SETUP901.LBM",
    "GFX/PICS/SETUP990.LBM",
    "GFX/PICS/SETUP998.LBM",
];

/// Interface items the Gold Edition adds to a pair the base game also has, by the pair's data
/// path without the extension.
const ITEMS: [(&str, &[&str]); 1] = [("DATA/IO/IO", &["iobob_03", "iobob_04"])];

/// Whether the data path `path` is the Gold Edition's alone. A named pair is given without its
/// extension, as `DATA/EDITRES`.
#[must_use]
pub fn is_gold(path: &str) -> bool {
    let path = path.to_ascii_uppercase();
    FOLDERS.iter().any(|folder| path.starts_with(folder)) || FILES.contains(&path.as_str())
}

/// Whether the item `name` of the pair at `stem` is one the Gold Edition adds.
pub(crate) fn is_gold_item(stem: &str, name: &str) -> bool {
    ITEMS
        .iter()
        .any(|(pair, names)| *pair == stem && names.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gold_editions_files_and_folders_are_gold() {
        assert!(is_gold("DATA/MAPS2/AFRICA.WLD"));
        assert!(is_gold("data/maps4/map01.wld"));
        assert!(is_gold("DATA/MBOB/WROM_Z.LST"));
        assert!(is_gold("DATA/IO/EDITIO"));
        assert!(!is_gold("DATA/MAPS/MISS200.WLD"));
        assert!(!is_gold("DATA/MBOB/ROM_Z.LST"));
        assert!(!is_gold("DATA/CBOB/ROM_BOBS.LST"));
        assert!(!is_gold("DATA/IO/IO"));
        assert!(!is_gold("DATA/MAPS2"));
    }

    #[test]
    fn only_the_added_interface_items_are_gold() {
        assert!(is_gold_item("DATA/IO/IO", "iobob_03"));
        assert!(!is_gold_item("DATA/IO/IO", "iobob_02"));
        assert!(!is_gold_item("DATA/RESOURCE", "iobob_03"));
    }
}
