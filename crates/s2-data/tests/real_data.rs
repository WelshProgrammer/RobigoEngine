//! Checks against a player's own copy of the game. Each test runs only when its environment
//! variables point at the data, and passes silently otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//! - `S2_DATA_CUE`: the retail CD's `.cue` (with its `.bin`)
//! - `S2_DATA_PATCH`: the official v1.51 patch folder
//! - `S2_DATA_ISO`: the retail CD's data track as a 2048-byte-sector `.iso`
//!
//! Only sizes, magic numbers and track timings are asserted; no game data lives here.

use std::io::Read;
use std::path::{Path, PathBuf};

use s2_data::{AudioTrack, DataSet, EntryKind, Layer, LayerKind, Release};

const ORIGINAL_EXE_LEN: u64 = 1_902_194;
const V151_EXE_LEN: u64 = 1_929_842;
const GOLD_EXE_LEN: u64 = 1_934_962;
const INTRO_LEN: u64 = 16_383_164;
const INTRO2_LEN: u64 = 30_322_032;
/// Sector counts of audio tracks 2 to 9; the retail CD and GOG's `SETTLERS2.gog` agree.
const TRACK_SECTORS: [u64; 8] = [11_692, 13_428, 13_089, 13_028, 16_817, 33_240, 9_667, 13_199];

fn env<const N: usize>(vars: [&str; N]) -> Option<[PathBuf; N]> {
    let found = vars.map(|v| std::env::var_os(v).map(PathBuf::from));
    if found.iter().any(Option::is_none) {
        eprintln!("skipped: set {} to run this test", vars.join(" and "));
        return None;
    }
    Some(found.map(Option::unwrap))
}

fn files_under(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let parts: Vec<_> = rel
                    .components()
                    .map(|c| c.as_os_str().to_str().unwrap())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    out.sort();
    out
}

fn assert_smacker(data: &DataSet, path: &str, len: u64) {
    let mut reader = data.open(path).unwrap();
    assert_eq!(reader.len(), len, "{path}");
    let mut magic = [0; 4];
    reader.read_exact(&mut magic).unwrap();
    assert_eq!(&magic, b"SMK2", "{path}");
}

fn assert_tracks(data: &DataSet, first_start: u64) {
    let audio = data.cd_audio().expect("CD audio");
    let numbers: Vec<u8> = audio.tracks().iter().map(AudioTrack::number).collect();
    assert_eq!(numbers, (2..=9).collect::<Vec<u8>>());
    let sectors: Vec<u64> = audio.tracks().iter().map(AudioTrack::sector_count).collect();
    assert_eq!(sectors, TRACK_SECTORS);
    assert_eq!(audio.tracks()[0].start_sector(), first_start);
    let mut pcm = Vec::new();
    let mut reader = audio.open_track(9).unwrap();
    assert_eq!(reader.len(), TRACK_SECTORS[7] * 2352);
    reader.read_to_end(&mut pcm).unwrap();
    assert_eq!(pcm.len() as u64, TRACK_SECTORS[7] * 2352);
}

fn assert_retail_cd(data: &DataSet) {
    let edition = data.edition().unwrap();
    assert_eq!(
        (edition.release(), edition.version()),
        (Release::Original, "1.03")
    );
    assert_eq!(data.file_size("S2.EXE").unwrap(), ORIGINAL_EXE_LEN);
    assert_eq!(data.file_size("data\\resource.idx").unwrap(), 1600);
    assert!(!data.exists("ROOT") && !data.exists("UNIVBE") && !data.exists("DATA/BOOTBOBS.LST"));
    let root: Vec<String> = data.list_dir("").unwrap().into_iter().map(|e| e.name).collect();
    for name in [
        "DATA",
        "GFX",
        "DRIVERS",
        "SAVE",
        "VIDEO",
        "S2.EXE",
        "SETUP.EXE",
        "INSTALL.SCR",
    ] {
        assert!(root.iter().any(|n| n == name), "{name} missing from {root:?}");
    }
    assert_smacker(data, "VIDEO/INTRO.SMK", INTRO_LEN);
    assert_smacker(data, "video/intro2.smk", INTRO2_LEN);
}

#[test]
fn plain_install() {
    let Some([plain]) = env(["S2_DATA_PLAIN"]) else {
        return;
    };
    let data = DataSet::discover(plain).unwrap();
    assert_eq!(data.layers()[0].kind(), LayerKind::Folder);
    let edition = data.edition().unwrap();
    assert_eq!((edition.release(), edition.version()), (Release::V151, "1.51"));
    assert_eq!(data.file_size("s2.exe").unwrap(), V151_EXE_LEN);
    assert_eq!(data.file_size("DATA/TXT/NAMES.ENG").unwrap(), 2528);
    assert_eq!(data.metadata("data/maps3").unwrap().kind, EntryKind::Directory);
    assert!(!data.exists("DATA/MAPS2/AFRICA.WLD"));
    assert_smacker(&data, "VIDEO\\INTRO.SMK", INTRO_LEN);
}

