//! Decodes every LST archive and `FONT14.FNT` from a player's own copy of the game. Each test
//! runs only when its environment variable points at the data, and passes otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only counts, dimensions, type tallies and FNV-1a hashes of decoded content are asserted; no
//! game data lives here.

use std::collections::BTreeMap;

use s2_data::{DataSet, EntryKind};
use s2_formats::lst::{Archive, Font, Item, ItemType, Pixel, PlayerColour, Sound};

/// The type ids in [`Tally`] order: empty, then types 1, 2, 3, 4, 5, 7, 14.
const KINDS: [Option<ItemType>; 8] = [
    None,
    Some(ItemType::Sound),
    Some(ItemType::Bitmap),
    Some(ItemType::Font),
    Some(ItemType::PlayerBitmap),
    Some(ItemType::Palette),
    Some(ItemType::Shadow),
    Some(ItemType::RawBitmap),
];

type Tally = [usize; KINDS.len()];

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
}

fn open(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).expect("data set opens"))
}

fn lst_files(data: &DataSet, dir: &str, out: &mut Vec<String>) {
    for entry in data.list_dir(dir).expect("directory lists") {
        let path = format!("{dir}/{}", entry.name);
        match entry.kind {
            EntryKind::Directory => lst_files(data, &path, out),
            EntryKind::File if entry.name.to_ascii_uppercase().ends_with(".LST") => out.push(path),
            EntryKind::File => {}
        }
    }
}

fn hash_item(h: &mut Fnv, item: &Item) {
    h.u16(item.kind().map_or(0, ItemType::id));
    match item {
        Item::Empty => {}
        Item::Sound(sound) => h.bytes(sound.bytes()),
        Item::Palette(palette) => palette.0.iter().for_each(|rgb| h.bytes(rgb)),
        Item::Bitmap(b) => {
            h.bytes(&[b.origin().x.to_le_bytes(), b.origin().y.to_le_bytes()].concat());
            h.u16(b.width());
            h.u16(b.height());
            h.bytes(b.image().pixels());
            b.mask().bits().iter().for_each(|&m| h.bytes(&[u8::from(m)]));
        }
        Item::Shadow(s) => {
            h.bytes(&[s.origin().x.to_le_bytes(), s.origin().y.to_le_bytes()].concat());
            h.u16(s.width());
            h.u16(s.height());
            s.mask().bits().iter().for_each(|&m| h.bytes(&[u8::from(m)]));
        }
        Item::PlayerBitmap(p) => {
            h.bytes(&[p.origin().x.to_le_bytes(), p.origin().y.to_le_bytes()].concat());
            h.u16(p.width());
            h.u16(p.height());
            for pixel in p.pixels() {
                h.bytes(&match pixel {
                    Pixel::Transparent => [0, 0],
                    Pixel::Colour(i) => [1, *i],
                    Pixel::Player(s) => [2, s.get()],
                });
            }
        }
        Item::RawBitmap(r) => {
            h.bytes(&[r.origin().x.to_le_bytes(), r.origin().y.to_le_bytes()].concat());
            h.u16(r.width());
            h.u16(r.height());
            h.bytes(r.image().pixels());
        }
        Item::Font(f) => {
            h.bytes(&[f.dx(), f.dy()]);
            for (code, glyph) in f.glyphs() {
                h.bytes(&[code]);
                hash_item(h, &Item::PlayerBitmap(glyph.clone()));
            }
        }
    }
}

fn tally(archive: &Archive) -> Tally {
    let mut out = [0; KINDS.len()];
    for item in archive {
        let slot = KINDS.iter().position(|k| *k == item.kind()).expect("known kind");
        out[slot] += 1;
    }
    out
}

struct Summary {
    files: BTreeMap<String, (Tally, u64)>,
    total: Tally,
    hash: u64,
}

/// Decodes every LST under `DATA`, failing on the first error.
fn decode_all(data: &DataSet) -> Summary {
    let mut paths = Vec::new();
    lst_files(data, "DATA", &mut paths);
    let mut files = BTreeMap::new();
    let mut total = [0; KINDS.len()];
    let mut all = Fnv::new();
    for path in paths {
        let bytes = data.read(&path).expect("file reads");
        let archive = Archive::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(archive.quirks(), [], "{path}");
        let counts = tally(&archive);
        let mut h = Fnv::new();
        archive.iter().for_each(|item| hash_item(&mut h, item));
        total.iter_mut().zip(counts).for_each(|(t, c)| *t += c);
        all.bytes(path.to_ascii_uppercase().as_bytes());
        all.bytes(&h.0.to_le_bytes());
        eprintln!(
            "{path}: {} slots, tally {counts:?}, hash {:#018x}",
            archive.len(),
            h.0
        );
        files.insert(path.to_ascii_uppercase(), (counts, h.0));
    }
    eprintln!("{} files, total {total:?}, hash {:#018x}", files.len(), all.0);
    Summary {
        files,
        total,
        hash: all.0,
    }
}

