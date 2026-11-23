use super::bitmap::END_OF_ROW;
use super::*;

/// A player-bitmap command byte: `kind` in the top two bits, `count` in the low six.
const fn op(kind: u8, count: u8) -> u8 {
    kind << 6 | count
}

fn le16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn le32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Joins rows behind a `u16` offset table.
fn rows_with_table(rows: &[&[u8]]) -> Vec<u8> {
    let mut table = Vec::new();
    let mut body = Vec::new();
    let mut at = rows.len() * 2;
    for row in rows {
        le16(&mut table, u16::try_from(at).unwrap());
        body.extend_from_slice(row);
        at += row.len();
    }
    table.extend(body);
    table
}

/// The type 2/4/7 header and data.
fn bitmap_body(nx: i16, ny: i16, w: u16, h: u16, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&nx.to_le_bytes());
    out.extend_from_slice(&ny.to_le_bytes());
    le32(&mut out, 0);
    le16(&mut out, w);
    le16(&mut out, h);
    le16(&mut out, 1);
    le32(&mut out, u32::try_from(data.len()).unwrap());
    out.extend_from_slice(data);
    out
}

fn rle_bitmap() -> Vec<u8> {
    let mut data = rows_with_table(&[&[0, 1, 2, 7, 8, 1, END_OF_ROW], &[4, 1, 2, 3, 4, 0, END_OF_ROW]]);
    data.push(END_OF_ROW);
    bitmap_body(-2, 3, 4, 2, &data)
}

fn shadow() -> Vec<u8> {
    let mut data = rows_with_table(&[&[1, 2, 1, 0, END_OF_ROW], &[0, 4, END_OF_ROW]]);
    data.push(END_OF_ROW);
    bitmap_body(0, 0, 4, 2, &data)
}

fn player_bitmap() -> Vec<u8> {
    let data = rows_with_table(&[
        &[op(0, 1), op(1, 2), 9, 8, op(2, 1), 3],
        &[op(3, 2), 5, op(2, 2), 0],
    ]);
    bitmap_body(1, 1, 4, 2, &data)
}

fn raw_bitmap() -> Vec<u8> {
    let mut out = Vec::new();
    le16(&mut out, 1);
    le32(&mut out, 6);
    out.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    out.extend_from_slice(&5i16.to_le_bytes());
    out.extend_from_slice(&(-5i16).to_le_bytes());
    le16(&mut out, 3);
    le16(&mut out, 2);
    out.extend_from_slice(&[0, 0, 2, 1, 244, 6, 112, 0]);
    out
}

fn palette() -> Vec<u8> {
    let mut out = vec![0, 1];
    out.extend((0..=255u8).flat_map(|i| [i, 255 - i, 7]));
    out
}

fn sound(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    le32(&mut out, u32::try_from(bytes.len()).unwrap());
    out.extend_from_slice(bytes);
    out
}

fn xmidi() -> Vec<u8> {
    let mut out = b"FORM".to_vec();
    out.extend_from_slice(&[0, 0, 0, 14]);
    out.extend_from_slice(b"XDIRINFO");
    sound(&out)
}

fn font() -> Vec<u8> {
    let mut out = vec![6, 8];
    for slot in 0..GLYPH_COUNT {
        if slot == usize::from(b'A' - FIRST_GLYPH) {
            le16(&mut out, 4);
            out.extend(player_bitmap());
        } else {
            le16(&mut out, 0);
        }
    }
    out
}

fn archive(slots: &[Option<(u16, Vec<u8>)>]) -> Vec<u8> {
    let mut out = Vec::new();
    le16(&mut out, MAGIC);
    le32(&mut out, u32::try_from(slots.len()).unwrap());
    for slot in slots {
        match slot {
            None => le16(&mut out, 0),
            Some((kind, body)) => {
                le16(&mut out, 1);
                le16(&mut out, *kind);
                out.extend_from_slice(body);
            }
        }
    }
    out
}

fn every_type() -> Vec<u8> {
    archive(&[
        Some((5, palette())),
        None,
        Some((2, rle_bitmap())),
        Some((7, shadow())),
        Some((4, player_bitmap())),
        Some((14, raw_bitmap())),
        Some((1, sound(&[128, 127, 129]))),
        Some((1, xmidi())),
        Some((3, font())),
    ])
}

