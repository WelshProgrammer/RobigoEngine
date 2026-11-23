//! Decodes every map in a player's copy of the game. Each test runs only when its variable is
//! set, and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder (its `DATA/MAPS` still holds the v1.02 pairs)
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//! - `S2_DATA_CD`: a copy of the retail CD's file tree, for its `S2/DATA/MAPS` v1.02 pairs
//!
//! Only counts, sizes, player counts and hashes are asserted; no game data lives here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use s2_data::{DataSet, EntryKind};
use s2_formats::map::{AreaKind, LayerKind, Map};

/// File name, width, height and player count of each `.WLD`, per folder.
type Expected = &'static [(&'static str, u16, u16, u8)];

const CAMPAIGN: Expected = &[
    ("MISS200.WLD", 64, 64, 7),
    ("MISS201.WLD", 64, 96, 2),
    ("MISS202.WLD", 128, 128, 3),
    ("MISS203.WLD", 128, 128, 3),
    ("MISS204.WLD", 128, 128, 3),
    ("MISS205.WLD", 192, 128, 3),
    ("MISS206.WLD", 144, 128, 3),
    ("MISS207.WLD", 144, 144, 3),
    ("MISS208.WLD", 160, 112, 3),
    ("MISS209.WLD", 128, 128, 3),
];

const UNLIMITED: Expected = &[
    ("OMAP00.WLD", 144, 144, 4),
    ("OMAP01.WLD", 144, 112, 2),
    ("OMAP02.WLD", 160, 144, 5),
    ("OMAP03.WLD", 96, 96, 4),
    ("OMAP04.WLD", 224, 192, 4),
    ("OMAP05.WLD", 112, 112, 4),
    ("OMAP06.WLD", 96, 96, 5),
    ("OMAP07.WLD", 96, 96, 3),
    ("OMAP08.WLD", 128, 144, 4),
    ("OMAP09.WLD", 144, 128, 4),
    ("OMAP10.WLD", 176, 176, 7),
    ("OMAP11.WLD", 192, 176, 6),
    ("OMAP12.WLD", 208, 112, 3),
    ("OMAP13.WLD", 144, 128, 2),
    ("OMAP14.WLD", 240, 192, 7),
    ("OMAP15.WLD", 96, 96, 4),
    ("OMAP16.WLD", 96, 96, 4),
    ("OMAP17.WLD", 96, 96, 2),
];

const WORLD_CAMPAIGN: Expected = &[
    ("AFRICA.WLD", 208, 208, 4),
    ("AUSTRA.WLD", 128, 112, 2),
    ("EUROPE.WLD", 224, 224, 6),
    ("GREEN.WLD", 96, 64, 2),
    ("JAPAN.WLD", 112, 80, 2),
    ("NAMERICA.WLD", 208, 240, 3),
    ("NASIA.WLD", 128, 112, 4),
    ("SAMERICA.WLD", 176, 208, 3),
    ("SASIA.WLD", 224, 176, 3),
];

const GOLD_EXTRA: Expected = &[
    ("MAP01.WLD", 112, 128, 4),
    ("MAP02.WLD", 64, 64, 2),
    ("MAP03.WLD", 128, 128, 3),
    ("MAP04.WLD", 112, 112, 3),
    ("MAP05.WLD", 112, 112, 4),
    ("MAP06.WLD", 80, 80, 4),
    ("MAP07.WLD", 160, 160, 7),
    ("MAP08.WLD", 96, 96, 4),
    ("MAP09.WLD", 144, 144, 3),
    ("MAP10.WLD", 64, 64, 4),
    ("MAP11.WLD", 144, 144, 6),
    ("MAP12.WLD", 112, 112, 4),
];

/// Width, height and stored layer count of each v1.02 world, by number.
const V102: &[(u16, u16, u16, usize)] = &[
    (0, 64, 64, 14),
    (100, 144, 144, 15),
    (101, 176, 176, 15),
    (102, 160, 144, 15),
    (103, 96, 96, 15),
    (104, 224, 192, 15),
    (105, 96, 96, 15),
    (106, 96, 96, 15),
    (107, 96, 96, 14),
    (108, 128, 144, 15),
    (109, 144, 128, 15),
    (110, 144, 112, 15),
    (111, 192, 176, 15),
    (112, 208, 112, 15),
    (113, 144, 128, 15),
    (114, 240, 192, 15),
    (115, 112, 112, 15),
    (116, 112, 112, 15),
    (117, 96, 96, 15),
    (118, 96, 96, 15),
    (119, 96, 96, 15),
    (200, 64, 64, 15),
    (201, 64, 96, 15),
    (202, 128, 128, 15),
    (203, 128, 128, 15),
    (204, 128, 128, 15),
    (205, 192, 128, 15),
    (206, 144, 128, 15),
    (207, 144, 144, 15),
    (208, 160, 112, 15),
    (209, 128, 128, 15),
];

const PLAIN_WLD_HASH: u64 = 0xf43a_5a8b_44a7_7459;
const GOLD_WLD_HASH: u64 = 0x6788_0396_1e9f_389d;
const V102_HASH: u64 = 0x38e7_b4b0_474b_dce0;

fn env(var: &str) -> Option<PathBuf> {
    let found = std::env::var_os(var).map(PathBuf::from);
    if found.is_none() {
        eprintln!("skipped: set {var} to run this test");
    }
    found
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn map(&mut self, map: &Map) {
        for layer in map.layers() {
            self.write(&[layer.kind().slot()]);
            self.write(layer.data());
        }
        for area in map.areas() {
            self.write(&area.size.to_le_bytes());
        }
        for animal in map.animals() {
            self.write(&[animal.species]);
            self.write(&animal.position.x.to_le_bytes());
            self.write(&animal.position.y.to_le_bytes());
        }
    }
}

