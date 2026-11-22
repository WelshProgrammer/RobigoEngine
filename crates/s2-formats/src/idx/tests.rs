use super::*;
use crate::bytes::Truncated;

fn sound(bytes: &[u8]) -> Vec<u8> {
    let mut out = 1u16.to_le_bytes().to_vec();
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

fn palette() -> Vec<u8> {
    let mut out = vec![5, 0, 0, 1];
    out.extend((0..=255u8).flat_map(|i| [i, 255 - i, 7]));
    out
}

fn raw_bitmap() -> Vec<u8> {
    let mut out = vec![14, 0, 1, 0];
    out.extend_from_slice(&6u32.to_le_bytes());
    out.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    for field in [2_i16, -2, 3, 2] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0, 2, 1, 244, 6, 112, 0]);
    out
}

fn entry(name: &[u8], offset: u32, kind: u16) -> Vec<u8> {
    let mut out = [0; NAME_SIZE].to_vec();
    out[..name.len()].copy_from_slice(name);
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&[0; RESERVED_SIZE]);
    out.extend_from_slice(&kind.to_le_bytes());
    out
}

fn index(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut out = u32::try_from(entries.len()).unwrap().to_le_bytes().to_vec();
    out.extend(entries.iter().flatten());
    out
}

/// Packs `(name, tagged item)` pairs back to back, the way the shipped files are laid out.
fn pair(items: &[(&[u8], Vec<u8>)]) -> (Vec<u8>, Vec<u8>) {
    let mut dat = Vec::new();
    let mut entries = Vec::new();
    for (name, item) in items {
        let kind = u16::from_le_bytes([item[0], item[1]]);
        entries.push(entry(name, u32::try_from(dat.len()).unwrap(), kind));
        dat.extend_from_slice(item);
    }
    (index(&entries), dat)
}

fn sample() -> (Vec<u8>, Vec<u8>) {
    pair(&[
        (b"font09", sound(&[128, 129])),
        (b"roemerpal", palette()),
        (b"iobob_02", raw_bitmap()),
    ])
}

#[test]
fn decodes_entries_in_index_order() {
    let (idx, dat) = sample();
    let index = Index::parse(&idx, &dat).unwrap();
    let names: Vec<_> = index.iter().map(Entry::name).collect();
    assert_eq!(names, [Some("font09"), Some("roemerpal"), Some("iobob_02")]);
    let kinds: Vec<_> = index.iter().map(Entry::kind).collect();
    assert_eq!(
        kinds,
        [
            Some(ItemType::Sound),
            Some(ItemType::Palette),
            Some(ItemType::RawBitmap)
        ]
    );
    let spans: Vec<_> = index.iter().map(|e| (e.offset(), e.size())).collect();
    assert_eq!(spans, [(0, 8), (8, 772), (780, 30)]);
    assert_eq!(index.iter().map(Entry::size).sum::<usize>(), dat.len());
    let found = index.quirks();
    assert!(found.is_empty(), "{found:?}");
    assert!(!index.iter().any(Entry::is_skipped));
    assert_eq!(index.entry(1).map(Entry::indexed_kind), Some(5));
}

#[test]
fn looks_up_by_name_and_position() {
    let (idx, dat) = sample();
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(index.get("ROEMERPAL").map(Entry::offset), Some(8));
    assert_eq!(
        index.get(b"iobob_02").map(Entry::kind),
        Some(Some(ItemType::RawBitmap))
    );
    assert_eq!(index.position("Font09"), Some(0));
    assert!(index.get("roemer").is_none());
    assert!(index.get("roemerpal\0").is_none());
    assert_eq!(index.entry(1), index.get("roemerpal"));
    assert!(index.entry(3).is_none());
    assert_eq!(index.len(), 3);
}

