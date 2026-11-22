//! Parses every shipped mission script. Each test runs only when its variable points at a copy of
//! that edition and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: installed S2
//! - `S2_DATA_GOLD`: GOG Gold Edition
//! - `S2_DATA_CUE`: the retail CD's `.cue`
//!
//! All three ship the same 31 scripts, so they share one set of expectations.

use std::collections::BTreeMap;
use std::env;
use std::path::Path;

use s2_data::{DataSet, EntryKind};
use s2_formats::rtx::{Keyword, Script};

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;

fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

/// FNV-1a over each file name and the `Debug` form of each of its commands, in directory order.
/// Renaming an `Op` field changes it.
const FINGERPRINT: u64 = 0xC301_CA48_77F6_0EFC;

const TALLY: [(Keyword, usize); 39] = [
    (Keyword::LoadMissionTexts, 10),
    (Keyword::SetActPlayer, 20),
    (Keyword::SetHouse, 35),
    (Keyword::AddWare, 378),
    (Keyword::AddPeople, 839),
    (Keyword::EnableHouse, 713),
    (Keyword::EnableAllHouses, 6),
    (Keyword::DisableHouse, 2),
    (Keyword::AddAnimal, 317),
    (Keyword::GlobalAddComputerPlayer, 17),
    (Keyword::GlobalSetMap, 31),
    (Keyword::GlobalSetBobs, 31),
    (Keyword::GlobalSetPattern, 4),
    (Keyword::GlobalSetPalette, 4),
    (Keyword::GlobalSetMapbobs, 4),
    (Keyword::GlobalSetHqBobs, 1),
    (Keyword::GlobalSetComputerBarrier, 30),
    (Keyword::GlobalSetComputerAlliance, 25),
    (Keyword::FmMaxPlayer, 20),
    (Keyword::FmMaxHuman, 20),
    (Keyword::FmHqPos, 79),
    (Keyword::FmCharacter, 84),
    (Keyword::MetPositionOccupied, 15),
    (Keyword::MetPositionExplored, 9),
    (Keyword::MetAddAsActEvent, 63),
    (Keyword::MetEndEvent, 15),
    (Keyword::MetHouseAmount, 10),
    (Keyword::MetWareAmount, 1),
    (Keyword::MetLandSize, 1),
    (Keyword::MetContactToPlayer, 8),
    (Keyword::MetFoundGold, 1),
    (Keyword::MetFoundIron, 1),
    (Keyword::MetFoundCoal, 1),
    (Keyword::MetFoundGranite, 1),
    (Keyword::MetFoundWater, 1),
    (Keyword::MetHouseEnabling, 19),
    (Keyword::MetDirectEvent, 10),
    (Keyword::MetSetFinalEvent, 10),
    (Keyword::MetSetMapElement, 10),
];

#[derive(Debug, PartialEq, Eq)]
struct Summary {
    files: usize,
    sections: BTreeMap<String, usize>,
    tally: BTreeMap<Keyword, usize>,
    fingerprint: u64,
}

fn check(var: &str) {
    let Ok(path) = env::var(var) else {
        eprintln!("skipped: set {var}");
        return;
    };
    let data = DataSet::discover(path).unwrap();
    let mut actual = Summary {
        files: 0,
        sections: BTreeMap::new(),
        tally: BTreeMap::new(),
        fingerprint: FNV_OFFSET,
    };
    for entry in data.list_dir("DATA/MISSIONS").unwrap() {
        let name = entry.name.to_ascii_uppercase();
        if entry.kind != EntryKind::File || !has_extension(&name, "rtx") {
            continue;
        }
        let bytes = data.read(&format!("DATA/MISSIONS/{name}")).unwrap();
        let script = Script::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(script.quirks(), [], "{name}");
        assert!(script.global_settings().is_some(), "{name}");
        assert!(script.map_commands().is_some(), "{name}");
        actual.files += 1;
        for section in &script.sections {
            *actual.sections.entry(section.name.clone()).or_default() += 1;
        }
        actual.fingerprint = fnv1a(actual.fingerprint, name.as_bytes());
        for command in script.commands() {
            *actual.tally.entry(command.op.keyword()).or_default() += 1;
            actual.fingerprint = fnv1a(actual.fingerprint, format!("{command:?}").as_bytes());
        }
    }
    let expected = Summary {
        files: 31,
        sections: [("ENDE", 9), ("GLOBAL_MAP_SETTINGS", 31), ("MAPCOMMANDS", 31)]
            .into_iter()
            .map(|(n, c)| (n.to_owned(), c))
            .collect(),
        tally: TALLY.into_iter().collect(),
        fingerprint: FINGERPRINT,
    };
    assert_eq!(actual, expected);

    let unused: Vec<&str> = Keyword::ALL
        .iter()
        .filter(|k| !actual.tally.contains_key(k))
        .map(|k| k.name())
        .collect();
    assert_eq!(unused, ["MET_CLEAR_ACT_EVENTS", "MET_PEOPLE_AMOUNT"]);
}

#[test]
fn plain_v151() {
    check("S2_DATA_PLAIN");
}

#[test]
fn gold() {
    check("S2_DATA_GOLD");
}

#[test]
fn retail_cd() {
    check("S2_DATA_CUE");
}

fn has_extension(name: &str, ext: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}
