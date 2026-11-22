//! Checks every `.ENG` file of the real game. Each test runs only when its variable points at a
//! copy of that edition and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: installed S2
//! - `S2_DATA_GOLD`: GOG Gold Edition
//! - `S2_DATA_CUE`: the retail CD's `.cue` (v1.03 text, before the patch)

use std::env;
use std::path::Path;

use s2_data::{DataSet, EntryKind};
use s2_formats::text::{MAGIC, Page, StringTable, cp437};

const DIRS: [&str; 4] = ["DATA/TXT", "DATA/TXT2", "DATA/TXT3", "DATA/ONLINE"];

/// Non-ASCII bytes the English text uses: ü ä Ä ö Ü ß and °. All of them decode the same in code
/// pages 437 and 850.
const HIGH_BYTES: [u8; 7] = [0x81, 0x84, 0x8E, 0x94, 0x9A, 0xE1, 0xF8];

fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;

/// One string table: path, slot count, present slots, and a hash over slot numbers and bytes.
type TableSummary = (String, usize, usize, u64);

#[derive(Debug, Default, PartialEq, Eq)]
struct Summary {
    tables: Vec<TableSummary>,
    pages: usize,
    page_lines: usize,
    pages_hash: u64,
}

fn summarise(data: &DataSet) -> Summary {
    let mut summary = Summary {
        pages_hash: FNV_OFFSET,
        ..Summary::default()
    };
    for dir in DIRS {
        let Ok(entries) = data.list_dir(dir) else {
            continue;
        };
        for entry in entries {
            if entry.kind != EntryKind::File || !has_extension(&entry.name, "eng") {
                continue;
            }
            let path = format!("{dir}/{}", entry.name.to_ascii_uppercase());
            let bytes = data.read(&path).unwrap();
            if bytes.get(..2) == Some(&MAGIC.to_le_bytes()[..]) {
                summary.tables.push(table(&path, &bytes));
            } else {
                let page = Page::parse(&bytes);
                summary.pages += 1;
                summary.page_lines += page.raw_lines().count();
                summary.pages_hash = fnv1a(summary.pages_hash, page.raw());
                assert!(!page.text().contains('\u{1a}'), "{path}");
                assert!(
                    page.raw().iter().all(|b| b.is_ascii() || HIGH_BYTES.contains(b)),
                    "{path}"
                );
            }
        }
    }
    summary
}

fn table(path: &str, bytes: &[u8]) -> TableSummary {
    let table = StringTable::parse(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert_eq!(table.quirks(), [], "{path}");
    let mut hash = FNV_OFFSET;
    let mut present = 0;
    for (index, raw) in table.iter() {
        present += 1;
        hash = fnv1a(hash, &u32::try_from(index).unwrap().to_le_bytes());
        hash = fnv1a(hash, raw);
        assert!(
            raw.iter()
                .all(|&b| (b' '..=b'~').contains(&b) || HIGH_BYTES.contains(&b)),
            "{path} slot {index} has a control or unexpected byte"
        );
        assert_eq!(cp437::decode(raw).chars().count(), raw.len());
    }
    (path.to_owned(), table.len(), present, hash)
}

fn open(var: &str) -> Option<DataSet> {
    let Ok(path) = env::var(var) else {
        eprintln!("skipped: set {var}");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

fn check(var: &str, expected_tables: &[(&str, usize, usize, u64)], pages: (usize, usize, u64)) {
    let Some(data) = open(var) else { return };
    let actual = summarise(&data);
    let expected = Summary {
        tables: expected_tables
            .iter()
            .map(|&(p, n, k, h)| (p.to_owned(), n, k, h))
            .collect(),
        pages: pages.0,
        page_lines: pages.1,
        pages_hash: pages.2,
    };
    assert_eq!(actual, expected);
}

/// Tables shared by every edition, before `MISS_003` and from `MISS_004` on.
const MISS_000_002: [(&str, usize, usize, u64); 3] = [
    ("DATA/TXT/MISS_000.ENG", 120, 34, 0xC4EA_5B89_A626_DEE0),
    ("DATA/TXT/MISS_001.ENG", 120, 34, 0xE4B9_023A_1093_9991),
    ("DATA/TXT/MISS_002.ENG", 120, 20, 0xC03E_5B68_9D50_D121),
];

const MISS_004_NAMES: [(&str, usize, usize, u64); 8] = [
    ("DATA/TXT/MISS_004.ENG", 50, 8, 0x699C_97AF_C93A_EFD3),
    ("DATA/TXT/MISS_005.ENG", 50, 8, 0x743A_0C97_C188_A7E8),
    ("DATA/TXT/MISS_006.ENG", 50, 8, 0x9F14_6414_5946_B28E),
    ("DATA/TXT/MISS_007.ENG", 50, 8, 0xDA6A_40B5_832C_5CCF),
    ("DATA/TXT/MISS_008.ENG", 50, 4, 0x34D1_5803_2077_9AA4),
    ("DATA/TXT/MISS_009.ENG", 50, 4, 0xDF0B_E17B_22EF_F6DF),
    ("DATA/TXT/MISS_010.ENG", 50, 6, 0x56FB_5C75_6D71_1052),
    ("DATA/TXT/NAMES.ENG", 300, 165, 0x2313_88DE_AEE0_19A4),
];

/// Before Gold, `MISS_003` holds chapter 2's twenty texts in the same slots, only in a
/// 50-slot table.
const MISS_003_AS_002: (&str, usize, usize, u64) = ("DATA/TXT/MISS_003.ENG", 50, 20, 0xC03E_5B68_9D50_D121);

fn tables(
    miss_003: (&'static str, usize, usize, u64),
    tail: &[(&'static str, usize, usize, u64)],
) -> Vec<(&'static str, usize, usize, u64)> {
    let mut all = MISS_000_002.to_vec();
    all.push(miss_003);
    all.extend_from_slice(&MISS_004_NAMES);
    all.extend_from_slice(tail);
    all
}

#[test]
fn plain_v151() {
    check(
        "S2_DATA_PLAIN",
        &tables(
            MISS_003_AS_002,
            &[("DATA/TXT/ONGAME.ENG", 600, 405, 0xAB6F_E866_C324_2CDE)],
        ),
        (75, 353, 0x6499_0027_4403_E3FB),
    );
}

#[test]
fn gold() {
    check(
        "S2_DATA_GOLD",
        &tables(
            ("DATA/TXT/MISS_003.ENG", 50, 8, 0x8B82_3A7A_54FA_2D66),
            &[
                ("DATA/TXT/ONGAME.ENG", 600, 405, 0x4A25_0FA4_04A3_B324),
                ("DATA/TXT3/ONGAME.ENG", 220, 201, 0x1FF4_1F74_737B_A17C),
            ],
        ),
        (66, 242, 0x33C6_3B40_2490_57AC),
    );
}

#[test]
fn retail_cd() {
    check(
        "S2_DATA_CUE",
        &tables(
            MISS_003_AS_002,
            &[("DATA/TXT/ONGAME.ENG", 600, 382, 0x6489_FF1A_3C52_380D)],
        ),
        (75, 353, 0x6499_0027_4403_E3FB),
    );
}

fn has_extension(name: &str, ext: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}