#[test]
fn first_duplicate_name_wins() {
    let (idx, dat) = pair(&[(b"a", sound(&[1])), (b"A", sound(&[2]))]);
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(index.position("a"), Some(0));
    assert_eq!(index.get("A").map(Entry::offset), Some(0));
}

#[test]
fn keeps_raw_names_and_reserved_bytes() {
    let mut idx = index(&[entry(b"exactly16bytes!!", 0, 1), entry(b"caf\x82", 7, 1)]);
    let reserved = [252, 62, 123, 0, 1, 0];
    idx[4 + 20..4 + 26].copy_from_slice(&reserved);
    let dat = [sound(&[9]), sound(&[]), vec![170]].concat();
    let index = Index::parse(&idx, &dat).unwrap();
    let first = index.entry(0).unwrap();
    assert_eq!(first.name(), Some("exactly16bytes!!"));
    assert_eq!(first.reserved(), reserved);
    let second = index.entry(1).unwrap();
    assert_eq!(second.name_bytes(), b"caf\x82");
    assert_eq!(second.name(), None);
    assert!(second.is_named(b"CAF\x82"));
    assert_eq!(second.reserved(), [0; RESERVED_SIZE]);
}

#[test]
fn items_may_be_out_of_order_with_gaps() {
    let dat = [vec![238; 3], sound(&[1, 2]), vec![238], palette()].concat();
    let idx = index(&[entry(b"pal", 12, 5), entry(b"snd", 3, 1)]);
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(index.get("pal").map(Entry::size), Some(772));
    assert_eq!(index.get("snd").map(Entry::size), Some(8));
}

#[test]
fn empty_items_and_empty_indexes_decode() {
    let index = Index::parse(&index(&[entry(b"none", 0, 0)]), &[0, 0]).unwrap();
    assert_eq!(index.entry(0).map(Entry::item), Some(&Item::Empty));
    assert_eq!(index.entry(0).map(Entry::size), Some(2));
    assert!(Index::parse(&0u32.to_le_bytes(), &[]).unwrap().is_empty());
}

#[test]
fn index_length_must_match_count() {
    assert_eq!(
        Index::parse(&[1, 0], &[]),
        Err(Error::Truncated(Truncated { offset: 0, needed: 4 }))
    );
    let (mut idx, dat) = sample();
    idx[0] = 4;
    assert_eq!(
        Index::parse(&idx, &dat),
        Err(Error::Truncated(Truncated {
            offset: HEADER_SIZE,
            needed: 4 * ENTRY_SIZE
        }))
    );
    idx[0] = 2;
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(index.len(), 2);
    assert_eq!(
        index.quirks(),
        [Quirk::TrailingBytes {
            offset: HEADER_SIZE + 2 * ENTRY_SIZE,
            len: ENTRY_SIZE
        }]
    );
}

#[test]
fn huge_count_fails_before_allocating() {
    assert!(matches!(
        Index::parse(&u32::MAX.to_le_bytes(), &[]),
        Err(Error::Truncated(_))
    ));
}

#[test]
fn offset_past_dat_is_an_error() {
    let idx = index(&[entry(b"far", 9, 1)]);
    assert_eq!(
        Index::parse(&idx, &[0; 8]),
        Err(Error::Offset {
            index: 0,
            offset: 9,
            len: 8
        })
    );
    let at_end = Index::parse(&index(&[entry(b"end", 8, 1)]), &[0; 8]);
    assert!(matches!(
        at_end,
        Err(Error::Item {
            index: 0,
            source: lst::Error::Truncated(_)
        })
    ));
}

#[test]
fn type_mismatch_skips_the_entry() {
    let (mut idx, dat) = sample();
    idx[4 + ENTRY_SIZE + 26] = 2;
    let index = Index::parse(&idx, &dat).unwrap();
    let skipped = index.get("roemerpal").unwrap();
    assert!(skipped.is_skipped());
    assert_eq!(skipped.item(), &Item::Empty);
    assert_eq!((skipped.size(), skipped.indexed_kind()), (0, 2));
    assert_eq!(
        index.get("iobob_02").and_then(Entry::kind),
        Some(ItemType::RawBitmap)
    );
    assert_eq!(
        index.quirks(),
        [Quirk::TypeMismatch {
            index: 1,
            indexed: 2,
            stored: 5
        }]
    );
}