#[test]
fn decodes_every_item_type() {
    let archive = Archive::parse(&every_type()).unwrap();
    let kinds: Vec<_> = archive.iter().map(Item::kind).collect();
    assert_eq!(
        kinds,
        [
            Some(ItemType::Palette),
            None,
            Some(ItemType::Bitmap),
            Some(ItemType::Shadow),
            Some(ItemType::PlayerBitmap),
            Some(ItemType::RawBitmap),
            Some(ItemType::Sound),
            Some(ItemType::Sound),
            Some(ItemType::Font),
        ]
    );
    let Some(Item::Palette(palette)) = archive.get(0) else {
        panic!("not a palette")
    };
    assert_eq!(palette.colour(255), [255, 0, 7]);
    assert!(matches!(archive.get(6), Some(Item::Sound(Sound::Pcm(s))) if s == &[128, 127, 129]));
    assert!(matches!(archive.get(7), Some(Item::Sound(Sound::Xmidi(_)))));
}

#[test]
fn rle_bitmap_draws_then_skips() {
    let (Item::Bitmap(bitmap), used) = Item::parse(ItemType::Bitmap, &rle_bitmap()).unwrap() else {
        panic!("not a bitmap")
    };
    assert_eq!(used, rle_bitmap().len());
    assert_eq!(bitmap.origin(), Origin { x: -2, y: 3 });
    assert_eq!(bitmap.image().pixels(), [0, 7, 8, 0, 1, 2, 3, 4]);
    assert_eq!(
        bitmap.mask().bits(),
        [false, true, true, false, true, true, true, true]
    );
    let mut palette = crate::Palette::default();
    palette.0[7] = [10, 20, 30];
    assert_eq!(bitmap.to_rgba(&palette)[..8], [0, 0, 0, 0, 10, 20, 30, 255]);
}

#[test]
fn shadow_is_a_mask() {
    let (Item::Shadow(shadow), _) = Item::parse(ItemType::Shadow, &shadow()).unwrap() else {
        panic!("not a shadow")
    };
    assert_eq!(
        shadow.mask().bits(),
        [true, false, false, true, false, false, false, false]
    );
    assert_eq!(shadow.mask().count(), 2);
}

#[test]
fn player_bitmap_remaps_shades() {
    let (Item::PlayerBitmap(bitmap), _) = Item::parse(ItemType::PlayerBitmap, &player_bitmap()).unwrap()
    else {
        panic!("not a player bitmap")
    };
    let shade = |n| Pixel::Player(Shade::new(n).unwrap());
    assert_eq!(
        bitmap.pixels(),
        [
            Pixel::Transparent,
            Pixel::Colour(9),
            Pixel::Colour(8),
            shade(3),
            Pixel::Colour(5),
            Pixel::Colour(5),
            shade(0),
            shade(0),
        ]
    );
    let (image, mask) = bitmap.to_indexed(PlayerColour::from_base(40).unwrap());
    assert_eq!(image.pixels(), [0, 9, 8, 43, 5, 5, 40, 40]);
    assert_eq!(mask.count(), 7);
    let mut palette = crate::Palette::default();
    palette.0[131] = [1, 2, 3];
    assert_eq!(
        bitmap.to_rgba(&palette, PlayerColour::DEFAULT)[12..16],
        [1, 2, 3, 255]
    );
}

#[test]
fn raw_bitmap_keeps_trailing_origin() {
    let (Item::RawBitmap(raw), used) = Item::parse(ItemType::RawBitmap, &raw_bitmap()).unwrap() else {
        panic!("not a raw bitmap")
    };
    assert_eq!(used, raw_bitmap().len());
    assert_eq!(raw.origin(), Origin { x: 5, y: -5 });
    assert_eq!((raw.width(), raw.height()), (3, 2));
    assert_eq!(raw.image().get(2, 1), Some(6));
}

