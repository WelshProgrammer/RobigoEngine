use super::container::{LAYER_CLASS, MAP_CLASS};
use super::header::NO_HEADQUARTERS;
use super::*;

const W: u16 = 4;
const H: u16 = 3;
const NODES: usize = (W as usize) * (H as usize);
/// File offset of the layer container in a `.WLD` file.
const CONTAINER_AT: usize = SIGNATURE.len() + HEADER_LEN + AREA_TABLE_LEN;

fn layer_bytes(slot: u8) -> Vec<u8> {
    (0..NODES).map(|i| slot * 16 + u8::try_from(i).unwrap()).collect()
}

fn tagged_layer(width: u16, height: u16, data: &[u8]) -> Vec<u8> {
    let mut out = LAYER_CLASS.to_le_bytes().to_vec();
    out.extend([0; 4]);
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out.extend(1_u16.to_le_bytes());
    out.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
    out.extend(data);
    out
}

fn tagged(slots: &[u8]) -> Vec<u8> {
    let mut out = MAP_CLASS.to_le_bytes().to_vec();
    out.extend([0; 4]);
    out.extend(W.to_le_bytes());
    out.extend(H.to_le_bytes());
    for &slot in slots {
        out.extend(tagged_layer(W, H, &layer_bytes(slot)));
    }
    out
}

fn compact(payloads: &[Vec<u8>]) -> Vec<u8> {
    let mut out = MAP_CLASS.to_le_bytes().to_vec();
    out.extend(u32::MAX.to_le_bytes());
    out.extend([0; 4]);
    out.extend(W.to_le_bytes());
    out.extend(H.to_le_bytes());
    for payload in payloads {
        out.extend(u32::try_from(payload.len()).unwrap().to_le_bytes());
        out.extend(payload);
    }
    out
}

fn v151_slots() -> Vec<u8> {
    LayerKind::STORED.iter().map(|k| k.slot()).collect()
}

fn area_table() -> Vec<u8> {
    let mut out = vec![0; AREA_TABLE_LEN];
    out[..9].copy_from_slice(&[1, 2, 0, 1, 0, 9, 0, 0, 0]);
    out[9..18].copy_from_slice(&[2, 3, 0, 2, 0, 3, 0, 0, 0]);
    out[18..27].copy_from_slice(&[0, 7, 0, 7, 0, 7, 0, 0, 0]);
    out
}

fn header(title: &[u8], stored: (u16, u16), terrain: u8) -> Vec<u8> {
    let mut out = vec![0; HEADER_LEN];
    out[..title.len()].copy_from_slice(title);
    if title.len() < 20 {
        out[20..22].copy_from_slice(&stored.0.to_le_bytes());
        out[22..24].copy_from_slice(&stored.1.to_le_bytes());
    }
    out[24] = terrain;
    out[25] = 2;
    out[26..33].copy_from_slice(b"Someone");
    let xs = [
        1,
        3,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
    ];
    let ys = [
        2,
        1,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
        NO_HEADQUARTERS,
    ];
    for (i, (x, y)) in xs.into_iter().zip(ys).enumerate() {
        out[46 + 2 * i..48 + 2 * i].copy_from_slice(&x.to_le_bytes());
        out[60 + 2 * i..62 + 2 * i].copy_from_slice(&y.to_le_bytes());
    }
    out[75..82].copy_from_slice(&[0, 4, 0, 0, 0, 0, 0]);
    out
}

fn wld_with(header: &[u8], container: &[u8], tail: &[u8]) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    out.extend(header);
    out.extend(area_table());
    out.extend(container);
    out.extend(tail);
    out
}

fn wld() -> Vec<u8> {
    let animals = [3, 1, 0, 2, 0, 5, 3, 0, 0, 0, ANIMALS_END];
    wld_with(
        &header(b"Short title", (W, H), 2),
        &tagged(&v151_slots()),
        &animals,
    )
}

