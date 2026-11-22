use super::*;

fn chunk(id: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend(u32::try_from(data.len()).unwrap().to_be_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn form(kind: &[u8], chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut inner = kind.to_vec();
    for c in chunks {
        inner.extend(c);
    }
    let mut out = b"FORM".to_vec();
    out.extend(u32::try_from(inner.len()).unwrap().to_be_bytes());
    out.extend(inner);
    out
}

fn bmhd(width: u16, height: u16, planes: u8, masking: u8, compression: u8) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(width.to_be_bytes());
    b.extend(height.to_be_bytes());
    b.extend(3i16.to_be_bytes());
    b.extend((-4i16).to_be_bytes());
    b.extend([planes, masking, compression, 0]);
    b.extend(254u16.to_be_bytes());
    b.extend([5, 6]);
    b.extend(320i16.to_be_bytes());
    b.extend(200i16.to_be_bytes());
    chunk(b"BMHD", &b)
}

fn crng(rate: u16, flags: u16, low: u8, high: u8) -> Vec<u8> {
    let mut b = vec![18, 52];
    b.extend(rate.to_be_bytes());
    b.extend(flags.to_be_bytes());
    b.extend([low, high]);
    chunk(b"CRNG", &b)
}

/// A plain `ByteRun1` encoder: runs of three or more become repeats, the rest literals.
fn pack(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut literal: Vec<u8> = Vec::new();
    let flush = |literal: &mut Vec<u8>, out: &mut Vec<u8>| {
        for part in literal.chunks(128) {
            out.push(u8::try_from(part.len() - 1).unwrap());
            out.extend(part);
        }
        literal.clear();
    };
    let mut i = 0;
    while i < data.len() {
        let run = data[i..].iter().take(128).take_while(|&&b| b == data[i]).count();
        if run >= 3 {
            flush(&mut literal, &mut out);
            out.push(u8::try_from(257 - run).unwrap());
            out.push(data[i]);
            i += run;
        } else {
            literal.push(data[i]);
            i += 1;
        }
    }
    flush(&mut literal, &mut out);
    out
}

fn to_planar(pixels: &[u8], width: usize, planes: usize, mask: bool) -> Vec<u8> {
    let plane_bytes = width.div_ceil(16) * 2;
    let mut out = Vec::new();
    for row in pixels.chunks(width) {
        for plane in 0..planes {
            let mut bits = vec![0u8; plane_bytes];
            for (x, &px) in row.iter().enumerate() {
                if (px >> plane) & 1 == 1 {
                    bits[x / 8] |= 1 << (7 - x % 8);
                }
            }
            out.extend(bits);
        }
        if mask {
            out.extend(vec![u8::MAX; plane_bytes]);
        }
    }
    out
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| {
            if i % 7 < 3 {
                9
            } else {
                u8::try_from(i * 31 % 251).unwrap()
            }
        })
        .collect()
}

#[test]
fn pbm_rows_are_padded_to_even_width() {
    let file = form(
        b"PBM ",
        &[
            bmhd(3, 2, 8, 2, 0),
            chunk(b"CMAP", &[0, 0, 0, 10, 20, 30]),
            chunk(b"BODY", &[1, 2, 3, 170, 4, 5, 6, 187]),
        ],
    );
    let picture = parse(&file).unwrap();
    assert_eq!(picture.form, Form::Pbm);
    assert_eq!(picture.image.pixels(), [1, 2, 3, 4, 5, 6]);
    let palette = picture.palette.unwrap();
    assert_eq!(palette.colour(1), [10, 20, 30]);
    assert_eq!(palette.colour(2), [0, 0, 0]);
    let header = picture.header;
    assert_eq!((header.x, header.y), (3, -4));
    assert_eq!(header.masking, Masking::TransparentColour);
    assert_eq!(header.compression, Compression::None);
    assert_eq!(header.transparent_colour, 254);
    assert_eq!((header.x_aspect, header.y_aspect), (5, 6));
    assert_eq!((header.page_width, header.page_height), (320, 200));
    assert!(picture.skipped.is_empty(), "{:?}", picture.skipped);
}

