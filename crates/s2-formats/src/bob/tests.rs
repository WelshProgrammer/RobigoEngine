use super::*;
use crate::lst::{Pixel, Shade};

/// A player-bitmap command byte: `kind` in the top two bits, `count` in the low six.
const fn op(kind: u8, count: u8) -> u8 {
    kind << 6 | count
}

/// An empty 32-pixel row, then four pixels of index 7 and 28 transparent.
const BODY_ROWS: [u8; 4] = [op(0, 32), op(3, 4), 7, op(0, 28)];
/// An empty row, then one pixel of player shade 2 and 31 transparent.
const OVERLAY_ROWS: [u8; 4] = [op(0, 32), op(2, 1), 2, op(0, 31)];

fn le16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn block(out: &mut Vec<u8>, data: &[u8]) {
    le16(out, BLOCK_TAG);
    le16(out, u16::try_from(data.len()).unwrap());
    out.extend_from_slice(data);
}

fn part(out: &mut Vec<u8>, offsets: &[u16], hotspot_y: u8) {
    le16(out, PART_TAG);
    out.push(u8::try_from(offsets.len()).unwrap());
    for &o in offsets {
        le16(out, o);
    }
    out.push(hotspot_y);
}

/// What a test BOB file is built from.
struct Spec {
    overlays: usize,
    links: Vec<(u16, i16)>,
    blocks: [[u8; 4]; DIRECTIONS],
}

impl Spec {
    /// One item whose link `i` names overlay `i`, with a `dy` of -1 on odd frames.
    fn one_item() -> Self {
        let links = (0..LINKS_PER_ITEM)
            .map(|i| {
                let frame = i / (BODY_TYPES * DIRECTIONS);
                (u16::try_from(i).unwrap(), if frame % 2 == 1 { -1 } else { 0 })
            })
            .collect();
        Self {
            overlays: LINKS_PER_ITEM,
            links,
            blocks: [OVERLAY_ROWS; DIRECTIONS],
        }
    }

    /// Builds the file. Body `i` and overlay `i` have their hotspot row set to `i`, so tests
    /// can tell which part came back.
    fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        le16(&mut out, BOB_TAG);
        block(&mut out, &BODY_ROWS);
        for i in 0..BODY_PARTS {
            part(&mut out, &[1, 0], u8::try_from(i).unwrap());
        }
        for rows in &self.blocks {
            block(&mut out, rows);
        }
        le16(&mut out, u16::try_from(self.overlays).unwrap());
        for i in 0..self.overlays {
            part(&mut out, &[0, 1, 0], u8::try_from(i).unwrap());
        }
        le16(&mut out, u16::try_from(self.links.len()).unwrap());
        for &(overlay, dy) in &self.links {
            le16(&mut out, overlay);
            out.extend_from_slice(&dy.to_le_bytes());
        }
        out
    }
}

fn hotspot_row(bitmap: &PlayerBitmap) -> i16 {
    bitmap.origin().y
}

#[test]
fn parses_parts_and_links() {
    let bob = Bob::parse(&Spec::one_item().build()).unwrap();
    assert_eq!(bob.items(), 1);
    assert_eq!(bob.bodies().len(), BODY_PARTS);
    assert_eq!(bob.overlays().len(), LINKS_PER_ITEM);

    let body = &bob.bodies()[0];
    assert_eq!((body.width(), body.height()), (PART_WIDTH, 2));
    assert_eq!(body.origin().x, PART_HOTSPOT_X);
    assert_eq!(body.get(0, 0), Some(Pixel::Colour(7)));
    assert_eq!(body.get(4, 0), Some(Pixel::Transparent));
    assert_eq!(body.get(0, 1), Some(Pixel::Transparent));

    assert!(bob.quirks().is_empty(), "{:?}", bob.quirks());
    let overlay = bob.overlay(0).unwrap();
    assert_eq!(overlay.height(), 3);
    assert_eq!(overlay.get(0, 1), Some(Pixel::Player(Shade::new(2).unwrap())));
    assert_eq!(overlay.get(0, 2), Some(Pixel::Transparent));
}

#[test]
fn composite_follows_engine_ordering() {
    let bob = Bob::parse(&Spec::one_item().build()).unwrap();
    let [body, overlay] = bob.composite(0, BodyType::Fat, Direction::West, 5).unwrap();
    assert_eq!(hotspot_row(body.bitmap), 48 + 3 * 8 + 5);
    assert_eq!((body.x, body.y), (0, 0));
    assert_eq!(hotspot_row(overlay.bitmap), 5 * 12 + 6 + 3);
    assert_eq!((overlay.x, overlay.y), (0, -1));

    let [_, even] = bob.composite(0, BodyType::Slim, Direction::East, 4).unwrap();
    assert_eq!(even.y, 0);
    assert_eq!(hotspot_row(even.bitmap), 48);
}