#[test]
fn decodes_a_wld() {
    let map = Map::parse_wld(&wld()).unwrap();
    assert_eq!((map.width(), map.height()), (W, H));
    assert_eq!(map.layers().len(), 14);
    assert!(map.layer(LayerKind::Runtime13).is_none());
    assert_eq!(map.layer(LayerKind::Areas).unwrap().data(), layer_bytes(14));
    assert_eq!(map.get(LayerKind::Heights, 1, 2), Some(9));
    assert_eq!(map.get(LayerKind::Heights, W, 0), None);
    assert_eq!(map.trailer(), &[] as &[u8]);

    let header = map.header().unwrap();
    assert_eq!(header.title(), "Short title");
    assert_eq!(header.author(), "Someone");
    assert!(!header.dimensions_overwritten());
    assert_eq!(header.terrain(), Terrain::Winter);
    assert_eq!(header.player_count(), 2);
    assert_eq!(header.headquarters()[1], Some(Position { x: 3, y: 1 }));
    assert_eq!(header.headquarters()[2], None);
    assert_eq!(header.leaders()[1], 4);

    assert_eq!(map.areas().len(), AREA_COUNT);
    assert_eq!(map.areas()[0].kind, AreaKind::Land);
    assert_eq!(map.areas()[1].kind, AreaKind::Water);
    assert_eq!(map.areas()[1].size, 3);
    assert_eq!(map.areas()[2].kind, AreaKind::Unused);
    assert_eq!(map.areas()[2].seed, Position { x: 7, y: 7 });

    assert_eq!(
        map.animals(),
        &[
            Animal {
                species: 3,
                position: Position { x: 1, y: 2 }
            },
            Animal {
                species: 5,
                position: Position { x: 3, y: 0 }
            }
        ]
    );
}

#[test]
fn long_titles_spill_into_the_dimensions() {
    let title = b"Twenty-two chars title";
    assert_eq!(title.len(), 22);
    let map = Map::parse_wld(&wld_with(
        &header(title, (0, 0), 0),
        &tagged(&v151_slots()),
        &[ANIMALS_END],
    ))
    .unwrap();
    let header = map.header().unwrap();
    assert!(header.dimensions_overwritten());
    assert_eq!(header.title_bytes(), title);
    assert_eq!(header.stored_dimensions(), (u16::from_le_bytes(*b"le"), 0));
    assert_eq!((map.width(), map.height()), (W, H));
}

#[test]
fn a_full_length_title_stops_at_intact_dimensions() {
    let mut raw = header(b"", (W, H), 0);
    raw[..20].copy_from_slice(b"Exactly twenty chars");
    let map = Map::parse_wld(&wld_with(&raw, &tagged(&v151_slots()), &[ANIMALS_END])).unwrap();
    let header = map.header().unwrap();
    assert!(!header.dimensions_overwritten());
    assert_eq!(header.title(), "Exactly twenty chars");
}

#[test]
fn rejects_bad_signature() {
    let mut data = wld();
    data[0] = b'X';
    assert_eq!(Map::parse_wld(&data), Err(Error::BadSignature));
}

#[test]
fn unknown_terrain_and_header_byte_are_kept() {
    let mut raw = header(b"t", (W, H), 3);
    raw[84 - SIGNATURE.len()] = 1;
    let map = Map::parse_wld(&wld_with(&raw, &tagged(&v151_slots()), &[ANIMALS_END])).unwrap();
    let header = map.header().unwrap();
    assert_eq!(header.terrain(), Terrain::Other(3));
    assert_eq!(header.terrain().byte(), 3);
    assert_eq!(header.unknown_54(), 1);
    assert_eq!(map.quirks(), [Quirk::Terrain(3), Quirk::Unknown54(1)]);
}

#[test]
fn shipped_style_maps_have_no_quirks() {
    assert!(
        Map::parse_wld(&wld()).unwrap().quirks().is_empty(),
        "{:?}",
        Map::parse_wld(&wld()).unwrap().quirks()
    );
    let all: Vec<u8> = (0..15).collect();
    assert!(
        Map::parse_v102(&tagged(&all), &area_table())
            .unwrap()
            .quirks()
            .is_empty(),
        "{:?}",
        Map::parse_v102(&tagged(&all), &area_table()).unwrap().quirks()
    );
}

#[test]
fn bytes_after_the_animals_are_a_trailer() {
    let data = wld_with(
        &header(b"t", (W, H), 0),
        &tagged(&v151_slots()),
        &[ANIMALS_END, 7, 7],
    );
    let map = Map::parse_wld(&data).unwrap();
    assert_eq!(map.trailer(), [7, 7]);
    assert_eq!(
        map.quirks(),
        [Quirk::Trailer {
            offset: data.len() - 2,
            len: 2
        }]
    );
}

/// A layer header whose buffer is the null object, with junk dimensions.
fn absent_layer(len: u32) -> Vec<u8> {
    let mut out = LAYER_CLASS.to_le_bytes().to_vec();
    out.extend([0; 4]);
    out.extend([u8::MAX; 4]);
    out.extend(0_u16.to_le_bytes());
    out.extend(len.to_le_bytes());
    out
}