#[test]
fn font_file_maps_codes_to_glyphs() {
    let mut file = Vec::new();
    le16(&mut file, 3);
    file.extend(font());
    let font = Font::parse_file(&file).unwrap();
    assert_eq!((font.dx(), font.dy()), (6, 8));
    assert_eq!(font.glyph(b'A').map(PlayerBitmap::width), Some(4));
    assert!(font.glyph(b'B').is_none());
    assert!(font.glyph(b' ' - 1).is_none());
    assert_eq!(font.glyphs().map(|(code, _)| code).collect::<Vec<_>>(), [b'A']);
    let found = font.quirks();
    assert!(found.is_empty(), "{found:?}");

    let (tagged, used) = Item::parse_tagged(&file).unwrap();
    assert_eq!(used, file.len());
    assert_eq!(tagged, Item::Font(font));

    file.push(0);
    let padded = Font::parse_file(&file).unwrap();
    assert_eq!(
        padded.quirks(),
        [Quirk::TrailingBytes {
            offset: file.len() - 1,
            len: 1
        }]
    );
    assert_eq!(Font::parse_file(&[2, 0]), Err(Error::NotAFont(2)));
}

#[test]
fn font_glyphs_may_be_plain_bitmaps() {
    let mut file = Vec::new();
    le16(&mut file, 3);
    file.extend([6, 8]);
    for slot in 0..GLYPH_COUNT {
        match slot {
            0 => {
                le16(&mut file, 2);
                file.extend(rle_bitmap());
            }
            1 => {
                le16(&mut file, 4);
                let data = rows_with_table(&[&[op(1, 5), 1, 2, 3, 4, 5]]);
                file.extend(bitmap_body(0, 0, 4, 1, &data));
            }
            _ => le16(&mut file, 0),
        }
    }
    let font = Font::parse_file(&file).unwrap();
    let space = font.glyph(b' ').unwrap();
    assert_eq!(space.get(0, 0), Some(Pixel::Transparent));
    assert_eq!(space.get(1, 0), Some(Pixel::Colour(7)));
    assert_eq!(space.origin(), Origin { x: -2, y: 3 });
    assert_eq!(
        font.quirks(),
        [Quirk::Glyph {
            code: b'!',
            quirk: Box::new(Quirk::RowOverrun {
                row: 0,
                decoded: 5,
                width: 4
            })
        }]
    );

    let bad = [3, 0, 0, 0, 7, 0];
    assert_eq!(Font::parse_file(&bad), Err(Error::GlyphType(7)));
}

#[test]
fn rejects_unknown_magic_and_types() {
    assert_eq!(Archive::parse(&[0, 0, 0, 0, 0, 0]), Err(Error::Magic(0)));
    let unknown = archive(&[None, Some((6, vec![]))]);
    assert_eq!(
        Archive::parse(&unknown),
        Err(Error::Item {
            index: 1,
            source: Box::new(Error::UnknownType(6))
        })
    );
}

#[test]
fn shipped_style_archives_have_no_quirks() {
    let archive = Archive::parse(&every_type()).unwrap();
    let found = archive.quirks();
    assert!(found.is_empty(), "{found:?}");
    let found = archive.trailing();
    assert!(found.is_empty(), "{found:?}");
    assert!(archive.iter().all(|item| item.quirks().is_empty()));
}

#[test]
fn trailing_bytes_are_kept() {
    let mut data = every_type();
    let end = data.len();
    data.extend([0, 0, 0]);
    let archive = Archive::parse(&data).unwrap();
    assert_eq!(archive.len(), 9);
    assert_eq!(archive.trailing(), [0, 0, 0]);
    assert_eq!(archive.quirks(), [Quirk::TrailingBytes { offset: end, len: 3 }]);
}

#[test]
fn odd_slot_flags_read_as_empty() {
    let mut data = archive(&[None, None, Some((1, sound(&[5])))]);
    data[8] = 2;
    let archive = Archive::parse(&data).unwrap();
    assert_eq!(archive.get(1), Some(&Item::Empty));
    assert!(matches!(archive.get(2), Some(Item::Sound(Sound::Pcm(s))) if s == &[5]));
    assert_eq!(
        archive.quirks(),
        [Quirk::SlotFlag {
            slot: 1,
            offset: 8,
            flag: 2
        }]
    );
}