#[test]
fn out_of_range_lookups_are_none() {
    let bob = Bob::parse(&Spec::one_item().build()).unwrap();
    assert!(bob.composite(1, BodyType::Slim, Direction::East, 0).is_none());
    assert!(bob.composite(0, BodyType::Slim, Direction::East, 8).is_none());
    assert!(bob.body(BodyType::Fat, Direction::NorthEast, 8).is_none());
    assert!(
        bob.link(usize::MAX, BodyType::Fat, Direction::NorthEast, 0)
            .is_none()
    );
    assert!(Direction::from_index(6).is_none());
    assert_eq!(Direction::from_index(4), Some(Direction::NorthWest));
}

#[test]
fn rejects_wrong_tags() {
    let mut data = Spec::one_item().build();
    data[0] = 0;
    assert_eq!(
        Bob::parse(&data),
        Err(Error::Tag {
            offset: 0,
            expected: BOB_TAG,
            found: 256
        })
    );
    let mut data = Spec::one_item().build();
    data[2 + 4 + BODY_ROWS.len()] = 0;
    assert!(matches!(
        Bob::parse(&data),
        Err(Error::Tag {
            expected: PART_TAG,
            ..
        })
    ));
}

#[test]
fn links_must_name_an_overlay() {
    let mut spec = Spec::one_item();
    spec.links[3].0 = 500;
    assert_eq!(
        Bob::parse(&spec.build()),
        Err(Error::OverlayRange {
            link: 3,
            overlay: 500,
            count: LINKS_PER_ITEM
        })
    );
}

#[test]
fn partial_items_are_kept() {
    let mut spec = Spec::one_item();
    spec.links.pop();
    let bob = Bob::parse(&spec.build()).unwrap();
    assert_eq!(
        bob.quirks(),
        [Quirk::UnusedOverlay(95), Quirk::LinkCount(LINKS_PER_ITEM - 1)]
    );
    assert_eq!(bob.items(), 0);
    assert_eq!(bob.links().len(), LINKS_PER_ITEM - 1);
    assert!(bob.composite(0, BodyType::Slim, Direction::East, 0).is_some());
    assert!(bob.composite(0, BodyType::Fat, Direction::NorthEast, 7).is_none());
}

#[test]
fn disagreeing_directions_keep_the_first_link() {
    let mut spec = Spec::one_item();
    spec.links[0].0 = 1;
    spec.links[1].0 = 0;
    spec.links[6].0 = 0;
    spec.blocks[1][2] = 3;
    let bob = Bob::parse(&spec.build()).unwrap();
    assert_eq!(
        bob.quirks(),
        [
            Quirk::OverlayDirections {
                overlay: 0,
                first: 1,
                second: 0
            },
            Quirk::UnusedOverlay(6)
        ]
    );
    let overlay = bob.overlay(0).unwrap();
    assert_eq!(overlay.get(0, 1), Some(Pixel::Player(Shade::new(3).unwrap())));

    let mut spec = Spec::one_item();
    spec.links[7].0 = 0;
    spec.blocks[1][2] = 3;
    let bob = Bob::parse(&spec.build()).unwrap();
    assert_eq!(
        bob.quirks(),
        [
            Quirk::OverlayDirections {
                overlay: 0,
                first: 0,
                second: 1
            },
            Quirk::UnusedOverlay(7)
        ]
    );
    assert_eq!(
        bob.overlay(0).unwrap().get(0, 1),
        Some(Pixel::Player(Shade::new(2).unwrap()))
    );
}

#[test]
fn unused_overlays_stay_undecoded() {
    let mut spec = Spec::one_item();
    spec.overlays += 1;
    let bob = Bob::parse(&spec.build()).unwrap();
    assert_eq!(bob.quirks(), [Quirk::UnusedOverlay(96)]);
    assert_eq!(bob.overlays().len(), LINKS_PER_ITEM + 1);
    assert!(bob.overlay(96).is_none());
    let Overlay::Unlinked(raw) = &bob.overlays()[96] else {
        panic!("overlay 96 was decoded");
    };
    assert_eq!(
        (raw.height(), raw.hotspot_y(), raw.row_starts()),
        (3, 96, &[0, 1, 0][..])
    );
}

#[test]
fn overlays_shared_between_directions_must_match() {
    let mut spec = Spec::one_item();
    spec.links[7].0 = 0;
    spec.links[95].0 = 7;
    spec.overlays -= 1;
    let bob = Bob::parse(&spec.build()).unwrap();
    let found = bob.quirks();
    assert!(found.is_empty(), "{found:?}");
    let [_, a] = bob.composite(0, BodyType::Slim, Direction::East, 0).unwrap();
    let [_, b] = bob.composite(0, BodyType::Fat, Direction::SouthEast, 0).unwrap();
    assert!(std::ptr::eq(a.bitmap, b.bitmap));
}

