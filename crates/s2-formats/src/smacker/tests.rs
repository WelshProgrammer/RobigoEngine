//! Synthetic streams built by a small encoder.

use super::*;

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    fn bit(&mut self, bit: bool) {
        if self.used.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if bit {
            *self.bytes.last_mut().unwrap() |= 1 << (self.used % 8);
        }
        self.used += 1;
    }

    fn bits(&mut self, value: u32, count: u32) {
        for i in 0..count {
            self.bit(value >> i & 1 != 0);
        }
    }
}

enum Shape {
    Leaf(u16),
    Branch(Box<Shape>, Box<Shape>),
}

impl Shape {
    fn balanced(values: &[u16]) -> Self {
        if let [value] = values {
            return Self::Leaf(*value);
        }
        let (left, right) = values.split_at(values.len() / 2);
        Self::Branch(Box::new(Self::balanced(left)), Box::new(Self::balanced(right)))
    }

    fn bytes() -> Self {
        Self::balanced(&(0..256).collect::<Vec<_>>())
    }

    fn write(&self, w: &mut BitWriter, leaf: &mut dyn FnMut(&mut BitWriter, u16)) {
        match self {
            Self::Leaf(value) => {
                w.bit(false);
                leaf(w, *value);
            }
            Self::Branch(zero, one) => {
                w.bit(true);
                zero.write(w, leaf);
                one.write(w, leaf);
            }
        }
    }

    fn path(&self, value: u16) -> Option<Vec<bool>> {
        match self {
            Self::Leaf(v) => (*v == value).then(Vec::new),
            Self::Branch(zero, one) => [(false, zero), (true, one)].into_iter().find_map(|(b, n)| {
                n.path(value).map(|mut p| {
                    p.insert(0, b);
                    p
                })
            }),
        }
    }

    fn emit(&self, w: &mut BitWriter, value: u16) {
        for bit in self.path(value).expect("value in tree") {
            w.bit(bit);
        }
    }
}

fn write_byte_tree(w: &mut BitWriter, shape: Option<&Shape>) {
    let Some(shape) = shape else {
        w.bit(false);
        return;
    };
    w.bit(true);
    shape.write(w, &mut |w, v| w.bits(v.into(), 8));
    w.bit(false);
}

/// A 16-bit tree over `values`, with full byte trees for its leaves.
struct WordCoder {
    shape: Shape,
    bytes: Shape,
}

impl WordCoder {
    fn new(values: &[u16]) -> Self {
        Self {
            shape: Shape::balanced(values),
            bytes: Shape::bytes(),
        }
    }

    fn write(&self, w: &mut BitWriter, escapes: [u16; 3]) {
        w.bit(true);
        write_byte_tree(w, Some(&self.bytes));
        write_byte_tree(w, Some(&self.bytes));
        for e in escapes {
            w.bits(e.into(), 16);
        }
        let bytes = &self.bytes;
        self.shape.write(w, &mut |w, v| {
            let [lo, hi] = v.to_le_bytes();
            bytes.emit(w, lo.into());
            bytes.emit(w, hi.into());
        });
        w.bit(false);
    }

    fn emit(&self, w: &mut BitWriter, value: u16) {
        self.shape.emit(w, value);
    }
}

const NO_ESCAPES: [u16; 3] = [0xfff0, 0xfff1, 0xfff2];

fn kind(block: u16, run_code: u16, data: u8) -> u16 {
    u16::from(data) << 8 | run_code << 2 | block
}

struct Coders {
    mmap: WordCoder,
    mclr: WordCoder,
    full: WordCoder,
    kind: WordCoder,
}

impl Coders {
    fn new(kinds: &[u16]) -> Self {
        Self {
            mmap: WordCoder::new(&[0x0000, 0xffff, 0x8421, 0x00ff]),
            mclr: WordCoder::new(&[0x0201, 0x0403]),
            full: WordCoder::new(&[0x0201, 0x0403, 0x0605, 0x0807]),
            kind: WordCoder::new(kinds),
        }
    }

