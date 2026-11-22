//! Decodes the savegames in a player's copy of the game. Each test runs only when its variable is
//! set, and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Saves are the player's own and change every time they play, so nothing here pins sizes or
//! hashes. The tests decode every slot that is present and check rules that hold for any save the
//! game writes. No game data lives here.

use std::path::PathBuf;

use s2_data::DataSet;
use s2_formats::map::{AreaKind, LayerKind};
use s2_formats::save::{
    ControlLayout, DirEntry, Directory, MissionProgress, Part, ScreenMode, Slot, SlotFiles,
    WorldCampaignProgress,
};

/// Clock units per in-game minute, bounded by the saves seen so far (see `docs/formats/save.md`).
const UNITS_PER_MINUTE: (u32, u32) = (400, 600);

fn open(var: &str) -> Option<DataSet> {
    let Some(root) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(root).unwrap())
}

fn read(data: &DataSet, part: Part, slot: u16) -> Option<Vec<u8>> {
    let path = format!("SAVE/{}", part.file_name(slot));
    data.exists(&path).then(|| data.read(&path).unwrap())
}

/// Splits a generated save name, `'<map title>'-H:MM`, into the title and the minutes.
fn parse_name(name: &str) -> Option<(&str, u32)> {
    let (quoted, time) = name.rsplit_once('-')?;
    let title = quoted.strip_prefix('\'')?.strip_suffix('\'')?;
    let (hours, minutes) = time.split_once(':')?;
    (minutes.len() == 2).then_some(())?;
    Some((
        title,
        hours.parse::<u32>().ok()? * 60 + minutes.parse::<u32>().ok()?,
    ))
}

/// Decodes one slot and checks the rules every save follows. Returns the game clock.
fn check_slot(data: &DataSet, slot: u16, entry: Option<&DirEntry>) -> u32 {
    let parts: Vec<_> = Part::ALL.iter().map(|&p| read(data, p, slot)).collect();
    let [world, transport, areas, control, desktop, mails, computer] = parts.as_slice() else {
        unreachable!()
    };
    let files = SlotFiles {
        world: world.as_deref().expect("world file"),
        areas: areas.as_deref().expect("area table"),
        transport: transport.as_deref(),
        control: control.as_deref(),
        desktop: desktop.as_deref(),
        mails: mails.as_deref(),
        computer: computer.as_deref(),
    };
    let decoded = Slot::parse(&files).unwrap_or_else(|e| panic!("slot {slot}: {e}"));
    assert_eq!(decoded.quirks(), [], "slot {slot}");
    assert_eq!(
        decoded.missing(),
        [],
        "slot {slot}: the game always writes all seven parts"
    );

    let map = decoded.world();
    assert_eq!(map.layers().len(), LayerKind::STORED.len(), "slot {slot}");
    let area_layer = map.layer(LayerKind::Areas).expect("area layer").data();
    for (index, area) in map.areas().iter().enumerate() {
        if matches!(area.kind, AreaKind::Land | AreaKind::Water) {
            let nodes = area_layer.iter().filter(|&&a| usize::from(a) == index).count();
            assert_eq!(nodes, area.size as usize, "slot {slot} area {index}");
        }
    }

    let control = decoded.control().expect("control block");
    assert_ne!(control.layout(), ControlLayout::Other, "slot {slot}");
    let desktops = decoded.desktops().expect("desktops").desktops().len();
    assert!(matches!(desktops, 1 | 2), "slot {slot}: {desktops} desktops");

    if let Some(entry) = entry {
        let name = entry.name();
        let (title, _) = parse_name(&name).unwrap_or_else(|| panic!("slot {slot}: name {name:?}"));
        assert!(!title.is_empty(), "slot {slot}");
        assert_eq!(
            entry.kind() == 2,
            desktops == 2,
            "slot {slot}: kind 2 has so far meant a split-screen save"
        );
    }
    eprintln!(
        "slot {slot}: {}x{}, clock {}, {desktops} desktop(s), {:?}",
        map.width(),
        map.height(),
        control.game_time(),
        control.layout(),
    );
    control.game_time()
}

#[test]
fn gold_saves() {
    let Some(data) = open("S2_DATA_GOLD") else {
        return;
    };
    let dir = Directory::parse(&data.read("SAVE/DIR.DAT").unwrap()).unwrap();
    assert_eq!(dir.entries().len(), 11);

    let mut sequences = Vec::new();
    let (mut low, mut high) = (0_u32, u32::MAX);
    for slot in dir.entries().iter().map(DirEntry::slot) {
        if read(&data, Part::World, slot).is_none() {
            continue;
        }
        let entry = dir.entries().iter().find(|e| e.slot() == slot && e.is_used());
        let clock = check_slot(&data, slot, entry);
        let Some(entry) = entry else { continue };
        sequences.push(entry.sequence());

        let (_, minutes) = parse_name(&entry.name()).expect("generated name");
        if minutes > 0 {
            low = low.max(clock / (minutes + 1) + 1);
            high = high.min(clock / minutes);
        }
    }
    sequences.sort_unstable();
    assert!(
        sequences.windows(2).all(|w| w[0] < w[1]),
        "unique sequence numbers"
    );
    assert!(sequences.last().is_none_or(|&s| s <= dir.counter()));

    eprintln!("clock units per in-game minute: {low}..={high}");
    assert!(low <= high, "save names and clocks disagree");
    if high != u32::MAX {
        assert!(low <= UNITS_PER_MINUTE.1 && high >= UNITS_PER_MINUTE.0);
    }
}

#[test]
fn gold_settings() {
    let Some(data) = open("S2_DATA_GOLD") else {
        return;
    };
    let res = ScreenMode::parse(&data.read("SAVE/RES.DAT").unwrap()).unwrap();
    assert!(res.mode() <= 2);

    let mission = MissionProgress::parse(&data.read("SAVE/MISSION.DAT").unwrap()).unwrap();
    assert!(mission.unlocked()[0], "the first chapter is always open");
    if data.exists("SAVE/MISSION2.DAT") {
        WorldCampaignProgress::parse(&data.read("SAVE/MISSION2.DAT").unwrap()).unwrap();
    }
}

#[test]
fn plain_mission() {
    let Some(data) = open("S2_DATA_PLAIN") else {
        return;
    };
    let mission = MissionProgress::parse(&data.read("SAVE/MISSION.DAT").unwrap()).unwrap();
    assert!(mission.unlocked().iter().filter(|&&u| u).count() >= 2);
}
