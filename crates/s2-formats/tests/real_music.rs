//! Music checks against a player's own copy of the game, run only when `S2_DATA_PLAIN` or
//! `S2_DATA_GOLD` points at it. Asserts counts, lengths and hashes; no game data lives here.
//!
//! Set `S2_MUSIC_DUMP` to a directory to also write each song as a `.mid` file there.

use std::path::PathBuf;

use s2_data::DataSet;
use s2_formats::music::{
    Event, MilesControl, Sequence, TICKS_PER_SECOND, TimbreBank, TimbreData, TimbreRef, TimedEvent, Xmidi,
};

/// What each song should decode to.
struct Song {
    name: &'static str,
    /// Length in 1/120 s ticks.
    ticks: u32,
    notes: usize,
    /// Tick of the endless for-loop.
    loop_start: u32,
    /// Tick of the matching next-loop, if the song has one.
    loop_end: Option<u32>,
    /// FNV-1a 64 of the SMF rendering.
    smf_fnv: u64,
}

const SONGS: [Song; 15] = [
    Song {
        name: "SNG_0001",
        ticks: 6984,
        notes: 858,
        loop_start: 72,
        loop_end: Some(6984),
        smf_fnv: 0xfcbb_caee_f6ee_1f46,
    },
    Song {
        name: "SNG_0002",
        ticks: 7542,
        notes: 877,
        loop_start: 54,
        loop_end: Some(7542),
        smf_fnv: 0x7d22_eaab_7cc9_2e3d,
    },
    Song {
        name: "SNG_0003",
        ticks: 9558,
        notes: 786,
        loop_start: 1206,
        loop_end: Some(9558),
        smf_fnv: 0xf9b4_79b3_4581_9a71,
    },
    Song {
        name: "SNG_0004",
        ticks: 20421,
        notes: 1769,
        loop_start: 261,
        loop_end: Some(20421),
        smf_fnv: 0x90c3_d5c4_933f_4565,
    },
    Song {
        name: "SNG_0005",
        ticks: 16560,
        notes: 1569,
        loop_start: 216,
        loop_end: Some(16560),
        smf_fnv: 0x6f3f_f171_ca24_4123,
    },
    Song {
        name: "SNG_0006",
        ticks: 10147,
        notes: 1188,
        loop_start: 67,
        loop_end: Some(10147),
        smf_fnv: 0xba48_41e9_8eee_9490,
    },
    Song {
        name: "SNG_0007",
        ticks: 29340,
        notes: 2219,
        loop_start: 6120,
        loop_end: None,
        smf_fnv: 0xe919_9cb9_69ff_51db,
    },
    Song {
        name: "SNG_0008",
        ticks: 14639,
        notes: 1312,
        loop_start: 239,
        loop_end: Some(14639),
        smf_fnv: 0xf73b_aa02_a489_e16c,
    },
    Song {
        name: "SNG_0009",
        ticks: 15719,
        notes: 1725,
        loop_start: 1079,
        loop_end: Some(15719),
        smf_fnv: 0x342b_45ef_1fa1_0c47,
    },
    Song {
        name: "SNG_0010",
        ticks: 15696,
        notes: 1358,
        loop_start: 288,
        loop_end: Some(15696),
        smf_fnv: 0x364a_7092_6726_f916,
    },
    Song {
        name: "SNG_0011",
        ticks: 10944,
        notes: 954,
        loop_start: 576,
        loop_end: Some(10944),
        smf_fnv: 0x8507_949e_d882_28d1,
    },
    Song {
        name: "SNG_0012",
        ticks: 11108,
        notes: 1218,
        loop_start: 1234,
        loop_end: Some(11108),
        smf_fnv: 0xcdd8_2657_ec71_784a,
    },
    Song {
        name: "SNG_0013",
        ticks: 13230,
        notes: 1683,
        loop_start: 270,
        loop_end: Some(13230),
        smf_fnv: 0xdfc9_4a64_6ec5_25f1,
    },
    Song {
        name: "SNG_0014",
        ticks: 21330,
        notes: 2333,
        loop_start: 270,
        loop_end: None,
        smf_fnv: 0x5e91_ca72_0aaa_f9e0,
    },
    Song {
        name: "SOUND.LST",
        ticks: 43891,
        notes: 5181,
        loop_start: 1001,
        loop_end: Some(43641),
        smf_fnv: 0x085d_799a_9ffa_4945,
    },
];
const TIMBRE_BANK_LEN: usize = 3622;
const TIMBRE_BANK_FNV: u64 = 0xffe6_d377_c530_e18f;

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
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

/// Cuts the XMIDI out of `SOUND.LST` by its `FORM` magic and its own chunk lengths.
fn embedded_xmidi(lst: &[u8]) -> &[u8] {
    let start = lst.windows(4).position(|w| w == b"FORM").unwrap();
    let len_at =
        |at: usize| usize::try_from(u32::from_be_bytes(lst[at + 4..at + 8].try_into().unwrap())).unwrap() + 8;
    let cat = start + len_at(start);
    assert_eq!(&lst[cat..cat + 4], b"CAT ");
    &lst[start..cat + len_at(cat)]
}