#[test]
fn pbm_byterun1_runs_may_cross_rows() {
    let pixels = pattern(5 * 7);
    let padded: Vec<u8> = pixels.chunks(5).flat_map(|r| [r, &[0][..]].concat()).collect();
    let file = form(b"PBM ", &[bmhd(5, 7, 8, 0, 1), chunk(b"BODY", &pack(&padded))]);
    assert_eq!(parse(&file).unwrap().image.pixels(), pixels);
}

#[test]
fn ilbm_planes_combine_into_indices() {
    for (planes, mask, compression) in [(3, false, 0), (8, false, 1), (1, true, 0), (5, true, 1)] {
        let width = 19;
        let pixels: Vec<u8> = pattern(width * 4)
            .iter()
            .map(|&p| p & u8::try_from((1u16 << planes) - 1).unwrap())
            .collect();
        let body = to_planar(&pixels, width, planes, mask);
        let body = if compression == 1 { pack(&body) } else { body };
        let file = form(
            b"ILBM",
            &[
                bmhd(19, 4, u8::try_from(planes).unwrap(), u8::from(mask), compression),
                chunk(b"BODY", &body),
            ],
        );
        let picture = parse(&file).unwrap();
        assert_eq!(picture.form, Form::Ilbm);
        assert_eq!(picture.image.pixels(), pixels, "{planes} planes");
    }
}

#[test]
fn ranges_hotspot_and_unknown_chunks() {
    let file = form(
        b"PBM ",
        &[
            bmhd(1, 1, 8, 0, 0),
            chunk(b"DPPS", &[1, 2, 3]),
            crng(ColourRange::RATE_60HZ, 3, 240, 247),
            crng(0, 0, 1, 7),
            chunk(b"GRAB", &[(-2_i16).to_be_bytes(), 9_i16.to_be_bytes()].concat()),
            chunk(b"TINY", &[0; 5]),
            chunk(b"BODY", &[42, 0]),
            chunk(b"BODY", &[43, 0]),
        ],
    );
    let picture = parse(&file).unwrap();
    assert_eq!(picture.image.pixels(), [42]);
    assert_eq!(picture.hotspot, Some((-2, 9)));
    assert_eq!(picture.palette, None);
    assert_eq!(
        picture.skipped,
        [ChunkId(*b"DPPS"), ChunkId(*b"TINY"), ChunkId::BODY]
    );
    let [cycling, still] = picture.ranges[..] else {
        panic!("expected two ranges")
    };
    assert!(cycling.is_active() && cycling.is_reversed());
    assert_eq!((cycling.rate, cycling.low, cycling.high), (16_384, 240, 247));
    assert!(!still.is_active() && !still.is_reversed());
}

#[test]
fn final_pad_byte_may_be_missing() {
    let mut file = form(b"PBM ", &[bmhd(1, 1, 8, 0, 0), chunk(b"BODY", &[7, 0, 0])]);
    file.pop();
    let len = u32::try_from(file.len() - 8).unwrap();
    file[4..8].copy_from_slice(&len.to_be_bytes());
    assert_eq!(parse(&file).unwrap().image.pixels(), [7]);
}

#[test]
fn palette_alone() {
    let mut cmap = vec![0; 800];
    cmap[767] = 99;
    cmap[768] = 1;
    let file = form(b"PBM ", &[bmhd(1, 1, 8, 0, 0), chunk(b"CMAP", &cmap)]);
    assert_eq!(read_palette(&file).unwrap().colour(255), [0, 0, 99]);
    assert_eq!(parse(&file), Err(Error::MissingChunk(ChunkId::BODY)));
    let bare = form(b"PBM ", &[bmhd(1, 1, 8, 0, 0)]);
    assert_eq!(read_palette(&bare), Err(Error::MissingChunk(ChunkId::CMAP)));
}

