//! Decodes every IFF picture, IFF palette and Gouraud shading table in a player's own copy of the
//! game. Each test runs only when its environment variable points at the data, and passes with a
//! note otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only counts, dimensions and FNV-1a hashes are asserted; no game data lives here.

use std::fmt::Write;

use s2_data::{DataSet, EntryKind};
use s2_formats::gouraud::{self, ShadingTable};
use s2_formats::iff::{self, Compression, Form, Picture};

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn dataset(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

fn files(data: &DataSet, dir: &str, keep: &dyn Fn(&str) -> bool, out: &mut Vec<String>) {
    for entry in data.list_dir(dir).unwrap() {
        let path = if dir.is_empty() {
            entry.name.to_uppercase()
        } else {
            format!("{dir}/{}", entry.name.to_uppercase())
        };
        match entry.kind {
            EntryKind::Directory => files(data, &path, keep, out),
            EntryKind::File if keep(&path) => out.push(path),
            EntryKind::File => {}
        }
    }
}

fn extension(path: &str) -> &str {
    path.rsplit_once('.').map_or("", |(_, ext)| ext)
}

fn is_iff(path: &str) -> bool {
    matches!(extension(path), "LBM" | "BBM")
}

fn is_shading(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();
    path.starts_with("DATA/TEXTURES/") && name.starts_with("GOU") && extension(name) == "DAT"
}

fn expected_size(path: &str) -> Option<(u16, u16)> {
    let name = path.rsplit('/').next()?;
    if path.starts_with("GFX/TEXTURES/") {
        Some((256, 256))
    } else if name == "INSTALL.LBM" {
        Some((320, 200))
    } else if name.starts_with("WORLD") {
        Some((512, 340))
    } else if path.starts_with("GFX/PICS") {
        Some((640, 480))
    } else {
        None
    }
}

fn palette_bytes(picture: &Picture) -> Vec<u8> {
    picture
        .palette
        .as_ref()
        .unwrap()
        .0
        .iter()
        .flatten()
        .copied()
        .collect()
}

/// Pixel hashes for the terrain sheets and colour hashes for the palettes; both editions agree.
const KNOWN: [(&str, u64); 6] = [
    ("GFX/TEXTURES/TEX5.LBM", 0xdecb_d97f_a1ec_4771),
    ("GFX/TEXTURES/TEX6.LBM", 0xac7d_09f2_2e87_e8a7),
    ("GFX/TEXTURES/TEX7.LBM", 0xacba_1aa1_10e9_c84d),
    ("GFX/PALETTE/PAL5.BBM", 0xf87d_0a3e_7d5b_6fb8),
    ("GFX/PALETTE/PAL6.BBM", 0x1ff4_f80f_d9af_b8ee),
    ("GFX/PALETTE/PAL7.BBM", 0xb2a5_209a_9de5_714e),
];

fn check_pictures(var: &str, count: usize, digest: u64) {
    let Some(data) = dataset(var) else { return };
    let mut paths = Vec::new();
    files(&data, "", &is_iff, &mut paths);
    let mut manifest = String::new();
    for path in &paths {
        let picture = iff::parse(&data.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(picture.quirks(), [], "{path}");
        let image = &picture.image;
        let (width, height) = (image.width(), image.height());
        assert_eq!(picture.form, Form::Pbm, "{path}");
        assert!(picture.palette.is_some(), "{path}");
        assert!(picture.ranges.iter().all(|r| r.flags == 0), "{path}");
        if let Some(size) = expected_size(path) {
            assert_eq!((width, height), size, "{path}");
            assert_eq!(picture.header.compression, Compression::ByteRun1, "{path}");
        } else {
            assert!(path.starts_with("GFX/PALETTE/"), "{path}");
            assert_eq!(picture.header.compression, Compression::None, "{path}");
        }
        let pixels = fnv1a(image.pixels());
        let colours = fnv1a(&palette_bytes(&picture));
        if let Some((_, hash)) = KNOWN.iter().find(|(known, _)| known == path) {
            let actual = if extension(path) == "BBM" { colours } else { pixels };
            assert_eq!(actual, *hash, "{path}");
        }
        writeln!(
            manifest,
            "{path} {width}x{height} {:?} {pixels:016x} {colours:016x} skipped={:?}",
            picture.header.compression, picture.skipped
        )
        .unwrap();
    }
    eprint!("{manifest}");
    assert_eq!(paths.len(), count);
    assert_eq!(fnv1a(manifest.as_bytes()), digest, "manifest digest");
}

fn check_shading(var: &str) {
    let Some(data) = dataset(var) else { return };
    let mut paths = Vec::new();
    files(&data, "DATA/TEXTURES", &is_shading, &mut paths);
    let mut hashes = Vec::new();
    for path in &paths {
        let bytes = data.read(path).unwrap();
        let table = ShadingTable::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        let neutral = table.row(gouraud::NEUTRAL_LEVEL);
        assert!(neutral.iter().zip(0..=255).all(|(&s, i)| s == i), "{path}");
        let filler = table.row(gouraud::MAX_LEVEL + 1);
        assert!(filler.iter().all(|&s| s == 204), "{path}");
        for level in gouraud::MAX_LEVEL + 1..=255 {
            assert_eq!(table.row(level), filler, "{path} level {level}");
        }
        for a in 0..=gouraud::MAX_LEVEL {
            for b in 0..a {
                assert_ne!(table.row(a), table.row(b), "{path} levels {b} and {a}");
            }
        }
        hashes.push((path.rsplit('/').next().unwrap().to_owned(), fnv1a(&bytes)));
        eprintln!("{path} {:016x}", fnv1a(&bytes));
    }
    let expected: Vec<(String, u64)> = SHADING
        .iter()
        .map(|&(name, hash)| (name.to_owned(), hash))
        .collect();
    assert_eq!(hashes, expected);
}

const SHADING: [(&str, u64); 6] = [
    ("GOU5.DAT", 0x7649_9bf6_6f50_f631),
    ("GOU6.DAT", 0x7969_5b90_58df_fca4),
    ("GOU7.DAT", 0xfcf4_0384_5c30_bf02),
    ("GOURAUD.DAT", 0x3401_b14b_4683_5ed1),
    ("GOURAUD0.DAT", 0x3401_b14b_4683_5ed1),
    ("GOURAUD1.DAT", 0xe6dc_5456_23f0_4f87),
];

#[test]
fn plain_pictures_decode() {
    check_pictures("S2_DATA_PLAIN", 26, 0xd8b8_2b03_c782_1394);
}

#[test]
fn gold_pictures_decode() {
    check_pictures("S2_DATA_GOLD", 44, 0xd9ad_5d01_7308_f486);
}

#[test]
fn plain_shading_tables() {
    check_shading("S2_DATA_PLAIN");
}

#[test]
fn gold_shading_tables() {
    check_shading("S2_DATA_GOLD");
}