/// Checks that every used area's size matches the nodes the area layer assigns to it.
fn assert_areas_consistent(name: &str, map: &Map) {
    let layer = map.layer(LayerKind::Areas).unwrap().data();
    let mut counts = [0_u32; 256];
    for &index in layer {
        counts[usize::from(index)] += 1;
    }
    for (index, area) in map.areas().iter().enumerate() {
        if area.kind != AreaKind::Unused {
            assert_eq!(area.size, counts[index], "{name}: area {index}");
            let seed = map.get(LayerKind::Areas, area.seed.x, area.seed.y);
            assert_eq!(seed.map(usize::from), Some(index), "{name}: area {index} seed");
        }
    }
}

fn wld_files(data: &DataSet, folder: &str) -> Vec<String> {
    let Ok(entries) = data.list_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries
        .into_iter()
        .filter(|e| e.kind == EntryKind::File && e.name.to_ascii_uppercase().ends_with(".WLD"))
        .map(|e| e.name.to_ascii_uppercase())
        .collect();
    names.sort();
    names
}

fn check_wld_edition(root: &Path, folders: &[(&str, Expected)]) -> Tally {
    let data = DataSet::discover(root).unwrap();
    let mut tally = Tally {
        maps: 0,
        overwritten: 0,
        hash: Fnv::new(),
    };
    for &(folder, expected) in folders {
        let names = wld_files(&data, folder);
        let listed: Vec<_> = expected.iter().map(|e| e.0.to_owned()).collect();
        assert_eq!(names, listed, "{folder}");
        for &(name, width, height, players) in expected {
            let path = format!("{folder}/{name}");
            let bytes = data.read(&path).unwrap();
            let map = Map::parse_wld(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(map.quirks(), [], "{path}");
            assert!(map.to_wld() == bytes, "{path}: written back differently");
            let header = map.header().unwrap();
            assert_eq!((map.width(), map.height()), (width, height), "{path}");
            assert_eq!(header.player_count(), players, "{path}");
            assert_eq!(map.layers().len(), LayerKind::STORED.len(), "{path}");
            assert!(map.trailer().is_empty(), "{path}");
            if header.dimensions_overwritten() {
                tally.overwritten += 1;
            } else {
                assert_eq!(header.stored_dimensions(), (width, height), "{path}");
            }
            assert_areas_consistent(&path, &map);
            tally.hash.write(path.as_bytes());
            tally.hash.map(&map);
            tally.maps += 1;
        }
    }
    println!("WLD hash {:#018x}", tally.hash.0);
    tally
}

struct Tally {
    maps: usize,
    overwritten: usize,
    hash: Fnv,
}

fn check_v102(read: impl Fn(&str) -> Vec<u8>) -> BTreeMap<u16, Map> {
    let mut maps = BTreeMap::new();
    let mut hash = Fnv::new();
    for &(number, width, height, layers) in V102 {
        let world = read(&format!("WORLD{number:03}.DAT"));
        let conti = read(&format!("CONTI{number:03}.DAT"));
        let map = Map::parse_v102(&world, &conti).unwrap_or_else(|e| panic!("{number}: {e}"));
        assert_eq!(map.quirks(), [], "{number}");
        assert_eq!((map.width(), map.height()), (width, height), "{number}");
        assert_eq!(map.layers().len(), layers, "{number}");
        assert!(map.trailer().is_empty(), "{number}");
        hash.write(&number.to_le_bytes());
        hash.map(&map);
        maps.insert(number, map);
    }
    println!("v1.02 hash {:#018x}", hash.0);
    assert_eq!(hash.0, V102_HASH);
    maps
}

#[test]
fn plain_maps_decode() {
    let Some(root) = env("S2_DATA_PLAIN") else {
        return;
    };
    let tally = check_wld_edition(&root, &[("DATA/MAPS", CAMPAIGN), ("DATA/MAPS3", UNLIMITED)]);
    assert_eq!((tally.maps, tally.overwritten), (28, 7));
    assert_eq!(tally.hash.0, PLAIN_WLD_HASH);

    let data = DataSet::discover(&root).unwrap();
    let worlds = check_v102(|name| data.read(&format!("DATA/MAPS/{name}")).unwrap());
    for number in 200..210 {
        let wld = Map::parse_wld(&data.read(&format!("DATA/MAPS/MISS{number}.WLD")).unwrap()).unwrap();
        let world = &worlds[&number];
        assert_eq!((world.width(), world.height()), (wld.width(), wld.height()));
        let old = world.layer(LayerKind::Heights).unwrap().data();
        let new = wld.layer(LayerKind::Heights).unwrap().data();
        let same = old.iter().zip(new).filter(|(a, b)| a == b).count();
        assert!(same * 100 >= new.len() * 99, "WORLD{number} heights");
    }
}

#[test]
fn gold_maps_decode() {
    let Some(root) = env("S2_DATA_GOLD") else {
        return;
    };
    let tally = check_wld_edition(
        &root,
        &[
            ("DATA/MAPS", CAMPAIGN),
            ("DATA/MAPS2", WORLD_CAMPAIGN),
            ("DATA/MAPS3", UNLIMITED),
            ("DATA/MAPS4", GOLD_EXTRA),
        ],
    );
    assert_eq!((tally.maps, tally.overwritten), (49, 7));
    assert_eq!(tally.hash.0, GOLD_WLD_HASH);
}

#[test]
fn cd_v102_maps_decode() {
    let Some(root) = env("S2_DATA_CD") else {
        return;
    };
    let maps = root.join("S2").join("DATA").join("MAPS");
    check_v102(|name| std::fs::read(maps.join(name)).unwrap());
}