fn songs(data: &DataSet) -> Vec<(String, Xmidi)> {
    let mut out: Vec<_> = (1..=14)
        .map(|n| {
            let name = format!("SNG_{n:04}");
            let bytes = data.read(&format!("DATA/SOUNDDAT/SNG/{name}.DAT")).unwrap();
            (name, Xmidi::parse(&bytes).unwrap())
        })
        .collect();
    let lst = data.read("DATA/SOUNDDAT/SOUND.LST").unwrap();
    out.push(("SOUND.LST".into(), Xmidi::parse(embedded_xmidi(&lst)).unwrap()));
    out
}

/// Walks an SMF and returns (format, division, absolute end tick, note-ons, events).
fn read_smf(smf: &[u8]) -> (u16, u16, u32, usize, usize) {
    const STATUS_BIT: u8 = 0x80;
    const DATA_MASK: u8 = 0x7f;
    const KIND_MASK: u8 = 0xf0;
    const NOTE_ON: u8 = 0x90;
    const PROGRAM_CHANGE: u8 = 0xc0;
    const CHANNEL_PRESSURE: u8 = 0xd0;
    const LAST_CHANNEL_STATUS: u8 = 0xef;
    const SYSEX: u8 = 0xf0;
    const SYSEX_ESCAPE: u8 = 0xf7;
    const META: u8 = 0xff;
    const META_END_OF_TRACK: u8 = 0x2f;

    fn varint(d: &[u8], pos: &mut usize) -> u32 {
        let mut v = 0;
        for _ in 0..4 {
            let b = d[*pos];
            *pos += 1;
            v = (v << 7) | u32::from(b & DATA_MASK);
            if b & STATUS_BIT == 0 {
                return v;
            }
        }
        panic!("varint too long");
    }
    assert_eq!(&smf[..8], b"MThd\0\0\0\x06");
    let format = u16::from_be_bytes([smf[8], smf[9]]);
    assert_eq!(u16::from_be_bytes([smf[10], smf[11]]), 1);
    let division = u16::from_be_bytes([smf[12], smf[13]]);
    assert_eq!(&smf[14..18], b"MTrk");
    let len = usize::try_from(u32::from_be_bytes(smf[18..22].try_into().unwrap())).unwrap();
    let track = &smf[22..];
    assert_eq!(track.len(), len);
    let (mut pos, mut tick, mut notes, mut events, mut running) = (0, 0u32, 0, 0, 0u8);
    loop {
        tick += varint(track, &mut pos);
        events += 1;
        let mut status = track[pos];
        if status < STATUS_BIT {
            status = running;
        } else {
            pos += 1;
        }
        match status {
            META => {
                let kind = track[pos];
                pos += 1;
                let n = usize::try_from(varint(track, &mut pos)).unwrap();
                pos += n;
                if kind == META_END_OF_TRACK {
                    assert_eq!(pos, track.len(), "data after end of track");
                    return (format, division, tick, notes, events);
                }
            }
            SYSEX | SYSEX_ESCAPE => {
                let n = usize::try_from(varint(track, &mut pos)).unwrap();
                pos += n;
            }
            STATUS_BIT..=LAST_CHANNEL_STATUS => {
                running = status;
                let n = if matches!(status & KIND_MASK, PROGRAM_CHANGE | CHANNEL_PRESSURE) {
                    1
                } else {
                    2
                };
                assert!(track[pos..pos + n].iter().all(|b| b & STATUS_BIT == 0));
                if status & KIND_MASK == NOTE_ON && track[pos + 1] > 0 {
                    notes += 1;
                }
                pos += n;
            }
            _ => panic!("bad status {status:#x}"),
        }
    }
}

/// The events of `seq` but its tempo events and the writer's loop markers.
fn without_tempo(seq: &Sequence) -> Vec<TimedEvent> {
    const META_MARKER: u8 = 6;
    const META_TEMPO: u8 = 81;
    seq.events
        .iter()
        .filter(|e| match &e.event {
            Event::Meta { kind: META_TEMPO, .. } => false,
            Event::Meta {
                kind: META_MARKER,
                data,
            } => data != b"loopStart" && data != b"loopEnd",
            _ => true,
        })
        .cloned()
        .collect()
}

fn note_ons(seq: &Sequence) -> usize {
    seq.events
        .iter()
        .filter(|e| matches!(e.event, Event::NoteOn { velocity, .. } if velocity > 0))
        .count()
}

/// Bit `n` is set when channel `n` plays any note.
fn note_channels(seq: &Sequence) -> u16 {
    seq.events
        .iter()
        .filter_map(|e| match e.event {
            Event::NoteOn { channel, .. } => Some(channel),
            _ => None,
        })
        .fold(0, |mask, ch| mask | (1 << ch))
}

/// The most notes waiting for their note-off at once; Miles queues at most 32 per sequence.
fn peak_pending_notes(seq: &Sequence) -> usize {
    let mut pending = 0usize;
    let mut peak = 0;
    for e in &seq.events {
        match e.event {
            Event::NoteOn { .. } => pending += 1,
            Event::NoteOff { .. } => pending -= 1,
            _ => {}
        }
        peak = peak.max(pending);
    }
    peak
}

