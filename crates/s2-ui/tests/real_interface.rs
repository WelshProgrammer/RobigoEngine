//! Builds the interface's items, fonts and texts from a player's copy of the game. Each test runs
//! only when its variable is set, and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only counts, sizes and names are asserted; no game data lives here.

use std::path::PathBuf;

use s2_data::DataSet;
use s2_formats::idx::Index;
use s2_formats::text::StringTable;
use s2_ui::Rect;
use s2_ui::border::Border;
use s2_ui::items::{self, Items, Texts};
use s2_ui::picture::Picture;
use s2_ui::skin::{Control, Look};
use s2_ui::text::{FontId, Labels};

/// How many entries each index holds, as [`docs/formats/idx.md`] records them.
const IO_ENTRIES: [(&str, usize); 2] = [("S2_DATA_PLAIN", 262), ("S2_DATA_GOLD", 264)];

/// The line height of each font, which is the number its name carries.
const LINE_HEIGHTS: [(FontId, u32); 3] = [(FontId::Small, 9), (FontId::Medium, 11), (FontId::Large, 14)];

/// The map screen's four buttons and the size of the item each draws. A control record gives the
/// rectangle the control answers to, and the item it names is smaller.
const MAP_BUTTONS: [(&str, u16, u16); 4] = [
    ("io067", 31, 31),
    ("io068", 31, 31),
    ("io069", 31, 31),
    ("io090", 32, 32),
];

fn data(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

fn index(data: &DataSet, stem: &str) -> Index {
    let idx = data.read(&format!("{stem}.IDX")).unwrap();
    let dat = data.read(&format!("{stem}.DAT")).unwrap();
    Index::parse(&idx, &dat).unwrap()
}

fn interface(data: &DataSet) -> Items {
    Items::from_index(&index(data, "DATA/IO/IO"))
}

#[test]
fn every_entry_takes_a_slot_of_its_own() {
    for (var, count) in IO_ENTRIES {
        let Some(data) = data(var) else { continue };
        let items = interface(&data);
        assert_eq!(items.len(), count, "{var}");
        assert_eq!(items.drawn(), count, "{var}: every entry holds a picture");
    }
}

#[test]
fn a_control_records_short_name_finds_its_item() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let items = interface(&data);
    for (name, width, height) in MAP_BUTTONS {
        let id = items.id(name).unwrap_or_else(|| panic!("{name} resolves"));
        let sprite = items.sheet().get(usize::from(id.0)).expect("a picture");
        assert_eq!((sprite.width(), sprite.height()), (width, height), "{name}");
    }
}

#[test]
fn the_long_and_short_names_reach_the_same_item() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let items = interface(&data);
    assert_eq!(items.id("io067"), items.id("iocon_067"));
    assert!(items.id("back_01").is_some(), "a style's background");
    assert!(items.id("rahm_00").is_some(), "a style's frame");
    assert!(items.id("iobob_00").is_some(), "the graph paper");
    assert_eq!(items.id("io999"), None, "no such control item");
}

/// Each piece of a window's frame and the size the shipped `RESOURCE` gives it.
const CHROME: [(&str, u16, u16); 10] = [
    ("leftbord", 35, 20),
    ("rghtbord", 34, 20),
    ("titlepas", 64, 20),
    ("titleact", 64, 20),
    ("leftfram", 10, 64),
    ("rghtfram", 10, 64),
    ("bottfram", 64, 11),
    ("closicup", 16, 16),
    ("iconicup", 16, 16),
    ("dmmyicup", 16, 16),
];

#[test]
fn the_window_frame_comes_out_of_the_resource_index() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let items = interface(&data).and_index(&index(&data, "DATA/RESOURCE"));
    for (name, width, height) in CHROME {
        let id = items.id(name).unwrap_or_else(|| panic!("{name} resolves"));
        let sprite = items.sheet().get(usize::from(id.0)).expect("a picture");
        assert_eq!((sprite.width(), sprite.height()), (width, height), "{name}");
    }
    let skin = items::skin(&items);
    assert_ne!(
        skin.item(Control::Title, Look::On),
        skin.item(Control::Title, Look::Idle),
        "a window in front carries its own title bar"
    );
    for control in Control::CHROME {
        assert!(skin.item(control, Look::Idle).is_some(), "{control:?}");
    }
}

#[test]
fn every_picture_the_original_names_resolves() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let items = interface(&data);
    let skin = items::skin(&items);
    for place in 0..Picture::COUNT {
        let picture = Picture(u16::try_from(place).unwrap());
        assert!(
            skin.picture(picture).is_some(),
            "picture {place} draws {}",
            s2_ui::picture::ORIGINAL[place]
        );
    }
}