#[test]
fn row_quirks_name_their_part() {
    let mut data = Spec::one_item().build();
    data[2 + 4 + 3] = op(0, 29);
    let bob = Bob::parse(&data).unwrap();
    assert_eq!(
        bob.quirks(),
        (0..BODY_PARTS)
            .map(|index| Quirk::Part {
                kind: PartKind::Body,
                index,
                quirk: lst::Quirk::RowOverrun {
                    row: 0,
                    decoded: 33,
                    width: PART_WIDTH
                }
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn unreadable_rows_are_errors() {
    let mut spec = Spec::one_item();
    spec.blocks[0] = [op(1, 63), 0, 0, 0];
    assert!(matches!(
        Bob::parse(&spec.build()),
        Err(Error::Part {
            kind: PartKind::Overlay,
            source: lst::Error::Truncated(_),
            ..
        })
    ));
}

#[test]
fn parts_share_one_row_read_limit() {
    let mut rows = vec![0; 1023];
    rows.push(op(0, 32));
    let tall: Vec<u16> = vec![0; 64];
    let mut data = Vec::new();
    le16(&mut data, BOB_TAG);
    block(&mut data, &rows);
    for _ in 0..BODY_PARTS {
        part(&mut data, &tall, 0);
    }
    for _ in 0..DIRECTIONS {
        block(&mut data, &[]);
    }
    data.extend([0; 4]);
    let one = PlayerBitmap::decode_rows(Origin::default(), PART_WIDTH, 64, &rows, vec![0; 64]);
    assert!(one.is_ok());
    assert!(matches!(
        Bob::parse(&data),
        Err(Error::Part {
            kind: PartKind::Body,
            source: lst::Error::RowReads { input, .. },
            ..
        }) if input == data.len()
    ));
}

#[test]
fn trailing_bytes_are_ignored() {
    let mut data = Spec::one_item().build();
    data.push(0);
    let bob = Bob::parse(&data).unwrap();
    assert_eq!(
        bob.quirks(),
        [Quirk::TrailingBytes {
            offset: data.len() - 1,
            len: 1
        }]
    );
}

#[test]
fn every_truncation_is_an_error() {
    let data = Spec::one_item().build();
    for len in 0..data.len() {
        assert!(Bob::parse(&data[..len]).is_err(), "prefix of {len} bytes parsed");
    }
}

#[test]
fn corrupted_bytes_never_panic() {
    let data = Spec::one_item().build();
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    for _ in 0..2000 {
        let mut copy = data.clone();
        for _ in 0..4 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let at = usize::try_from(state % copy.len() as u64).unwrap();
            copy[at] = state.to_le_bytes()[7];
        }
        let _ = Bob::parse(&copy);
    }
}

fn anim_file(records: &[[u8; 6]]) -> Vec<u8> {
    let mut out = Vec::new();
    le16(&mut out, RomAnim::TAG);
    le16(&mut out, u16::try_from(records.len()).unwrap());
    for r in records {
        out.extend_from_slice(r);
    }
    out
}

/// A `ROM_ANIM.DAT` record from its fields.
fn record(kind: u8, arg: u16, dx: i8, dy: i8, command: u8) -> [u8; 6] {
    let [lo, hi] = arg.to_le_bytes();
    [kind, lo, hi, dx.to_le_bytes()[0], dy.to_le_bytes()[0], command]
}

#[test]
fn rom_anim_decodes_steps() {
    let anim = RomAnim::parse(&anim_file(&[
        record(0, 2, 0, 0, 0),
        record(0, u16::MAX, 0, 0, 1),
        record(0, 4660, -10, 17, u8::MAX),
        record(1, u16::from_le_bytes([9, 3]), -1, 0, u8::MAX),
        record(0, 5, 0, 0, 4),
        record(0, 3, 0, 0, 3),
        record(0, u16::MAX, 0, 0, 2),
        [u8::MAX; 6],
    ]))
    .unwrap();
    assert_eq!(anim.len(), 8);
    let steps: Vec<_> = anim.records().iter().map(Record::step).collect();
    assert_eq!(
        steps,
        [
            Step::Call { target: 2 },
            Step::End,
            Step::Frame(Sprite::Bitmap(4660)),
            Step::Frame(Sprite::Figure { job: 9, direction: 3 }),
            Step::Sound { id: 5 },
            Step::MapObject { value: 3 },
            Step::Return,
            Step::Frame(Sprite::Figure {
                job: u8::MAX,
                direction: u8::MAX
            }),
        ]
    );
    let frame = anim.get(2).unwrap();
    assert_eq!((frame.dx, frame.dy), (-10, 17));
    assert!(anim.get(7).unwrap().is_blank());
    assert!(!frame.is_blank());
}

#[test]
fn rom_anim_rejects_malformed_files() {
    let data = anim_file(&[[0; 6], [0; 6]]);
    for len in 0..data.len() {
        assert!(RomAnim::parse(&data[..len]).is_err());
    }
    let mut long = data.clone();
    long.push(0);
    assert_eq!(
        RomAnim::parse(&long).unwrap().quirks(),
        [Quirk::TrailingBytes { offset: 16, len: 1 }]
    );
    assert!(
        RomAnim::parse(&data).unwrap().quirks().is_empty(),
        "{:?}",
        RomAnim::parse(&data).unwrap().quirks()
    );
    assert!(matches!(
        RomAnim::parse(&data[2..]),
        Err(Error::Tag {
            expected: RomAnim::TAG,
            ..
        })
    ));
    assert!(RomAnim::parse(&anim_file(&[])).unwrap().is_empty());
}
