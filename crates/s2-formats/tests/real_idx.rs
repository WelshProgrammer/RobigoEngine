//! Decodes every IDX/DAT pair from a player's own copy of the game. Each test runs only when its
//! environment variable points at the data, and passes otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only entry names, counts, type tallies, byte coverage and FNV-1a hashes of decoded content
//! are asserted; no game data lives here.

use s2_data::{DataSet, EntryKind};
use s2_formats::idx::{Entry, Index};
use s2_formats::lst::{Font, Item, ItemType, Pixel};

/// Type ids in [`Tally`] order: empty, then 1, 2, 3, 4, 5, 7, 14.
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

    fn i16(&mut self, v: i16) {
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

/// Every `.IDX` under `dir`, as an upper-case path without the extension.
fn idx_stems(data: &DataSet, dir: &str, out: &mut Vec<String>) {
    for entry in data.list_dir(dir).expect("directory lists") {
        let path = format!("{dir}/{}", entry.name).to_ascii_uppercase();
        match entry.kind {
            EntryKind::Directory => idx_stems(data, &path, out),
            EntryKind::File => {
                if let Some(stem) = path.strip_suffix(".IDX") {
                    out.push(stem.to_owned());
                }
            }
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
            h.i16(b.origin().x);
            h.i16(b.origin().y);
            h.u16(b.width());
            h.u16(b.height());
            h.bytes(b.image().pixels());
            b.mask().bits().iter().for_each(|&m| h.bytes(&[u8::from(m)]));
        }
        Item::Shadow(s) => {
            h.i16(s.origin().x);
            h.i16(s.origin().y);
            h.u16(s.width());
            h.u16(s.height());
            s.mask().bits().iter().for_each(|&m| h.bytes(&[u8::from(m)]));
        }
        Item::PlayerBitmap(p) => {
            h.i16(p.origin().x);
            h.i16(p.origin().y);
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
            h.i16(r.origin().x);
            h.i16(r.origin().y);
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

/// What one pair decoded to.
#[derive(Debug, PartialEq, Eq)]
struct Pair {
    stem: String,
    entries: usize,
    tally: Tally,
    /// DAT bytes no entry covers.
    unreferenced: usize,
    hash: u64,
}

fn decode_pair(data: &DataSet, stem: &str) -> (Index, Pair) {
    let idx = data.read(&format!("{stem}.IDX")).expect("IDX reads");
    let dat = data.read(&format!("{stem}.DAT")).expect("DAT reads");
    let index = Index::parse(&idx, &dat).unwrap_or_else(|e| panic!("{stem}: {e}"));
    assert_eq!(index.quirks(), [], "{stem}");

    let mut tally = [0; KINDS.len()];
    let mut h = Fnv::new();
    for entry in &index {
        let slot = KINDS.iter().position(|k| *k == entry.kind()).expect("known kind");
        tally[slot] += 1;
        h.bytes(entry.name_bytes());
        hash_item(&mut h, entry.item());
    }

    let mut spans: Vec<_> = index.iter().map(|e| (to_usize(e.offset()), e.size())).collect();
    spans.sort_unstable();
    let mut at = 0;
    let mut unreferenced = 0;
    for (offset, size) in spans {
        assert!(offset >= at, "{stem}: overlap at {offset}");
        if offset > at {
            eprintln!("{stem}: {} unreferenced bytes at {at}", offset - at);
        }
        unreferenced += offset - at;
        at = offset + size;
    }
    if dat.len() > at {
        eprintln!("{stem}: {} unreferenced bytes at the end", dat.len() - at);
    }
    unreferenced += dat.len() - at;

    let pair = Pair {
        stem: stem.to_owned(),
        entries: index.len(),
        tally,
        unreferenced,
        hash: h.0,
    };
    eprintln!("{pair:?}");
    (index, pair)
}

fn to_usize(v: u32) -> usize {
    usize::try_from(v).expect("32-bit offsets fit")
}

fn decode_all(data: &DataSet) -> Vec<(Index, Pair)> {
    let mut stems = Vec::new();
    idx_stems(data, "DATA", &mut stems);
    stems.sort_unstable();
    stems.iter().map(|stem| decode_pair(data, stem)).collect()
}

fn names(index: &Index) -> Vec<&str> {
    index.iter().map(|e| e.name().expect("ASCII name")).collect()
}

fn find<'a>(decoded: &'a [(Index, Pair)], stem: &str) -> &'a Index {
    &decoded
        .iter()
        .find(|(_, p)| p.stem == stem)
        .expect("pair present")
        .0
}

/// The fonts sit at the same positions in RESOURCE and EDITRES. The editor's copies add eight
/// glyphs to the 115 the game's fonts have.
fn check_fonts(index: &Index, glyphs: usize) {
    assert_eq!(&names(index)[..4], ["font11", "roemerpal", "font09", "font14"]);
    let fonts = [("font09", 2, 8, 9), ("FONT11", 0, 10, 11), ("Font14", 3, 12, 14)];
    for (name, position, dx, dy) in fonts {
        assert_eq!(index.position(name), Some(position));
        let Some(Item::Font(font)) = index.get(name).map(Entry::item) else {
            panic!("{name} is not a font");
        };
        assert_eq!((font.dx(), font.dy()), (dx, dy));
        assert_eq!(font.glyphs().count(), glyphs);
    }
    assert_eq!(
        index.get("roemerpal").and_then(Entry::kind),
        Some(ItemType::Palette)
    );
}

/// RESOURCE's `font14` is the same font as the stand-alone `FONT14.FNT`.
fn check_font14(data: &DataSet, resource: &Index) {
    let fnt = Font::parse_file(&data.read("DATA/IO/FONT14.FNT").unwrap()).unwrap();
    assert_eq!(resource.get("font14").map(Entry::item), Some(&Item::Font(fnt)));
}

/// Decoded RESOURCE content, identical in both editions.
const RESOURCE_HASH: u64 = 0xe567_de3f_33a0_28c8;
const RESOURCE_TALLY: Tally = [0, 0, 32, 3, 0, 1, 0, 21];

fn pair(stem: &str, entries: usize, tally: Tally, hash: u64) -> Pair {
    Pair {
        stem: stem.to_owned(),
        entries,
        tally,
        unreferenced: 0,
        hash,
    }
}

#[test]
fn plain_idx_pairs_decode() {
    let Some(data) = open("S2_DATA_PLAIN") else {
        return;
    };
    let decoded = decode_all(&data);
    let resource = find(&decoded, "DATA/RESOURCE");
    check_fonts(resource, 115);
    check_font14(&data, resource);
    let io = names(find(&decoded, "DATA/IO/IO"));
    assert_eq!((io[0], io[261]), ("back_00", "cast_30"));

    let pairs: Vec<_> = decoded.into_iter().map(|(_, p)| p).collect();
    assert_eq!(
        pairs,
        [
            pair(
                "DATA/IO/IO",
                262,
                [0, 0, 244, 0, 0, 0, 0, 18],
                0x5ccc_3bb5_7d78_e999
            ),
            pair("DATA/RESOURCE", 57, RESOURCE_TALLY, RESOURCE_HASH),
        ]
    );
}

#[test]
fn gold_idx_pairs_decode() {
    let Some(data) = open("S2_DATA_GOLD") else {
        return;
    };
    let decoded = decode_all(&data);
    let resource = find(&decoded, "DATA/RESOURCE");
    let editres = find(&decoded, "DATA/EDITRES");
    check_fonts(resource, 115);
    check_fonts(editres, 123);
    check_font14(&data, resource);
    assert_eq!(names(resource), names(editres));

    let io = names(find(&decoded, "DATA/IO/IO"));
    let bobs: Vec<_> = io.into_iter().filter(|n| n.starts_with("iobob_")).collect();
    assert_eq!(bobs, ["iobob_00", "iobob_01", "iobob_02", "iobob_03", "iobob_04"]);
    let editio = names(find(&decoded, "DATA/IO/EDITIO"));
    assert_eq!((editio[0], editio[148]), ("back_00", "iocon_396"));

    let pairs: Vec<_> = decoded.into_iter().map(|(_, p)| p).collect();
    assert_eq!(
        pairs,
        [
            pair("DATA/EDITRES", 57, RESOURCE_TALLY, 0x2e2c_ddd2_76d9_bd34),
            pair(
                "DATA/IO/EDITIO",
                149,
                [0, 0, 131, 0, 0, 0, 0, 18],
                0x8512_3dfe_0949_8cd6
            ),
            pair(
                "DATA/IO/IO",
                264,
                [0, 0, 246, 0, 0, 0, 0, 18],
                0x7f13_bf6a_fd75_f469
            ),
            pair("DATA/RESOURCE", 57, RESOURCE_TALLY, RESOURCE_HASH),
        ]
    );
}