#[test]
fn null_buffers_are_absent_layers() {
    let slots = v151_slots();
    let mut container = tagged(&slots[..3]);
    let offset = CONTAINER_AT + container.len();
    container.extend(absent_layer(0));
    container.extend(tagged(&slots[4..])[10..].iter());
    let map = Map::parse_wld(&wld_with(&header(b"t", (W, H), 0), &container, &[ANIMALS_END])).unwrap();
    assert_eq!(map.layers().len(), 13);
    assert!(map.layer(LayerKind::Roads).is_none());
    assert_eq!(map.get(LayerKind::Roads, 0, 0), None);
    assert_eq!(map.layer(LayerKind::Areas).unwrap().data(), layer_bytes(14));
    assert_eq!(
        map.quirks(),
        [Quirk::AbsentLayer {
            layer: LayerKind::Roads,
            offset
        }]
    );

    let mut container = tagged(&[]);
    container.extend(absent_layer(4));
    let data = wld_with(&header(b"t", (W, H), 0), &container, &[]);
    assert_eq!(
        Map::parse_wld(&data),
        Err(Error::LayerLength {
            layer: 0,
            len: 4,
            expected: 0
        })
    );
}

#[test]
fn extra_word_before_the_container_is_skipped() {
    let mut container = vec![0, 0];
    container.extend(tagged(&v151_slots()));
    let map = Map::parse_wld(&wld_with(&header(b"t", (W, H), 0), &container, &[ANIMALS_END])).unwrap();
    assert_eq!(map.layer(LayerKind::Heights).unwrap().data(), layer_bytes(0));
    assert_eq!(map.quirks(), [Quirk::ExtraWord { offset: CONTAINER_AT }]);
}

#[test]
fn das_dreieck_layout_recovers_the_first_layer() {
    let slots = v151_slots();
    let mut container = vec![0, 0];
    container.extend(tagged(&slots[..1]));
    container.truncate(container.len() - 2);
    container.extend(tagged(&slots[1..])[10..].iter());
    let map = Map::parse_wld(&wld_with(&header(b"t", (W, H), 0), &container, &[ANIMALS_END])).unwrap();
    let mut heights = layer_bytes(0);
    heights[NODES - 2..].fill(0);
    assert_eq!(map.layer(LayerKind::Heights).unwrap().data(), heights);
    assert_eq!(map.layer(LayerKind::Terrain1).unwrap().data(), layer_bytes(1));
    assert_eq!(map.layer(LayerKind::Areas).unwrap().data(), layer_bytes(14));
    assert_eq!(
        map.quirks(),
        [
            Quirk::ExtraWord { offset: CONTAINER_AT },
            Quirk::ShortLayer {
                layer: LayerKind::Heights,
                missing: 2
            }
        ]
    );

    let mut without = tagged(&slots[..1]);
    without.truncate(without.len() - 2);
    without.extend(tagged(&slots[1..])[10..].iter());
    assert!(Map::parse_wld(&wld_with(&header(b"t", (W, H), 0), &without, &[ANIMALS_END])).is_err());
}

#[test]
fn layers_must_agree_on_size() {
    let mut container = tagged(&v151_slots()[..13]);
    container.extend(tagged_layer(W, H + 1, &[0; NODES + 4]));
    let data = wld_with(&header(b"t", (W, H), 0), &container, &[]);
    assert_eq!(
        Map::parse_wld(&data),
        Err(Error::LayerSize {
            layer: 13,
            width: W,
            height: H + 1,
            map_width: W,
            map_height: H
        })
    );

    let mut container = tagged(&[]);
    container.extend(tagged_layer(W, H, &[0; NODES - 1]));
    let data = wld_with(&header(b"t", (W, H), 0), &container, &[]);
    assert_eq!(
        Map::parse_wld(&data),
        Err(Error::LayerLength {
            layer: 0,
            len: NODES - 1,
            expected: NODES
        })
    );
}

#[test]
fn rejects_wrong_class_tags_and_empty_maps() {
    let mut container = tagged(&v151_slots());
    container[0] = 16;
    let data = wld_with(&header(b"t", (W, H), 0), &container, &[]);
    assert_eq!(
        Map::parse_wld(&data),
        Err(Error::UnexpectedClass {
            offset: CONTAINER_AT,
            expected: MAP_CLASS,
            found: LAYER_CLASS
        })
    );

    let mut container = tagged(&[]);
    container[6..8].copy_from_slice(&[0, 0]);
    let data = wld_with(&header(b"t", (W, H), 0), &container, &[]);
    assert_eq!(Map::parse_wld(&data), Err(Error::Empty { width: 0, height: H }));
}

