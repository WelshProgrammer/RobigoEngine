//! The mix checked against the original's own output, run only when `S2_DATA_PLAIN` or
//! `S2_DATA_GOLD` points at a copy of the game. No game data lives here.
//!
//! Each hash is FNV-1a 64 over the 16-bit stereo frames S2 sent the Sound Blaster 16 for item
//! 111 of `SOUND.LST`, the main menu's click, at volume 100 under a full master, from the sound's
//! first frame to its last. The recordings came from DOSBox-X with its mixer at 11,025 Hz and
//! the Sound Blaster's filtering off, with the click's rate patched in S2.EXE's effect table to each
//! rate below so the recordings cover the resampling at rates above and below the mixer's.

use std::path::PathBuf;
use std::sync::Arc;

use s2_audio::{CHANNELS, OUTPUT_BUFFER, Sample, Stream};
use s2_data::DataSet;
use s2_formats::lst::{Archive, Item, Sound};

/// The click's item in `SOUND.LST`: effect `0x3d` plus the item base.
const CLICK: usize = 111;

/// The click's volume in the effect table.
const VOLUME: u16 = 100;

/// A rate the click was recorded at, how many frames it lasted and the hash of those frames.
struct Recorded {
    rate: u16,
    frames: usize,
    fnv: u64,
}

const RECORDED: [Recorded; 6] = [
    Recorded {
        rate: 22_000,
        frames: 303,
        fnv: 0xea04_a9cd_d4aa_bf59,
    },
    Recorded {
        rate: 21_000,
        frames: 317,
        fnv: 0x4622_bcd6_6dfa_df15,
    },
    Recorded {
        rate: 11_200,
        frames: 595,
        fnv: 0x72eb_3bb5_a244_0d11,
    },
    Recorded {
        rate: 11_000,
        frames: 604,
        fnv: 0x5651_f4b8_aaec_e185,
    },
    Recorded {
        rate: 6_500,
        frames: 1024,
        fnv: 0x218f_f0a3_f5a9_9249,
    },
    Recorded {
        rate: 4_000,
        frames: 1662,
        fnv: 0xdaaf_a9f5_3ecb_fb55,
    },
];

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes
        .into_iter()
        .fold(OFFSET, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(PRIME))
}

fn datasets() -> Vec<(&'static str, DataSet)> {
    let found: Vec<_> = ["S2_DATA_PLAIN", "S2_DATA_GOLD"]
        .into_iter()
        .filter_map(|var| {
            let path = std::env::var_os(var)?;
            Some((var, DataSet::discover(PathBuf::from(path)).unwrap()))
        })
        .collect();
    if found.is_empty() {
        eprintln!("skipped: set S2_DATA_PLAIN or S2_DATA_GOLD to run this test");
    }
    found
}

/// Everything the original's mix sends the card for `samples` at `rate`, up to the call the
/// sound ends in.
fn mixed(samples: &Arc<[u8]>, rate: u16) -> Vec<i16> {
    let mut stream = Stream::new();
    assert!(stream.start(0, Sample::Original(Arc::clone(samples)), VOLUME, rate, rate));
    let mut out = Vec::new();
    loop {
        let mut buffer = [0; OUTPUT_BUFFER];
        let ended = stream.mix(&mut buffer);
        out.extend_from_slice(stream.card());
        if !ended.is_empty() {
            return out;
        }
    }
}

#[test]
fn the_click_mixes_as_v151_played_it() {
    for (var, data) in datasets() {
        let lst = Archive::parse(&data.read("DATA/SOUNDDAT/SOUND.LST").unwrap()).unwrap();
        let Some(Item::Sound(Sound::Pcm(bytes))) = lst.get(CLICK) else {
            panic!("{var}: item {CLICK} is not a sample");
        };
        let samples: Arc<[u8]> = Arc::from(bytes.as_slice());
        for recorded in &RECORDED {
            let out = mixed(&samples, recorded.rate);
            let (sound, after) = out.split_at(recorded.frames * CHANNELS);
            let hash = fnv(sound.iter().flat_map(|sample| sample.to_le_bytes()));
            assert_eq!(hash, recorded.fnv, "{var}: the click at {} Hz", recorded.rate);
            assert!(
                after.iter().all(|&sample| sample == 0),
                "{var}: sound past the end"
            );
        }
    }
}