#[test]
fn gold_edition() {
    let Some([gold]) = env(["S2_DATA_GOLD"]) else {
        return;
    };
    let data = DataSet::discover(&gold).unwrap();
    let edition = data.edition().unwrap();
    assert_eq!((edition.release(), edition.version()), (Release::Gold, "1.51"));
    assert_eq!(data.file_size("S2.EXE").unwrap(), GOLD_EXE_LEN);
    assert_eq!(
        data.metadata("DATA/MAPS2/africa.wld").unwrap().kind,
        EntryKind::File
    );
    assert_smacker(&data, "VIDEO/INTRO.SMK", INTRO_LEN);
    assert_tracks(&data, 171);
    let from_sheet = DataSet::discover(gold.join("SETTLERS2.ins")).unwrap();
    assert_eq!(from_sheet.edition().unwrap().release(), Release::Gold);
    assert_tracks(&from_sheet, 171);
}

#[test]
fn retail_cd_image() {
    let Some([cue]) = env(["S2_DATA_CUE"]) else { return };
    let data = DataSet::discover(&cue).unwrap();
    assert_eq!(data.layers()[0].kind(), LayerKind::Image);
    assert_retail_cd(&data);
    assert_tracks(&data, 151_500);
}

#[test]
fn retail_cd_iso() {
    let Some([iso]) = env(["S2_DATA_ISO"]) else { return };
    let data = DataSet::discover(iso).unwrap();
    assert_retail_cd(&data);
    assert!(data.cd_audio().is_none());
}

#[test]
fn retail_cd_with_patch() {
    let Some([cue, patch]) = env(["S2_DATA_CUE", "S2_DATA_PATCH"]) else {
        return;
    };
    let cd = DataSet::discover(&cue).unwrap();
    let data = cd.clone().with_layer(Layer::folder(&patch).unwrap());
    let edition = data.edition().unwrap();
    assert_eq!((edition.release(), edition.version()), (Release::V151, "1.51"));
    assert_eq!(data.file_size("S2.EXE").unwrap(), V151_EXE_LEN);

    for rel in files_under(&patch) {
        let patched = std::fs::read(patch.join(&rel)).unwrap();
        assert_eq!(
            data.read(&rel).unwrap(),
            patched,
            "{rel} should come from the patch"
        );
    }
    for changed in ["S2.EXE", "DATA/TXT/ONGAME.ENG"] {
        assert_ne!(
            cd.read(changed).unwrap(),
            data.read(changed).unwrap(),
            "{changed}"
        );
    }
    assert!(!cd.exists("DATA/BOOTBOBS.LST") && data.exists("data/bootbobs.lst"));
    assert!(!cd.exists("DATA/MAPS3") && data.list_dir("DATA/MAPS3").unwrap().len() == 18);
    assert_eq!(
        data.read("DATA/RESOURCE.IDX").unwrap(),
        cd.read("DATA/RESOURCE.IDX").unwrap()
    );
}

#[test]
fn patched_cd_matches_plain_install() {
    let Some([cue, patch, plain]) = env(["S2_DATA_CUE", "S2_DATA_PATCH", "S2_DATA_PLAIN"]) else {
        return;
    };
    let data = DataSet::discover(cue)
        .unwrap()
        .with_layer(Layer::folder(patch).unwrap());
    for rel in files_under(&plain) {
        let expected = std::fs::read(plain.join(&rel)).unwrap();
        assert_eq!(data.read(&rel).unwrap(), expected, "{rel}");
    }
}

#[test]
fn retail_and_gog_audio_match_after_a_lag() {
    let Some([cue, gold]) = env(["S2_DATA_CUE", "S2_DATA_GOLD"]) else {
        return;
    };
    let cd = DataSet::discover(cue).unwrap();
    let gog = DataSet::discover(gold).unwrap();
    let (cd, gog) = (cd.cd_audio().unwrap(), gog.cd_audio().unwrap());
    for track in 2..=9 {
        let mut a = Vec::new();
        let mut b = Vec::new();
        cd.open_track(track).unwrap().read_to_end(&mut a).unwrap();
        gog.open_track(track).unwrap().read_to_end(&mut b).unwrap();
        assert_eq!(a.len(), b.len(), "track {track}");
        let shift = gog_lag(&a, &b).unwrap_or_else(|| panic!("track {track}: no common audio"));
        assert!(
            a[..a.len() - shift] == b[shift..],
            "track {track} differs beyond a {shift}-byte lag"
        );
    }
}

/// GOG's rip runs 1763 to 1775 sample frames behind the retail CD, varying by track.
fn gog_lag(cd: &[u8], gog: &[u8]) -> Option<usize> {
    const WINDOW: usize = 8192;
    let mid = cd.len() / 2;
    let probe = &cd[mid..mid + 256];
    gog[mid..mid + WINDOW + probe.len()]
        .windows(probe.len())
        .position(|w| w == probe)
}