#[test]
fn compact_containers_mix_raw_and_rle_layers() {
    let payloads: Vec<Vec<u8>> = LayerKind::STORED
        .iter()
        .map(|kind| match kind {
            LayerKind::Heights => layer_bytes(0),
            other => rle::encode(&[other.slot(); NODES]),
        })
        .collect();
    let data = wld_with(&header(b"t", (W, H), 1), &compact(&payloads), &[ANIMALS_END]);
    let map = Map::parse_wld(&data).unwrap();
    assert_eq!(map.layer(LayerKind::Heights).unwrap().data(), layer_bytes(0));
    assert_eq!(map.layer(LayerKind::Areas).unwrap().data(), [14; NODES]);
    assert_eq!(map.header().unwrap().terrain(), Terrain::Wasteland);

    let mut bad = payloads;
    bad[3] = vec![9, 9, 200, 1];
    let data = wld_with(&header(b"t", (W, H), 0), &compact(&bad), &[]);
    assert_eq!(
        Map::parse_wld(&data),
        Err(Error::Rle {
            layer: 3,
            source: RleError::Overrun { expected: NODES }
        })
    );
}

#[test]
fn decodes_v102_pairs_with_and_without_slot_13() {
    let all: Vec<u8> = (0..15).collect();
    let map = Map::parse_v102(&tagged(&all), &area_table()).unwrap();
    assert!(map.header().is_none());
    assert_eq!(map.layers().len(), 15);
    assert_eq!(map.layer(LayerKind::Runtime13).unwrap().data(), layer_bytes(13));
    assert_eq!(map.layer(LayerKind::Areas).unwrap().data(), layer_bytes(14));
    assert_eq!(map.areas()[1].kind, AreaKind::Water);
    let found = map.animals();
    assert!(found.is_empty(), "{found:?}");

    let map = Map::parse_v102(&tagged(&v151_slots()), &area_table()).unwrap();
    assert_eq!(map.layers().len(), 14);
    assert!(map.layer(LayerKind::Runtime13).is_none());

    assert_eq!(
        Map::parse_v102(&tagged(&all), &[0; 9]),
        Err(Error::AreaTableLength(9))
    );
}

#[test]
fn every_truncation_fails_cleanly() {
    let data = wld();
    let layers_end = data.len() - 11;
    for len in 0..data.len() {
        let result = Map::parse_wld(&data[..len]);
        if len < layers_end {
            assert!(result.is_err(), "prefix of {len} bytes parsed");
        }
    }
    let v102 = tagged(&(0..15).collect::<Vec<_>>());
    for len in 0..v102.len() {
        let _ = Map::parse_v102(&v102[..len], &area_table());
    }
}

#[test]
fn huge_claimed_sizes_fail_without_allocating() {
    let mut container = MAP_CLASS.to_le_bytes().to_vec();
    container.extend(u32::MAX.to_le_bytes());
    container.extend([0; 4]);
    container.extend([u8::MAX; 4]);
    container.extend(3_u32.to_le_bytes());
    container.extend([9, 1, 2]);
    let data = wld_with(&header(b"t", (1, 1), 0), &container, &[]);
    assert!(matches!(
        Map::parse_wld(&data),
        Err(Error::Rle {
            layer: 0,
            source: RleError::Short { actual: 2, .. }
        })
    ));

    let mut container = tagged(&[]);
    container.extend(tagged_layer(W, H, &[]));
    container.truncate(container.len() - 4);
    container.extend(u32::MAX.to_le_bytes());
    let data = wld_with(&header(b"t", (W, H), 0), &container, &[]);
    assert!(matches!(
        Map::parse_wld(&data),
        Err(Error::LayerLength { layer: 0, .. })
    ));
}

#[test]
fn corrupt_bytes_never_panic() {
    let payloads: Vec<Vec<u8>> = LayerKind::STORED
        .iter()
        .map(|kind| rle::encode(&[kind.slot(); NODES]))
        .collect();
    let compact = wld_with(&header(b"t", (W, H), 0), &compact(&payloads), &[ANIMALS_END]);
    for input in [wld(), compact] {
        for at in 0..input.len() {
            for value in [0, 4, u8::MAX] {
                let mut data = input.clone();
                data[at] = value;
                let _ = Map::parse_wld(&data);
            }
        }
    }
    let v102 = tagged(&(0..15).collect::<Vec<_>>());
    for at in 0..v102.len() {
        let mut data = v102.clone();
        data[at] = !data[at];
        let _ = Map::parse_v102(&data, &area_table());
    }
}