/// The pictures are icons drawn over a control's face, so they carry transparency and are
/// smaller than the rectangles the control records give them.
#[test]
fn a_picture_is_an_icon_rather_than_a_whole_button() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let items = interface(&data);
    let skin = items::skin(&items);
    let id = skin.picture(Picture::CONFIRM).expect("the tick");
    let sprite = items.sheet().get(usize::from(id.0)).expect("a picture");
    assert_eq!((sprite.width(), sprite.height()), (12, 10));
    let covered = usize::from(sprite.width()) * usize::from(sprite.height());
    assert!(
        sprite.drawn() * 2 < covered,
        "under half of it is drawn: {} of {covered}",
        sprite.drawn()
    );
}

#[test]
fn the_shared_chrome_resolves() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let items = interface(&data);
    let skin = items::skin(&items);
    for control in Control::ALL.into_iter().filter(|c| !c.is_chrome()) {
        for look in Look::ALL {
            assert!(
                skin.item(control, look).is_some(),
                "{control:?} draws in {look:?}"
            );
        }
    }
    let face = skin.item(Control::Frame, Look::Idle);
    assert_eq!(
        face,
        skin.item(Control::Button, Look::Idle),
        "every control takes the first style's background"
    );
    for control in [Control::Edge, Control::GraphPaper] {
        assert_ne!(
            skin.item(control, Look::Idle),
            face,
            "{control:?} draws from an item of its own"
        );
    }
    assert_ne!(
        skin.item(Control::Button, Look::Hover),
        face,
        "a button under the pointer takes the style's other background"
    );
    assert_eq!(
        skin.item(Control::Button, Look::Pressed),
        face,
        "a held button keeps its background and lights the other edges"
    );
}

#[test]
fn the_three_fonts_come_out_of_the_resource_index() {
    for var in ["S2_DATA_PLAIN", "S2_DATA_GOLD"] {
        let Some(data) = data(var) else { continue };
        let fonts = items::fonts(&index(&data, "DATA/RESOURCE")).expect("all three fonts");
        for (id, height) in LINE_HEIGHTS {
            assert_eq!(fonts.get(id).line_height(), height, "{var} {id:?}");
        }
        // Every font draws the printable range the pen walks.
        for id in [FontId::Small, FontId::Medium, FontId::Large] {
            let glyphs = fonts.get(id);
            assert!(glyphs.glyph(b'A').is_some(), "{var} {id:?} has a letter");
            assert!(glyphs.width(b'A') > 0, "{var} {id:?} measures a letter");
        }
    }
}

#[test]
fn a_window_title_slot_reads_a_string() {
    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let table = StringTable::parse(&data.read("DATA/TXT/ONGAME.ENG").unwrap()).unwrap();
    let texts = Texts::new(table);
    // Title slots the windows use: the game menu, the map and the trading routes.
    for slot in [155, 157, 571] {
        assert!(!texts.slot(slot).is_empty(), "slot {slot} holds a title");
    }
    assert!(
        texts.slot(u16::MAX).is_empty(),
        "a slot past the table draws nothing"
    );
}

#[test]
fn every_building_type_the_menu_offers_has_an_icon() {
    use s2_assets::{Nation, SpriteSheet};
    use s2_sim::{BuildingKind, BuildingTypes};

    let Some(data) = data("S2_DATA_PLAIN") else {
        return;
    };
    let table = &BuildingTypes::ORIGINAL;
    for nation in Nation::ALL {
        let sheet = SpriteSheet::load(&data, nation.icons_path()).unwrap();
        for (place, row) in table.types().iter().enumerate() {
            let Ok(kind) = u8::try_from(place).map(BuildingKind) else {
                continue;
            };
            if kind == BuildingKind::HEADQUARTERS {
                continue;
            }
            assert!(
                sheet.get(usize::from(row.group)).is_some(),
                "{nation} has no icon in slot {} for building type {place}",
                row.group
            );
        }
    }
}

/// The pixels the four boulders and the tray draw: `dskbobol`, `dskbobor`, `dskbobul`, `dskbobur`
/// and `dskbobic` in `RESOURCE.DAT`.
const PIECE_PIXELS: u32 = 1438 + 1432 + 2032 + 2023 + 3259;

#[test]
fn the_frame_round_the_view_covers_the_border_and_every_piece() {
    for var in ["S2_DATA_PLAIN", "S2_DATA_GOLD"] {
        let Some(data) = data(var) else { continue };
        let items = interface(&data).and_index(&index(&data, "DATA/RESOURCE"));
        let border = Border::from_items(&items);
        let screen = Rect::new(0, 0, 640, 480);
        let cover = border.cover(screen);
        let strips: u32 = cover[..4].iter().map(|rect| rect.width * rect.height).sum();
        let pieces: u32 = cover[4..].iter().map(|rect| rect.width * rect.height).sum();
        assert_eq!(strips, 640 * 480 - 616 * 456, "{var}");
        assert_eq!(pieces, PIECE_PIXELS, "{var}");
    }
}