#[test]
fn malformed_files_are_errors() {
    let body = || chunk(b"BODY", &[0; 64]);
    let cases = [
        (b"RIFF\0\0\0\x04WAVE".to_vec(), Error::NotForm),
        (Vec::new(), Error::NotForm),
        (form(b"8SVX", &[]), Error::UnsupportedForm(ChunkId(*b"8SVX"))),
        (form(b"PBM ", &[body()]), Error::MissingChunk(ChunkId::BMHD)),
        (
            form(b"PBM ", &[bmhd(2, 2, 8, 0, 2), body()]),
            Error::UnsupportedCompression(2),
        ),
        (
            form(b"PBM ", &[bmhd(2, 2, 4, 0, 0), body()]),
            Error::UnsupportedPlanes(4),
        ),
        (
            form(b"ILBM", &[bmhd(2, 2, 9, 0, 0), body()]),
            Error::UnsupportedPlanes(9),
        ),
        (
            form(b"ILBM", &[bmhd(2, 2, 0, 0, 0), body()]),
            Error::UnsupportedPlanes(0),
        ),
        (
            form(b"PBM ", &[bmhd(2, 2, 8, 1, 0), body()]),
            Error::UnsupportedMaskPlane,
        ),
        (
            form(b"PBM ", &[bmhd(9, 8, 8, 0, 0), body()]),
            Error::BodyTooShort {
                expected: 80,
                body: 64,
            },
        ),
        (
            form(b"PBM ", &[bmhd(2, 4, 8, 0, 1), chunk(b"BODY", &[0xfd, 1])]),
            Error::BodyTooShort { expected: 8, body: 2 },
        ),
        (
            form(
                b"PBM ",
                &[bmhd(u16::MAX, u16::MAX, 8, 0, 1), chunk(b"BODY", &[0x81, 0])],
            ),
            Error::BodyTooShort {
                expected: 65_536 * 65_535,
                body: 2,
            },
        ),
    ];
    for (file, expected) in cases {
        assert_eq!(parse(&file), Err(expected));
    }
    let short_header = form(b"PBM ", &[chunk(b"BMHD", &[0; 19])]);
    assert!(matches!(parse(&short_header), Err(Error::Truncated(_))));
}

#[test]
fn form_length_past_the_end_is_clamped() {
    let mut file = form(b"PBM ", &[bmhd(2, 1, 8, 0, 0), chunk(b"BODY", &[4, 5])]);
    assert!(
        parse(&file).unwrap().quirks().is_empty(),
        "{:?}",
        parse(&file).unwrap().quirks()
    );
    file[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    let picture = parse(&file).unwrap();
    assert_eq!(picture.image.pixels(), [4, 5]);
    assert_eq!(
        picture.quirks(),
        [Quirk::FormLength {
            declared: usize::try_from(u32::MAX).unwrap(),
            available: file.len() - 8
        }]
    );
    assert!(read_palette(&file).is_err());

    let mut cut = form(b"PBM ", &[bmhd(2, 1, 8, 0, 0), chunk(b"BODY", &[4, 5])]);
    cut.truncate(cut.len() - 1);
    assert!(matches!(parse(&cut), Err(Error::Truncated(_))));
}

#[test]
fn empty_image_is_allowed() {
    let file = form(b"PBM ", &[bmhd(0, 3, 8, 0, 1), chunk(b"BODY", &[])]);
    let picture = parse(&file).unwrap();
    assert_eq!((picture.image.width(), picture.image.height()), (0, 3));
}

#[test]
fn damaged_input_never_panics() {
    let pixels = pattern(6 * 5);
    let samples = [
        form(
            b"PBM ",
            &[
                bmhd(6, 5, 8, 0, 1),
                chunk(b"CMAP", &[1; 9]),
                crng(1, 1, 2, 3),
                chunk(b"BODY", &pack(&pixels)),
            ],
        ),
        form(
            b"ILBM",
            &[
                bmhd(6, 5, 8, 1, 1),
                chunk(b"GRAB", &[0; 4]),
                chunk(b"BODY", &pack(&to_planar(&pixels, 6, 8, true))),
            ],
        ),
    ];
    for sample in samples {
        for len in 0..sample.len() {
            let _ = parse(&sample[..len]);
            let _ = read_palette(&sample[..len]);
        }
        for i in 0..sample.len() {
            for value in [0, 1, 127, 128, u8::MAX, sample[i] ^ 0b0101_0101] {
                let mut damaged = sample.clone();
                damaged[i] = value;
                let _ = parse(&damaged);
            }
        }
    }
}

#[test]
fn chunk_ids_print_as_text() {
    assert_eq!(ChunkId::PBM.to_string(), "PBM ");
    assert_eq!(ChunkId([b'A', 0, u8::MAX, b'z']).to_string(), "A??z");
    assert_eq!(format!("{:?}", ChunkId::BODY), "ChunkId(\"BODY\")");
}
