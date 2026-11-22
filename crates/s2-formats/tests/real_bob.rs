//! Decodes the BOB files and `ROM_ANIM.DAT` from a player's own copy of the game. Each test runs
//! only when its environment variable points at the data, and passes otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only counts, ranges and FNV-1a hashes of decoded content are asserted; no game data lives
//! here.

use s2_data::DataSet;
use s2_formats::bob::{Bob, BodyType, Direction, FRAMES, Record, RomAnim, Sprite, Step};
use s2_formats::lst::{Archive, Item, Pixel, PlayerBitmap};

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

    fn bitmap(&mut self, b: &PlayerBitmap) {
        self.bytes(&b.origin().x.to_le_bytes());
        self.bytes(&b.origin().y.to_le_bytes());
        self.bytes(&b.width().to_le_bytes());
        self.bytes(&b.height().to_le_bytes());
        for pixel in b.pixels() {
            self.bytes(&match *pixel {
                Pixel::Transparent => [0, 0],
                Pixel::Colour(index) => [1, index],
                Pixel::Player(shade) => [2, shade.get()],
            });
        }
    }
}

fn open(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).expect("data set opens"))
}

/// File, overlays, items and the hash of every part and link.
const BOBS: [(&str, usize, usize, u64); 3] = [
    ("DATA/BOBS/CARRIER.BOB", 602, 34, 0xcc69_4470_8e25_b1bb),
    ("DATA/BOBS/CARRIER2.BOB", 738, 34, 0x331a_0fd6_6476_d2f1),
    ("DATA/BOBS/JOBS.BOB", 2269, 93, 0x3425_d3dc_dc12_37ba),
];

fn check_bobs(data: &DataSet) {
    for (path, overlays, items, hash) in BOBS {
        let bob = Bob::parse(&data.read(path).expect("file reads")).expect(path);
        assert_eq!(bob.quirks(), [], "{path}");
        assert_eq!(bob.bodies().len(), 96, "{path}");
        assert_eq!(bob.overlays().len(), overlays, "{path}");
        assert_eq!(bob.items(), items, "{path}");

        let mut h = Fnv::new();
        bob.bodies().iter().for_each(|b| h.bitmap(b));
        bob.overlays()
            .iter()
            .for_each(|o| h.bitmap(o.bitmap().expect("every overlay is linked")));
        for item in 0..items {
            for body in BodyType::ALL {
                for direction in Direction::ALL {
                    for frame in 0..u8::try_from(FRAMES).unwrap() {
                        let layers = bob.composite(item, body, direction, frame).expect(path);
                        for layer in layers {
                            assert_eq!(layer.bitmap.width(), 32);
                            assert!((-1..=1).contains(&layer.y), "{path}: dy {}", layer.y);
                            h.bytes(&layer.y.to_le_bytes());
                        }
                    }
                }
            }
        }
        assert_eq!(h.0, hash, "{path}");
    }
}

/// The step tally in order: frames, calls, returns, ends, map object, sounds.
type Tally = [usize; 6];

fn check_rom_anim(data: &DataSet) {
    let anim = RomAnim::parse(&data.read("DATA/ANIMDAT/ROM_ANIM.DAT").expect("file reads"))
        .expect("ROM_ANIM.DAT parses");
    assert_eq!(anim.quirks(), []);
    let bobs = Archive::parse(&data.read("DATA/CBOB/ROM_BOBS.LST").expect("file reads"))
        .expect("ROM_BOBS.LST parses");
    assert_eq!(anim.len(), 6400);

    let blank = anim.records().iter().filter(|r| r.is_blank()).count();
    assert_eq!(blank, 819);
    assert!(anim.records()[6400 - blank..].iter().all(Record::is_blank));

    let mut tally: Tally = [0; 6];
    let (mut bitmaps, mut no_item, mut figures) = (0, 0, 0);
    let mut h = Fnv::new();
    for record in &anim.records()[..6400 - blank] {
        h.bytes(&[record.kind, record.op]);
        h.bytes(&record.arg.to_le_bytes());
        h.bytes(&[record.dx.to_le_bytes()[0], record.dy.to_le_bytes()[0]]);
        let slot = match record.step() {
            Step::Frame(Sprite::Bitmap(id)) => {
                match bobs.get(usize::from(id)) {
                    Some(Item::PlayerBitmap(_)) => bitmaps += 1,
                    Some(other) => panic!("bitmap {id} is {:?}", other.kind()),
                    None => {
                        assert!(id == u16::MAX || (65_475..=65_482).contains(&id), "id {id}");
                        no_item += 1;
                    }
                }
                0
            }
            Step::Frame(Sprite::Figure { job, direction }) => {
                assert!(job < 72 && direction < 6, "figure {job}/{direction}");
                figures += 1;
                0
            }
            Step::Call { target } => {
                assert!(usize::from(target) < 6400 - blank, "call {target}");
                1
            }
            Step::End => 2,
            Step::Return => 3,
            Step::MapObject { value } => {
                assert!((1..=5).contains(&value));
                4
            }
            Step::Sound { id } => {
                assert!((1..=57).contains(&id));
                5
            }
        };
        tally[slot] += 1;
    }
    assert_eq!(tally, [4016, 946, 293, 71, 5, 250]);
    assert_eq!((bitmaps, no_item, figures), (3211, 427, 378));
    assert_eq!(h.0, 0x4f37_e91f_48b2_5b37);
}

#[test]
fn plain_bobs() {
    if let Some(data) = open("S2_DATA_PLAIN") {
        check_bobs(&data);
    }
}

#[test]
fn gold_bobs() {
    if let Some(data) = open("S2_DATA_GOLD") {
        check_bobs(&data);
    }
}

#[test]
fn plain_rom_anim() {
    if let Some(data) = open("S2_DATA_PLAIN") {
        check_rom_anim(&data);
    }
}

#[test]
fn gold_rom_anim() {
    if let Some(data) = open("S2_DATA_GOLD") {
        check_rom_anim(&data);
    }
}