    fn trees(&self, kind_escapes: [u16; 3]) -> Vec<u8> {
        let mut w = BitWriter::default();
        self.mmap.write(&mut w, NO_ESCAPES);
        self.mclr.write(&mut w, NO_ESCAPES);
        self.full.write(&mut w, NO_ESCAPES);
        self.kind.write(&mut w, kind_escapes);
        w.bytes
    }
}

struct Track {
    word: u32,
    max: u32,
}

struct Frame {
    palette: Option<Vec<u8>>,
    audio: Vec<Vec<u8>>,
    video: Vec<u8>,
}

impl Frame {
    fn video(video: Vec<u8>) -> Self {
        Self {
            palette: None,
            audio: Vec::new(),
            video,
        }
    }

    fn encode(&self) -> (u8, Vec<u8>) {
        let mut kind = 0;
        let mut out = Vec::new();
        if let Some(body) = &self.palette {
            kind |= 1;
            let len = (body.len() + 1).div_ceil(4);
            out.push(u8::try_from(len).unwrap());
            out.extend_from_slice(body);
            out.resize(len * 4, 0);
        }
        for (track, chunk) in self.audio.iter().enumerate() {
            kind |= 2 << track;
            out.extend_from_slice(&u32::try_from(chunk.len() + 4).unwrap().to_le_bytes());
            out.extend_from_slice(chunk);
        }
        out.extend_from_slice(&self.video);
        out.resize(out.len().next_multiple_of(4), 0);
        (kind, out)
    }
}

struct File<'a> {
    signature: &'a [u8; 4],
    width: u32,
    height: u32,
    rate: i32,
    flags: u32,
    tracks: Vec<Track>,
    trees: Vec<u8>,
    frames: Vec<Frame>,
}