/// Checks that every instrument the song plays has a timbre, as the OPL driver would look it up.
fn assert_instruments_resolve(name: &str, seq: &Sequence, bank: &TimbreBank) {
    for timbre in &seq.timbres {
        assert!(bank.get(*timbre).is_some(), "{name}: TIMB {timbre:?} missing");
    }
    for e in &seq.events {
        let wanted = match e.event {
            Event::NoteOn { channel: 9, key, .. } => TimbreRef {
                patch: key,
                bank: 127,
            },
            Event::ProgramChange { program, .. } => TimbreRef {
                patch: program,
                bank: 0,
            },
            _ => continue,
        };
        assert!(bank.get(wanted).is_some(), "{name}: {wanted:?} missing");
        assert!(seq.timbres.contains(&wanted), "{name}: {wanted:?} not in TIMB");
    }
}

/// Every song opens an endless loop on one channel; most close it at their end.
fn assert_loop_shape(song: &Song, seq: &Sequence) {
    let miles: Vec<_> = seq
        .events
        .iter()
        .filter_map(|e| match e.event {
            Event::Miles { control, value, .. } => Some((e.tick, control, value)),
            _ => None,
        })
        .collect();
    let mut expected = vec![(song.loop_start, MilesControl::ForLoop, 0)];
    expected.extend(song.loop_end.map(|tick| (tick, MilesControl::NextLoop, 90)));
    assert_eq!(miles, expected, "{}", song.name);
}

#[test]
fn timbre_banks_parse_and_match() {
    for (var, data) in datasets() {
        let ad = data.read("DRIVERS/SETT_SND.AD").unwrap();
        let opl = data.read("DRIVERS/SETT_SND.OPL").unwrap();
        assert_eq!(ad.len(), TIMBRE_BANK_LEN, "{var}");
        assert_eq!(fnv1a(&ad), TIMBRE_BANK_FNV, "{var}");
        assert_eq!(ad, opl, "{var}: .AD and .OPL differ");
        let bank = TimbreBank::parse(&ad).unwrap();
        assert_eq!(bank.timbres.len(), 181);
        assert_eq!(bank.timbres.iter().filter(|t| t.bank == 0).count(), 128);
        let drums: Vec<_> = bank.timbres.iter().filter(|t| t.bank == 127).collect();
        assert_eq!(drums.len(), 53);
        assert_eq!(drums.first().map(|t| t.patch), Some(35));
        assert_eq!(drums.last().map(|t| t.patch), Some(87));
        for timbre in &bank.timbres {
            let TimbreData::Opl2(opl2) = timbre.data else {
                panic!("{timbre:?} is not OPL2");
            };
            assert_eq!(opl2.feedback_connection >> 4, 0, "{timbre:?}");
            if timbre.bank == 0 {
                assert_eq!(opl2.transpose, 0, "{timbre:?}");
            } else {
                assert!((24..=84).contains(&opl2.transpose), "{timbre:?}");
            }
        }
    }
}

#[test]
fn songs_parse_resolve_and_render() {
    for (var, data) in datasets() {
        let bank = TimbreBank::parse(&data.read("DRIVERS/SETT_SND.OPL").unwrap()).unwrap();
        let dump = std::env::var_os("S2_MUSIC_DUMP").map(PathBuf::from);
        for ((name, xmidi), expected) in songs(&data).iter().zip(&SONGS) {
            assert_eq!(name, expected.name);
            assert_eq!(xmidi.sequences.len(), 1, "{name}");
            let seq = &xmidi.sequences[0];
            assert!(seq.branches.is_empty(), "{name}");
            assert_instruments_resolve(name, seq, &bank);
            assert_loop_shape(expected, seq);
            assert!(peak_pending_notes(seq) <= 32, "{name}");

            let smf = seq.to_smf();
            let (format, division, end, notes, _) = read_smf(&smf);
            assert_eq!((format, division), (0, 60));
            assert_eq!(end, seq.length_ticks(), "{name}");
            assert_eq!(notes, note_ons(seq), "{name}");
            let read = Sequence::from_smf(&smf).unwrap();
            assert_eq!(without_tempo(&read), without_tempo(seq), "{name}: read back");
            assert_eq!(read.end_tick, seq.length_ticks(), "{name}: read back");
            println!(
                "{name}: {:.1}s, {} ticks, note channels {:016b}, {notes} notes, {} TIMB, peak {} pending",
                f64::from(seq.length_ticks()) / f64::from(TICKS_PER_SECOND),
                seq.length_ticks(),
                note_channels(seq),
                seq.timbres.len(),
                peak_pending_notes(seq),
            );
            if let Some(dir) = &dump {
                std::fs::write(dir.join(format!("{name}.mid")), &smf).unwrap();
            }
            assert_eq!(
                (seq.length_ticks(), notes, fnv1a(&smf)),
                (expected.ticks, expected.notes, expected.smf_fnv),
                "{var} {name}"
            );
        }
    }
}