#[test]
fn draw_skip_rows_are_clipped_or_filled() {
    let long = rows_with_table(&[&[2, 1, 1, 3, 3, 9, 9, 9, 0, END_OF_ROW], &[0, 9, END_OF_ROW]]);
    let bitmap = Bitmap::decode(Origin::default(), 4, 2, &long).unwrap();
    assert_eq!(&bitmap.image().pixels()[..4], [1, 1, 0, 0]);
    assert_eq!(&bitmap.mask().bits()[..4], [true, true, false, false]);
    assert_eq!(
        bitmap.quirks(),
        [
            Quirk::RowOverrun {
                row: 0,
                decoded: 8,
                width: 4
            },
            Quirk::RowOverrun {
                row: 1,
                decoded: 9,
                width: 4
            },
        ]
    );

    let short = rows_with_table(&[&[2, 1, 1, 0, END_OF_ROW]]);
    let bitmap = Bitmap::decode(Origin::default(), 4, 1, &short).unwrap();
    assert_eq!(bitmap.mask().bits(), [true, true, false, false]);
    assert_eq!(
        bitmap.quirks(),
        [Quirk::RowShort {
            row: 0,
            decoded: 2,
            width: 4
        }]
    );

    let shadow = Shadow::decode(Origin::default(), 2, 1, &rows_with_table(&[&[3, 0, END_OF_ROW]])).unwrap();
    assert_eq!(shadow.mask().bits(), [true, true]);
    assert_eq!(shadow.quirks().len(), 1);
}

#[test]
fn player_rows_are_clipped() {
    let long = rows_with_table(&[&[op(0, 2), op(1, 3), 7, 8, 9]]);
    let bitmap = PlayerBitmap::decode(Origin::default(), 4, 1, &long).unwrap();
    assert_eq!(
        bitmap.pixels(),
        [
            Pixel::Transparent,
            Pixel::Transparent,
            Pixel::Colour(7),
            Pixel::Colour(8)
        ]
    );
    assert_eq!(
        bitmap.quirks(),
        [Quirk::RowOverrun {
            row: 0,
            decoded: 5,
            width: 4
        }]
    );
}

#[test]
fn out_of_range_shades_are_kept() {
    let data = rows_with_table(&[&[op(2, 2), 4, op(2, 2), 0]]);
    let bitmap = PlayerBitmap::decode(Origin::default(), 4, 1, &data).unwrap();
    let odd = Shade::from_byte(4);
    assert!(!odd.is_valid());
    assert_eq!(bitmap.get(0, 0), Some(Pixel::Player(odd)));
    assert_eq!(
        bitmap.quirks(),
        [Quirk::Shade {
            row: 0,
            x: 0,
            shade: 4
        }]
    );
    assert_eq!(PlayerColour::DEFAULT.index(odd), 132);
    assert_eq!(
        PlayerColour::from_base(252).unwrap().index(Shade::from_byte(5)),
        1
    );
    let (image, _) = bitmap.to_indexed(PlayerColour::DEFAULT);
    assert_eq!(image.pixels(), [132, 132, 128, 128]);
}