#[test]
fn layer_kinds_round_trip_slots() {
    for kind in LayerKind::ALL {
        assert_eq!(LayerKind::from_slot(kind.slot()), Some(kind));
    }
    assert_eq!(LayerKind::from_slot(15), None);
}

#[test]
fn writes_a_decoded_wld_back_byte_for_byte() {
    let bytes = wld();
    assert_eq!(Map::parse_wld(&bytes).unwrap().to_wld(), bytes);
}

#[test]
fn writes_a_title_that_ran_into_the_size_back_over_it() {
    let bytes = wld_with(
        &header(b"A title of twenty-two", (W, H), 0),
        &tagged(&v151_slots()),
        &[ANIMALS_END],
    );
    let map = Map::parse_wld(&bytes).unwrap();
    assert!(map.header().unwrap().dimensions_overwritten());
    assert_eq!(map.to_wld(), bytes);
}

#[test]
fn a_built_map_decodes_to_itself() {
    let hq = Some(Position { x: 1, y: 1 });
    let mut headquarters = [None; MAX_PLAYERS];
    headquarters[0] = hq;
    let header = Header::new("Built", "robigo", Terrain::Wasteland, 1, headquarters).unwrap();
    let layers = LayerKind::STORED
        .iter()
        .map(|&kind| Layer::new(kind, layer_bytes(kind.slot())))
        .collect();
    let areas = vec![Area {
        kind: AreaKind::Land,
        seed: Position { x: 0, y: 0 },
        size: u32::try_from(NODES).unwrap(),
    }];
    let animals = vec![Animal {
        species: 2,
        position: Position { x: 3, y: 2 },
    }];
    let map = Map::new(header, W, H, layers, areas, animals).unwrap();
    let decoded = Map::parse_wld(&map.to_wld()).unwrap();
    assert_eq!(decoded, map);
    assert_eq!(decoded.header().unwrap().title(), "Built");
    assert_eq!(decoded.areas().len(), AREA_COUNT);
}

#[test]
fn a_layer_left_out_is_stored_absent() {
    let header = Header::new("Sparse", "", Terrain::Greenland, 0, [None; MAX_PLAYERS]).unwrap();
    let layers = vec![Layer::new(LayerKind::Heights, vec![5; NODES])];
    let map = Map::new(header, W, H, layers, Vec::new(), Vec::new()).unwrap();
    let decoded = Map::parse_wld(&map.to_wld()).unwrap();
    assert_eq!(decoded.layers().len(), 1);
    assert_eq!(decoded.quirks().len(), LayerKind::STORED.len() - 1);
}

#[test]
fn building_refuses_what_a_wld_cannot_hold() {
    let header = || Header::new("Bad", "", Terrain::Greenland, 0, [None; MAX_PLAYERS]).unwrap();
    let new = |layers, areas| Map::new(header(), W, H, layers, areas, Vec::new());
    assert!(matches!(
        new(vec![Layer::new(LayerKind::Heights, vec![0; 3])], Vec::new()),
        Err(BuildError::LayerLength { .. })
    ));
    assert_eq!(
        new(vec![Layer::new(LayerKind::Runtime13, vec![0; NODES])], Vec::new()),
        Err(BuildError::Layer(LayerKind::Runtime13))
    );
    let unused = Area {
        kind: AreaKind::Unused,
        seed: Position::default(),
        size: 0,
    };
    assert_eq!(
        new(Vec::new(), vec![unused; AREA_COUNT + 1]),
        Err(BuildError::Areas(AREA_COUNT + 1))
    );
    assert_eq!(
        Map::new(header(), 0, H, Vec::new(), Vec::new(), Vec::new()),
        Err(BuildError::Empty { width: 0, height: H })
    );
    assert!(
        Header::new(
            "Twenty characters!!!",
            "",
            Terrain::Greenland,
            0,
            [None; MAX_PLAYERS]
        )
        .is_err()
    );
    assert!(Header::new("snow \u{2603}", "", Terrain::Greenland, 0, [None; MAX_PLAYERS]).is_err());
}
