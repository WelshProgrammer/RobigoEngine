//! Decodes the intro films from a player's own copy of the game. Each test runs only when its
//! environment variable points at the data, and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder (`INTRO.SMK` and `INTRO2.SMK`)
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder (`INTRO.SMK` only)
//! - `S2_DATA_CD`: a dump of the retail CD (`INTRO.SMK` and `INTRO2.SMK`)
//!
//! Only counts and hashes are asserted. When `ffmpeg` is on the path, a sample of frames is also
//! compared pixel for pixel against its decoder.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use s2_data::DataSet;
use s2_formats::smacker::{AudioCodec, Pcm, Scaling, Version, Video};

/// What decoding a whole film should produce.
struct Expected {
    path: &'static str,
    size: usize,
    /// FNV-1a 64 over every frame's indices followed by its palette.
    video_hash: u64,
    /// FNV-1a 64 over every audio sample of track 0, in order.
    audio_hash: u64,
}

const FRAMES: usize = 2224;
const AUDIO_CHUNKS: usize = 2209;
/// Stereo sample frames in track 0 of both films; ffmpeg agrees.
const AUDIO_FRAMES: usize = 3_236_586;
const SAMPLE_FRAMES: [usize; 6] = [0, 1, 2, 750, 1500, 2223];

const INTRO: Expected = Expected {
    path: "VIDEO/INTRO.SMK",
    size: 16_383_164,
    video_hash: 0x890a_3f0c_75f4_95b4,
    audio_hash: 0x6c8d_3557_426c_626f,
};

const INTRO2: Expected = Expected {
    path: "VIDEO/INTRO2.SMK",
    size: 30_322_032,
    video_hash: 0xcee6_9cef_3a43_9094,
    audio_hash: 0x6c8d_3557_426c_626f,
};

#[derive(Clone, Copy)]
struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

fn data_set(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

/// Decodes `SAMPLE_FRAMES` with ffmpeg as packed RGB, or `None` when ffmpeg is unavailable.
fn ffmpeg_frames(file: &[u8]) -> Option<Vec<u8>> {
    let select: Vec<_> = SAMPLE_FRAMES.iter().map(|n| format!("eq(n\\,{n})")).collect();
    let mut child = Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-f", "smk"])
        .args(["-i", "pipe:0", "-map", "0:v", "-fps_mode", "passthrough"])
        .args(["-vf", &format!("select='{}'", select.join("+"))])
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let mut rgb = Vec::new();
    std::thread::scope(|scope| {
        scope.spawn(move || stdin.write_all(file));
        stdout.read_to_end(&mut rgb).unwrap();
    });
    child.wait().ok()?.success().then_some(rgb)
}

fn check(data: &DataSet, expected: &Expected) {
    let file = data.read(expected.path).unwrap();
    assert_eq!(file.len(), expected.size);
    let video = Video::parse(&file).unwrap();
    let header = video.header();
    assert_eq!(header.version, Version::Smk2);
    assert_eq!((header.width, header.height), (320, 200));
    assert_eq!(header.scaling, Scaling::None);
    assert!(!header.ring_frame);
    assert_eq!(header.frame_count as usize, FRAMES);
    assert_eq!(header.frame_duration, Duration::from_millis(66));
    assert_eq!(header.frame_rate(), (500, 33));
    let track = header.tracks[0].unwrap();
    assert_eq!(
        (track.rate, track.channels, track.sixteen_bit, track.codec),
        (22_050, 2, false, AudioCodec::Dpcm)
    );
    assert!(header.tracks[1..].iter().all(Option::is_none));

    let mut decoder = video.decoder();
    let (mut video_hash, mut audio_hash) = (Fnv::new(), Fnv::new());
    let (mut frames, mut chunks, mut samples) = (0, 0, 0);
    let mut ours = Vec::new();
    let started = std::time::Instant::now();
    while let Some(frame) = decoder.decode_next().unwrap() {
        video_hash.write(frame.image.pixels());
        video_hash.write(frame.palette.0.as_flattened());
        if let Some(pcm) = frame.audio(0) {
            let Pcm::U8(pcm) = pcm else {
                panic!("track 0 is 8-bit")
            };
            audio_hash.write(pcm);
            chunks += 1;
            samples += pcm.len();
        }
        if SAMPLE_FRAMES.contains(&frames) {
            ours.extend(
                frame
                    .image
                    .to_rgba(frame.palette)
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| &p[..3]),
            );
        }
        frames += 1;
    }
    let elapsed = started.elapsed();
    eprintln!(
        "{}: {frames} frames in {elapsed:?} ({:.0} fps), video {:#018x}, audio {:#018x}",
        expected.path,
        f64::from(u32::try_from(frames).unwrap()) / elapsed.as_secs_f64(),
        video_hash.0,
        audio_hash.0,
    );
    assert_eq!(frames, FRAMES);
    assert_eq!(chunks, AUDIO_CHUNKS);
    assert_eq!(samples, AUDIO_FRAMES * 2);
    assert_eq!(video_hash.0, expected.video_hash);
    assert_eq!(audio_hash.0, expected.audio_hash);

    match ffmpeg_frames(&file) {
        Some(reference) => assert!(reference == ours, "frames differ from ffmpeg"),
        None => eprintln!("ffmpeg unavailable: skipped the pixel comparison"),
    }
}

#[test]
fn plain_intro_films() {
    let Some(data) = data_set("S2_DATA_PLAIN") else {
        return;
    };
    check(&data, &INTRO);
    check(&data, &INTRO2);
}

#[test]
fn gold_intro_film() {
    let Some(data) = data_set("S2_DATA_GOLD") else {
        return;
    };
    check(&data, &INTRO);
    assert!(!data.exists(INTRO2.path));
}

#[test]
fn cd_intro_films() {
    let Some(data) = data_set("S2_DATA_CD") else {
        return;
    };
    check(&data, &INTRO);
    check(&data, &INTRO2);
}
