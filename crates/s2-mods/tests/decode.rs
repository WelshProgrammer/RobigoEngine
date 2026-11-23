//! A mod's samples in every form the loader takes, from made-up tones: `fixtures/tone.flac` and
//! `fixtures/tone.ogg` are 50 ms of a 440 Hz sine at 22,050 Hz in stereo, made with ffmpeg's
//! `sine` source. `fixtures/looped.flac` and `fixtures/looped.ogg` are the same tone tagged
//! `LOOPSTART=100` and `LOOPLENGTH=900`.

use std::path::Path;

use s2_mods::{AudioStream, Error, decode, write_wav};

/// Frames in 50 ms at 22,050 Hz.
const FRAMES: usize = 1102;

fn fixture(name: &str) -> (std::path::PathBuf, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes)
}

/// The loudest sample in `frames`.
fn peak(frames: &[[i16; 2]]) -> i32 {
    frames
        .iter()
        .flatten()
        .map(|&sample| i32::from(sample).abs())
        .max()
        .unwrap_or_default()
}

#[test]
fn flac_and_ogg_decode_to_stereo_at_their_own_rate() {
    for name in ["tone.flac", "tone.ogg"] {
        let (path, bytes) = fixture(name);
        let decoded = decode(&path, bytes).unwrap();
        assert_eq!(decoded.rate, 22_050, "{name}");
        assert!(
            decoded.frames.len().abs_diff(FRAMES) < 64,
            "{name}: {} frames",
            decoded.frames.len()
        );
        assert!(peak(&decoded.frames) > 1000, "{name} is not silent");
        assert!(
            decoded.frames.iter().all(|[left, right]| left == right),
            "{name}: both sides alike"
        );
    }
}

#[test]
fn an_eight_bit_mono_wav_widens_to_both_sides() {
    let decoded = decode(Path::new("click.wav"), write_wav(8000, &[128, 255, 0])).unwrap();
    assert_eq!(decoded.rate, 8000);
    assert_eq!(&*decoded.frames, &[[0, 0], [32_512, 32_512], [-32_768, -32_768]]);
}

#[test]
fn a_sixteen_bit_stereo_wav_keeps_its_two_sides() {
    let mut file = Vec::new();
    let frames: [[i16; 2]; 3] = [[1000, -1000], [2000, -2000], [-3000, 3000]];
    let data: Vec<u8> = frames
        .iter()
        .flatten()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let rate: u32 = 44_100;
    let size = u32::try_from(data.len()).unwrap();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + size).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16_u32.to_le_bytes());
    file.extend_from_slice(&1_u16.to_le_bytes());
    file.extend_from_slice(&2_u16.to_le_bytes());
    file.extend_from_slice(&rate.to_le_bytes());
    file.extend_from_slice(&(rate * 4).to_le_bytes());
    file.extend_from_slice(&4_u16.to_le_bytes());
    file.extend_from_slice(&16_u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&size.to_le_bytes());
    file.extend_from_slice(&data);
    let decoded = decode(Path::new("wide.wav"), file).unwrap();
    assert_eq!(decoded.rate, 44_100);
    assert_eq!(&*decoded.frames, &frames);
}

#[test]
fn a_file_that_is_not_audio_is_refused() {
    let refused = decode(Path::new("notes.ogg"), b"not a sound at all".to_vec()).unwrap_err();
    assert!(matches!(refused, Error::Audio { .. }), "{refused}");
}

#[test]
fn a_files_loop_tags_name_what_repeats() {
    for name in ["looped.flac", "looped.ogg"] {
        let (path, _) = fixture(name);
        assert_eq!(
            AudioStream::open(&path).unwrap().repeat(),
            Some((100, 900)),
            "{name}"
        );
    }
    let (path, _) = fixture("tone.flac");
    assert_eq!(
        AudioStream::open(&path).unwrap().repeat(),
        None,
        "an untagged file plays once"
    );
}