#[test]
fn bitmaps_without_data_are_transparent() {
    for kind in [2, 4, 7] {
        let body = bitmap_body(1, 2, 3, 2, &[]);
        let (item, used) = Item::parse(ItemType::from_id(kind).unwrap(), &body).unwrap();
        assert_eq!(used, body.len());
        assert_eq!(item.quirks(), [Quirk::NoData { width: 3, height: 2 }]);
        match item {
            Item::Bitmap(b) => assert_eq!(b.mask().count(), 0),
            Item::Shadow(s) => assert_eq!((s.width(), s.mask().count()), (3, 0)),
            Item::PlayerBitmap(p) => {
                assert!(p.pixels().iter().all(|&p| p == Pixel::Transparent));
                assert_eq!(p.pixels().len(), 6);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    let flat = bitmap_body(0, 0, 5, 0, &[]);
    assert!(
        Item::parse(ItemType::Bitmap, &flat)
            .unwrap()
            .0
            .quirks()
            .is_empty(),
        "{:?}",
        Item::parse(ItemType::Bitmap, &flat).unwrap().0.quirks()
    );

    let data = archive(&[Some((2, bitmap_body(0, 0, 4, 1, &[])))]);
    assert_eq!(
        Archive::parse(&data).unwrap().quirks(),
        [Quirk::Item {
            slot: 0,
            quirk: Box::new(Quirk::NoData { width: 4, height: 1 })
        }]
    );
}

#[test]
fn bitmaps_without_data_share_the_input_budget() {
    let huge = bitmap_body(0, 0, u16::MAX, u16::MAX, &[]);
    assert!(matches!(
        Item::parse(ItemType::Shadow, &huge),
        Err(Error::TooLarge { .. })
    ));
    let slots: Vec<_> = (0..64)
        .map(|_| Some((7, bitmap_body(0, 0, 100, 100, &[]))))
        .collect();
    let data = archive(&slots);
    assert!(data.len() * MAX_EXPANSION < 64 * 100 * 100);
    assert!(matches!(
        Archive::parse(&data),
        Err(Error::Item { source, .. }) if matches!(*source, Error::TooLarge { .. })
    ));
}

#[test]
fn refuses_sizes_the_input_cannot_back() {
    let data = rows_with_table(&[&[0, 255, END_OF_ROW]]);
    assert!(matches!(
        Shadow::decode(Origin::default(), u16::MAX, u16::MAX, &data),
        Err(Error::TooLarge { .. } | Error::Truncated(_))
    ));
    assert!(matches!(
        PlayerBitmap::decode_rows(
            Origin::default(),
            u16::MAX,
            u16::MAX,
            &[op(0, 63)],
            std::iter::repeat(0)
        ),
        Err(Error::TooLarge { .. })
    ));
}

#[test]
fn rows_cannot_reread_shared_data_without_bound() {
    const HEIGHT: u16 = 300;
    let mut data = vec![0; usize::from(HEIGHT) * 2];
    data.push(END_OF_ROW);
    let over = Error::RowReads {
        row: u16::try_from(MAX_EXPANSION).unwrap(),
        input: data.len(),
    };
    assert_eq!(
        Bitmap::decode(Origin::default(), 1, HEIGHT, &data),
        Err(over.clone())
    );
    assert_eq!(
        Shadow::decode(Origin::default(), 1, HEIGHT, &data),
        Err(over.clone())
    );
    *data.last_mut().unwrap() = op(0, 1);
    assert_eq!(
        PlayerBitmap::decode(Origin::default(), 1, HEIGHT, &data),
        Err(over)
    );
    let shared = PlayerBitmap::decode_rows(Origin::default(), 1, 256, &data, std::iter::repeat(0));
    assert!(shared.is_ok());
}

#[test]
fn decode_rows_takes_explicit_starts() {
    let data = [op(0, 2), op(1, 1), 7, op(3, 1), 3];
    let bitmap = PlayerBitmap::decode_rows(Origin::default(), 2, 2, &data, [1, 0]).unwrap();
    assert_eq!(
        bitmap.pixels(),
        [
            Pixel::Colour(7),
            Pixel::Colour(3),
            Pixel::Transparent,
            Pixel::Transparent
        ]
    );
    assert_eq!(
        PlayerBitmap::decode_rows(Origin::default(), 2, 2, &data, [0]),
        Err(Error::MissingRow { row: 1 })
    );
    assert_eq!(
        PlayerBitmap::decode_rows(Origin::default(), 2, 1, &data, [9]),
        Err(Error::RowStart { row: 0, offset: 9 })
    );
}

#[test]
fn never_panics_on_damaged_input() {
    let good = every_type();
    for len in 0..good.len() {
        assert!(Archive::parse(&good[..len]).is_err());
    }
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..8000 {
        let mut bytes = good.clone();
        for _ in 0..4 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let at = usize::try_from(state % u64::try_from(bytes.len()).unwrap()).unwrap();
            bytes[at] = state.to_le_bytes()[7];
        }
        let _ = Archive::parse(&bytes);
        let _ = Item::parse_tagged(&bytes[6..]);
    }
}