impl File<'_> {
    fn new(width: u32, height: u32, trees: Vec<u8>, frames: Vec<Frame>) -> Self {
        Self {
            signature: b"SMK2",
            width,
            height,
            rate: -6600,
            flags: 0,
            tracks: Vec::new(),
            trees,
            frames,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let ring = u32::from(self.flags & 1 != 0);
        let frames: Vec<_> = self.frames.iter().map(Frame::encode).collect();
        let mut out = self.signature.to_vec();
        let count = u32::try_from(frames.len()).unwrap() - ring;
        for v in [self.width, self.height, count] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.rate.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        for t in 0..MAX_TRACKS {
            let max = self.tracks.get(t).map_or(0, |t| t.max);
            out.extend_from_slice(&max.to_le_bytes());
        }
        out.extend_from_slice(&u32::try_from(self.trees.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&[0; 16]);
        for t in 0..MAX_TRACKS {
            let word = self.tracks.get(t).map_or(0, |t| t.word);
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&[0; 4]);
        assert_eq!(out.len(), HEADER_SIZE);
        for (i, (_, data)) in frames.iter().enumerate() {
            let size = u32::try_from(data.len()).unwrap() | u32::from(i == 0);
            out.extend_from_slice(&size.to_le_bytes());
        }
        out.extend(frames.iter().map(|(kind, _)| kind));
        out.extend_from_slice(&self.trees);
        for (_, data) in &frames {
            out.extend_from_slice(data);
        }
        out
    }
}

fn decode_all(bytes: &[u8]) -> Vec<(Vec<u8>, Palette)> {
    let video = Video::parse(bytes).unwrap();
    let mut decoder = video.decoder();
    let mut out = Vec::new();
    while let Some(frame) = decoder.decode_next().unwrap() {
        out.push((frame.image.pixels().to_vec(), frame.palette.clone()));
    }
    out
}

const SOLID: u16 = 3;

#[test]
fn header_tables_and_ring_frame() {
    let solid = kind(SOLID, 3, 9);
    let coders = Coders::new(&[solid, 0]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, solid);
    let frame = || Frame::video(w.bytes.clone());
    let mut file = File::new(16, 4, coders.trees(NO_ESCAPES), vec![frame(), frame(), frame()]);
    file.flags = 1;
    let bytes = file.bytes();
    let video = Video::parse(&bytes).unwrap();
    let header = video.header();
    assert_eq!(header.version, Version::Smk2);
    assert_eq!((header.width, header.height, header.frame_count), (16, 4, 2));
    assert!(header.ring_frame);
    assert_eq!(header.frame_rate(), (500, 33));
    assert_eq!(header.display_height(), 4);
    assert_eq!(video.frames().len(), 3);
    assert!(video.frames()[0].keyframe && !video.frames()[1].keyframe);
    let frames = decode_all(&bytes);
    assert_eq!(frames.len(), 2);
    assert!(frames[1].0.iter().all(|&p| p == 9));
}

#[test]
fn rejects_bad_headers() {
    let coders = Coders::new(&[0, 1]);
    let good = File::new(8, 8, coders.trees(NO_ESCAPES), vec![Frame::video(vec![0; 4])]);
    let bytes = good.bytes();
    assert!(Video::parse(&bytes).is_ok());

    let mut bad = bytes.clone();
    bad[3] = b'3';
    assert!(matches!(Video::parse(&bad), Err(Error::Signature(_))));

    let mut huge = bytes.clone();
    huge[4..8].copy_from_slice(&4096u32.to_le_bytes());
    assert!(matches!(Video::parse(&huge), Err(Error::Dimensions { .. })));

    let mut many = bytes.clone();
    many[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(Video::parse(&many), Err(Error::Truncated(_))));

    let mut long = bytes.clone();
    long[HEADER_SIZE..HEADER_SIZE + 4].copy_from_slice(&64u32.to_le_bytes());
    assert_eq!(Video::parse(&long).unwrap_err(), Error::FrameBounds(0));

    for len in 0..bytes.len() - 4 {
        assert!(Video::parse(&bytes[..len]).is_err(), "prefix {len}");
    }
}

#[test]
fn block_types() {
    let mono = kind(0, 0, 0);
    let full = kind(1, 0, 0);
    let void = kind(2, 0, 0);
    let solid = kind(SOLID, 3, 7);
    let coders = Coders::new(&[mono, full, void, solid]);
    let mut first = BitWriter::default();
    coders.kind.emit(&mut first, solid);
    let mut second = BitWriter::default();
    coders.kind.emit(&mut second, mono);
    coders.mclr.emit(&mut second, 0x0201);
    coders.mmap.emit(&mut second, 0x8421);
    coders.kind.emit(&mut second, full);
    for pair in [0x0201, 0x0403, 0x0605, 0x0807, 0x0201, 0x0201, 0x0403, 0x0403] {
        coders.full.emit(&mut second, pair);
    }
    coders.kind.emit(&mut second, void);
    coders.kind.emit(&mut second, void);
    let bytes = File::new(
        8,
        8,
        coders.trees(NO_ESCAPES),
        vec![Frame::video(first.bytes), Frame::video(second.bytes)],
    )
    .bytes();
    let frames = decode_all(&bytes);
    assert!(frames[0].0.iter().all(|&p| p == 7));
    #[rustfmt::skip]
    let expected = [
        2, 1, 1, 1, 3, 4, 1, 2,
        1, 2, 1, 1, 7, 8, 5, 6,
        1, 1, 2, 1, 1, 2, 1, 2,
        1, 1, 1, 2, 3, 4, 3, 4,
        7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7,
    ];
    assert_eq!(frames[1].0, expected);
}

#[test]
fn recent_values_reset_each_frame() {
    let fill = kind(SOLID, 0, 5);
    let pair = kind(SOLID, 1, 6);
    let escapes = [0xe000, 0xe001, 0xe002];
    let coders = Coders::new(&[fill, pair, 0xe000, 0xe001]);
    let mut first = BitWriter::default();
    coders.kind.emit(&mut first, fill);
    coders.kind.emit(&mut first, 0xe000);
    coders.kind.emit(&mut first, pair);
    let mut second = BitWriter::default();
    coders.kind.emit(&mut second, pair);
    coders.kind.emit(&mut second, 0xe001);
    coders.mclr.emit(&mut second, 0x0201);
    coders.mmap.emit(&mut second, 0x0000);
    coders.kind.emit(&mut second, 0xe001);
    let bytes = File::new(
        16,
        4,
        coders.trees(escapes),
        vec![Frame::video(first.bytes), Frame::video(second.bytes)],
    )
    .bytes();
    let frames = decode_all(&bytes);
    let row = |frame: &[u8]| -> Vec<u8> { frame.iter().step_by(4).take(4).copied().collect() };
    assert_eq!(row(&frames[0].0), [5, 5, 6, 6]);
    assert_eq!(row(&frames[1].0), [6, 6, 1, 6]);
}

#[test]
fn smk4_full_block_variants() {
    let full = kind(1, 2, 0);
    let coders = Coders::new(&[full, 0]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, full);
    w.bit(true);
    coders.full.emit(&mut w, 0x0201);
    coders.full.emit(&mut w, 0x0403);
    w.bit(false);
    w.bit(true);
    coders.full.emit(&mut w, 0x0201);
    coders.full.emit(&mut w, 0x0403);
    coders.full.emit(&mut w, 0x0605);
    coders.full.emit(&mut w, 0x0807);
    w.bit(false);
    w.bit(false);
    for _ in 0..4 {
        coders.full.emit(&mut w, 0x0807);
        coders.full.emit(&mut w, 0x0605);
    }
    let mut file = File::new(12, 4, coders.trees(NO_ESCAPES), vec![Frame::video(w.bytes)]);
    file.signature = b"SMK4";
    let frames = decode_all(&file.bytes());
    #[rustfmt::skip]
    let expected = [
        1, 1, 2, 2, 3, 4, 1, 2, 5, 6, 7, 8,
        1, 1, 2, 2, 3, 4, 1, 2, 5, 6, 7, 8,
        3, 3, 4, 4, 7, 8, 5, 6, 5, 6, 7, 8,
        3, 3, 4, 4, 7, 8, 5, 6, 5, 6, 7, 8,
    ];
    assert_eq!(frames[0].0, expected);
}

#[test]
fn clips_partial_blocks() {
    let fill = kind(SOLID, 5, 3);
    let coders = Coders::new(&[fill, 0]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, fill);
    let bytes = File::new(6, 5, coders.trees(NO_ESCAPES), vec![Frame::video(w.bytes)]).bytes();
    let frames = decode_all(&bytes);
    assert_eq!(frames[0].0, [3; 30]);
}

#[test]
fn palette_chunks() {
    let coders = Coders::new(&[kind(2, 63, 0), 0]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, kind(2, 63, 0));
    let mut first = Frame::video(w.bytes.clone());
    first.palette = Some(vec![0x3f, 0x3f, 0x3f, 0x10, 0x20, 0x30, 0xff, 0xfd]);
    let mut second = Frame::video(w.bytes.clone());
    second.palette = Some(vec![0x81, 0x40, 0x00, 0xff, 0xfc]);
    let third = Frame::video(w.bytes);
    let bytes = File::new(4, 4, coders.trees(NO_ESCAPES), vec![first, second, third]).bytes();
    let video = Video::parse(&bytes).unwrap();
    let mut decoder = video.decoder();
    let frame = decoder.decode_next().unwrap().unwrap();
    assert!(frame.palette_changed);
    assert_eq!(frame.palette.colour(0), [255; 3]);
    assert_eq!(frame.palette.colour(1), [65, 130, 195]);
    let frame = decoder.decode_next().unwrap().unwrap();
    assert_eq!(frame.palette.colour(1), [65, 130, 195]);
    assert_eq!(frame.palette.colour(2), [255; 3]);
    let frame = decoder.decode_next().unwrap().unwrap();
    assert!(!frame.palette_changed);
    assert_eq!(frame.palette.colour(2), [255; 3]);
    assert!(decoder.decode_next().unwrap().is_none());
    decoder.rewind();
    assert_eq!(decoder.position(), 0);
    assert!(decoder.decode_next().unwrap().is_some());
}

/// Encodes interleaved samples as a Smacker DPCM chunk with full byte trees.
fn dpcm_chunk(channels: usize, sixteen_bit: bool, samples: &[i32]) -> Vec<u8> {
    let bytes_per = if sixteen_bit { 2 } else { 1 };
    let mut out = u32::try_from(samples.len() * bytes_per)
        .unwrap()
        .to_le_bytes()
        .to_vec();
    let tree = Shape::bytes();
    let mut w = BitWriter::default();
    w.bit(true);
    w.bit(channels == 2);
    w.bit(sixteen_bit);
    for _ in 0..channels * bytes_per {
        write_byte_tree(&mut w, Some(&tree));
    }
    for ch in (0..channels).rev() {
        if sixteen_bit {
            let [hi, lo] = i16::try_from(samples[ch]).unwrap().to_be_bytes();
            w.bits(hi.into(), 8);
            w.bits(lo.into(), 8);
        } else {
            w.bits(u32::try_from(samples[ch]).unwrap(), 8);
        }
    }
    for (i, &sample) in samples.iter().enumerate().skip(channels) {
        let delta = sample - samples[i - channels];
        if sixteen_bit {
            let [lo, hi] = u16::try_from(delta.rem_euclid(1 << 16)).unwrap().to_le_bytes();
            tree.emit(&mut w, lo.into());
            tree.emit(&mut w, hi.into());
        } else {
            tree.emit(&mut w, u16::try_from(delta.rem_euclid(256)).unwrap());
        }
    }
    out.extend_from_slice(&w.bytes);
    out
}

fn track_word(channels: u8, sixteen_bit: bool, compressed: bool) -> u32 {
    const RATE: u32 = 22_050;
    RATE | 1 << 30
        | u32::from(compressed) << 31
        | u32::from(sixteen_bit) << 29
        | u32::from(channels == 2) << 28
}

fn audio_file(track: Track, chunk: Vec<u8>) -> Vec<u8> {
    let coders = Coders::new(&[kind(2, 63, 0), 0]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, kind(2, 63, 0));
    let mut frame = Frame::video(w.bytes);
    frame.audio = vec![chunk];
    let mut file = File::new(4, 4, coders.trees(NO_ESCAPES), vec![frame]);
    file.tracks = vec![track];
    file.bytes()
}

#[derive(Debug, PartialEq)]
enum Samples {
    U8(Vec<u8>),
    I16(Vec<i16>),
}

fn first_audio(bytes: &[u8]) -> Result<Option<Samples>, Error> {
    let video = Video::parse(bytes)?;
    let mut decoder = video.decoder();
    let frame = decoder.decode_next()?.unwrap();
    Ok(frame.audio(0).map(|pcm| match pcm {
        Pcm::U8(s) => Samples::U8(s.to_vec()),
        Pcm::I16(s) => Samples::I16(s.to_vec()),
    }))
}

#[test]
fn dpcm_audio_all_formats() {
    let wave: Vec<i32> = (0..40).map(|i| (i * 37) % 200 + 20).collect();
    for channels in [1u8, 2] {
        let chunk = dpcm_chunk(channels.into(), false, &wave);
        let track = Track {
            word: track_word(channels, false, true),
            max: 1000,
        };
        let expected: Vec<u8> = wave.iter().map(|&s| u8::try_from(s).unwrap()).collect();
        assert_eq!(
            first_audio(&audio_file(track, chunk)).unwrap(),
            Some(Samples::U8(expected))
        );

        let loud: Vec<i32> = wave.iter().map(|&s| (s - 110) * 250).collect();
        let chunk = dpcm_chunk(channels.into(), true, &loud);
        let track = Track {
            word: track_word(channels, true, true),
            max: 1000,
        };
        let expected: Vec<i16> = loud.iter().map(|&s| i16::try_from(s).unwrap()).collect();
        assert_eq!(
            first_audio(&audio_file(track, chunk)).unwrap(),
            Some(Samples::I16(expected))
        );
    }
}

#[test]
fn raw_audio() {
    let track = Track {
        word: track_word(2, false, false),
        max: 0,
    };
    let pcm = first_audio(&audio_file(track, vec![1, 2, 3, 4])).unwrap();
    assert_eq!(pcm, Some(Samples::U8(vec![1, 2, 3, 4])));

    let track = Track {
        word: track_word(1, true, false),
        max: 0,
    };
    let pcm = first_audio(&audio_file(track, vec![1, 2, 0xff, 0xff])).unwrap();
    assert_eq!(pcm, Some(Samples::I16(vec![i16::from_le_bytes([1, 2]), -1])));

    let track = Track {
        word: track_word(2, true, false),
        max: 0,
    };
    assert!(matches!(
        first_audio(&audio_file(track, vec![1, 2])),
        Err(Error::Audio(_))
    ));
}

#[test]
fn audio_errors() {
    let wave = [100, 101, 102, 103];
    let track = Track {
        word: track_word(1, false, true),
        max: 3,
    };
    assert!(matches!(
        first_audio(&audio_file(track, dpcm_chunk(1, false, &wave))),
        Err(Error::Audio(_))
    ));
    let track = Track {
        word: track_word(2, false, true),
        max: 100,
    };
    assert!(matches!(
        first_audio(&audio_file(track, dpcm_chunk(1, false, &wave))),
        Err(Error::Audio(_))
    ));
    let track = Track {
        word: track_word(1, false, true) | 1 << 27,
        max: 100,
    };
    assert_eq!(first_audio(&audio_file(track, vec![0; 8])), Err(Error::BinkAudio));
    let track = Track {
        word: track_word(1, false, true),
        max: 100,
    };
    let mut silent = 4u32.to_le_bytes().to_vec();
    silent.push(0);
    assert_eq!(
        first_audio(&audio_file(track, silent)),
        Ok(Some(Samples::U8(Vec::new())))
    );
}

#[test]
fn damaged_input_never_panics() {
    let solid = kind(SOLID, 2, 4);
    let mono = kind(0, 1, 0);
    let full = kind(1, 1, 0);
    let coders = Coders::new(&[solid, mono, full, 0xe000]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, solid);
    for value in [mono, 0xe000] {
        coders.kind.emit(&mut w, value);
        for _ in 0..2 {
            coders.mclr.emit(&mut w, 0x0403);
            coders.mmap.emit(&mut w, 0x00ff);
        }
    }
    coders.kind.emit(&mut w, full);
    for _ in 0..8 {
        coders.full.emit(&mut w, 0x0605);
    }
    let mut frame = Frame::video(w.bytes);
    frame.palette = Some(vec![0x3f, 0, 0, 0x81, 0x40, 0, 0xff, 0xfb]);
    frame.audio = vec![dpcm_chunk(2, false, &[1, 2, 3, 4, 5, 6])];
    let mut file = File::new(16, 8, coders.trees([0xe000, 0xe001, 0xe002]), vec![frame]);
    file.tracks = vec![Track {
        word: track_word(2, false, true),
        max: 100,
    }];
    let bytes = file.bytes();
    decode_all(&bytes);

    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..5_000 {
        let mut damaged = bytes.clone();
        for _ in 0..=next() % 4 {
            let at = usize::try_from(next()).unwrap() % bytes.len();
            damaged[at] ^= 1 << (next() % 8);
        }
        if let Ok(video) = Video::parse(&damaged) {
            let mut decoder = video.decoder();
            while let Ok(Some(_)) = decoder.decode_next() {}
        }
    }
}

#[test]
fn sound_only_reads_the_sound_and_leaves_the_picture_black() {
    let solid = kind(SOLID, 0, 7);
    let coders = Coders::new(&[solid, 0]);
    let mut w = BitWriter::default();
    coders.kind.emit(&mut w, solid);
    let mut frame = Frame::video(w.bytes);
    frame.audio = vec![vec![1, 2, 3, 4]];
    let mut file = File::new(4, 4, coders.trees(NO_ESCAPES), vec![frame]);
    file.tracks = vec![Track {
        word: track_word(2, false, false),
        max: 0,
    }];
    let bytes = file.bytes();
    let video = Video::parse(&bytes).unwrap();
    let mut whole = video.decoder();
    let pictured = whole.decode_next().unwrap().unwrap();
    assert!(pictured.image.pixels().iter().all(|&pixel| pixel == 7));
    assert_eq!(pictured.audio(0), Some(Pcm::U8(&[1, 2, 3, 4])));
    let mut sound = video.decoder().sound_only();
    let heard = sound.decode_next().unwrap().unwrap();
    assert!(heard.image.pixels().iter().all(|&pixel| pixel == 0));
    assert_eq!(heard.audio(0), Some(Pcm::U8(&[1, 2, 3, 4])));
}