fn check_shared_files(data: &DataSet, summary: &Summary) {
    let tally_of = |name: &str| summary.files[name].0;
    assert_eq!(tally_of("DATA/CBOB/ROM_BOBS.LST"), [797, 0, 0, 0, 1803, 0, 0, 0]);
    assert_eq!(tally_of("DATA/MAP_0_Z.LST"), [2076, 0, 767, 0, 47, 1, 309, 0]);
    assert_eq!(tally_of("DATA/MAP_1_Y.LST"), [2418, 0, 503, 0, 43, 1, 235, 0]);
    assert_eq!(tally_of("DATA/IO.LST"), [177, 0, 11, 0, 0, 0, 0, 12]);

    let sound = Archive::parse(&data.read("DATA/SOUNDDAT/SOUND.LST").unwrap()).unwrap();
    let (mut pcm, mut xmidi) = (0, Vec::new());
    for (index, item) in sound.iter().enumerate() {
        match item {
            Item::Sound(Sound::Pcm(_)) => pcm += 1,
            Item::Sound(Sound::Xmidi(_)) => xmidi.push(index),
            Item::Empty => {}
            other => panic!("unexpected {:?} in SOUND.LST", other.kind()),
        }
    }
    assert_eq!((sound.len(), pcm, xmidi), (200, 54, vec![0]));

    let io = Archive::parse(&data.read("DATA/IO.LST").unwrap()).unwrap();
    let raw_sizes: Vec<_> = io
        .iter()
        .filter_map(|item| match item {
            Item::RawBitmap(raw) => Some((raw.width(), raw.height())),
            _ => None,
        })
        .collect();
    assert_eq!(raw_sizes, [[(64, 64); 8].as_slice(), &[(32, 32); 4]].concat());

    let map = Archive::parse(&data.read("DATA/MAP_0_Z.LST").unwrap()).unwrap();
    let mut player_pixels = 0;
    for item in &map {
        if let Item::PlayerBitmap(bitmap) = item {
            let (image, mask) = bitmap.to_indexed(PlayerColour::DEFAULT);
            assert_eq!(image.pixels().len(), mask.bits().len());
            player_pixels += bitmap
                .pixels()
                .iter()
                .filter(|p| matches!(p, Pixel::Player(_)))
                .count();
        }
    }
    assert!(player_pixels > 0);
}

fn check_font(data: &DataSet) {
    let font = Font::parse_file(&data.read("DATA/IO/FONT14.FNT").unwrap()).unwrap();
    assert_eq!(font.quirks(), []);
    assert_eq!((font.dx(), font.dy()), (12, 14));
    assert_eq!(font.glyphs().count(), 115);
    assert!(font.glyphs().all(|(_, g)| g.height() == 14));
    let widest = font.glyphs().map(|(_, g)| g.width()).max();
    assert_eq!(widest, Some(15));
    let mut h = Fnv::new();
    hash_item(&mut h, &Item::Font(font));
    assert_eq!(h.0, FONT14_HASH);
}

const FONT14_HASH: u64 = 0xde55_8d1e_f0ff_c6fa;

#[test]
fn plain_lst_archives_decode() {
    let Some(data) = open("S2_DATA_PLAIN") else {
        return;
    };
    let summary = decode_all(&data);
    check_shared_files(&data, &summary);
    check_font(&data);
    assert_eq!(summary.files.len(), 30);
    assert_eq!(summary.total, [13086, 55, 3559, 0, 2321, 4, 1913, 12]);
    assert_eq!(summary.hash, 0xa6a0_6e3d_db26_2427);
}

#[test]
fn gold_lst_archives_decode() {
    let Some(data) = open("S2_DATA_GOLD") else {
        return;
    };
    let summary = decode_all(&data);
    check_shared_files(&data, &summary);
    check_font(&data);
    assert_eq!(summary.files.len(), 52);
    assert_eq!(summary.total, [34376, 55, 10787, 0, 3115, 13, 5572, 12]);
    assert_eq!(summary.hash, 0x6b87_3283_64d0_301b);
}