#[test]
fn unknown_tag_is_an_item_error() {
    let idx = index(&[entry(b"odd", 0, 9)]);
    assert_eq!(
        Index::parse(&idx, &[9, 0, 0, 0]),
        Err(Error::Item {
            index: 0,
            source: lst::Error::UnknownType(9)
        })
    );
}

#[test]
fn shared_offsets_share_one_item() {
    let idx = index(&[entry(b"a", 0, 1), entry(b"b", 8, 1), entry(b"c", 0, 1)]);
    let dat = [sound(&[1, 2]), sound(&[3])].concat();
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(index.get("c").map(Entry::item), index.get("a").map(Entry::item));
    assert_eq!(index.get("c").map(Entry::size), Some(8));
    assert!(std::ptr::eq(
        index.entry(0).unwrap().item(),
        index.entry(2).unwrap().item()
    ));
    assert_eq!(
        index.quirks(),
        [Quirk::SharedOffset {
            first: 0,
            second: 2,
            offset: 0
        }]
    );
    let item = index.into_entries().swap_remove(2).into_item();
    assert!(matches!(item, Item::Sound(_)));
}

#[test]
fn overlapping_items_are_reported() {
    let dat = sound(&[0, 0, 0, 0, 0, 0]);
    let idx = index(&[entry(b"outer", 0, 1), entry(b"inner", 6, 0)]);
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(index.get("inner").map(Entry::item), Some(&Item::Empty));
    assert_eq!(
        index.quirks(),
        [Quirk::Overlap {
            index: 0,
            end: 12,
            next: 6
        }]
    );
}

/// `n` sound items at every sixth byte, each running to the end of the DAT.
fn nested_sounds(n: u32) -> (Vec<u8>, Vec<u8>) {
    let total = 6 * n;
    let mut dat = Vec::new();
    for k in 0..n {
        dat.extend([1, 0]);
        dat.extend((total - 6 * k - 6).to_le_bytes());
    }
    let entries: Vec<_> = (0..n).map(|k| entry(b"n", 6 * k, 1)).collect();
    (index(&entries), dat)
}

#[test]
fn overlaps_stay_bounded() {
    let (idx, dat) = nested_sounds(2);
    let index = Index::parse(&idx, &dat).unwrap();
    assert_eq!(
        index.quirks(),
        [Quirk::Overlap {
            index: 0,
            end: 12,
            next: 6
        }]
    );

    let (idx, dat) = nested_sounds(20);
    assert!(matches!(
        Index::parse(&idx, &dat),
        Err(Error::Coverage { len: 120, covered, .. }) if covered > 4 * 120
    ));
}

#[test]
fn item_quirks_carry_the_entry() {
    let mut body = vec![2, 0];
    body.extend([0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 1, 0, 0, 0, 0, 0]);
    let idx = index(&[entry(b"blank", 0, 2)]);
    let index = Index::parse(&idx, &body).unwrap();
    assert_eq!(
        index.quirks(),
        [Quirk::Item {
            index: 0,
            quirk: lst::Quirk::NoData { width: 2, height: 1 }
        }]
    );
}

#[test]
fn never_panics_on_truncated_or_corrupt_input() {
    let (idx, dat) = sample();
    for len in 0..=idx.len() {
        let _ = Index::parse(&idx[..len], &dat);
    }
    for len in 0..=dat.len() {
        let _ = Index::parse(&idx, &dat[..len]);
    }
    for at in 0..idx.len() {
        let mut bad = idx.clone();
        bad[at] = !bad[at];
        let _ = Index::parse(&bad, &dat);
    }
}
