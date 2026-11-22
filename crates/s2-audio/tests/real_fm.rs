//! The FM driver against S2 itself: the OPL register writes an emulator recorded while S2
//! played each song through `OPL3.MDI`, in the raw OPL capture format (`.dro`, version 2). Runs
//! only when `S2_OPL_CAPTURES` names a folder of `song<n>.dro` files and `S2_DATA_PLAIN` the game;
//! no game data or capture lives here. `S2_OPL_SONG` picks one song.

use std::path::PathBuf;

use s2_audio::{FmDriver, Score, Sequencer};
use s2_data::DataSet;
use s2_formats::music::{Sequence, TimbreBank, Xmidi};

/// The timer registers, which the emulator keeps apart from the rest and never records.
const TIMERS: std::ops::RangeInclusive<u16> = 2..=4;

/// Registers a register state holds: two banks of 256.
const REGISTERS: usize = 512;

/// One register write at its time in milliseconds after the first note.
type Timed = (u32, u16, u8);

/// A capture: the registers when the first note sounded, then every write from that note on that
/// changed a register.
struct Capture {
    state: Vec<(u16, u8)>,
    writes: Vec<Timed>,
}

/// Reads a capture. It opens with a dump of the registers in rising order, both banks of each
/// register together, at time 0, and the first write out of that order is the first note.
fn read_dro(bytes: &[u8]) -> Capture {
    assert_eq!(&bytes[..8], b"DBRAWOPL");
    let (short, long) = (bytes[23], bytes[24]);
    let codes = usize::from(bytes[25]);
    let map = &bytes[26..26 + codes];
    let mut time = 0u32;
    let mut all = Vec::new();
    for &[code, value] in bytes[26 + codes..].as_chunks::<2>().0 {
        if code == short {
            time += u32::from(value) + 1;
        } else if code == long {
            time += (u32::from(value) + 1) << 8;
        } else {
            let bank = if code & 0x80 == 0 { 0 } else { 0x100 };
            all.push((time, bank | u16::from(map[usize::from(code & 0x7f)]), value));
        }
    }
    let mut dumped = 0;
    let mut previous = None;
    for &(time, register, _) in &all {
        let order = (register & 0xff, register >> 8);
        if time != 0 || previous.is_some_and(|previous| order <= previous) {
            break;
        }
        previous = Some(order);
        dumped += 1;
    }
    Capture {
        state: all[..dumped]
            .iter()
            .map(|&(_, register, value)| (register, value))
            .collect(),
        writes: all[dumped..].to_vec(),
    }
}

fn key_on(register: u16, value: u8) -> bool {
    (0xb0..=0xb8).contains(&(register & 0xff)) && value & 0x20 != 0
}

/// Plays `song` until `last` milliseconds after its first note, giving back the registers just
/// before that note and every write from it on that changed a register.
fn play(bank: &TimbreBank, song: &Sequence, last: u32) -> (Vec<(u16, u8)>, Vec<Timed>) {
    let mut driver = FmDriver::new(bank.clone());
    let mut sequencer = Sequencer::new(Score::new(song));
    let mut registers = [0u8; REGISTERS];
    let mut writes = Vec::new();
    let mut state = Vec::new();
    let mut started = None;
    let mut tick = 0u32;
    while sequencer.playing() && started.is_none_or(|start| (tick - start) * 1000 / 120 <= last) {
        let mut events = Vec::new();
        sequencer.tick(&mut events);
        for event in &events {
            driver.send(event);
        }
        for (register, value) in driver.take_writes() {
            if started.is_none() && key_on(register, value) {
                started = Some(tick);
                state = (0..512u16)
                    .zip(registers)
                    .filter(|&(register, value)| value != 0 && !TIMERS.contains(&register))
                    .collect();
            }
            let held = &mut registers[usize::from(register)];
            if *held == value || TIMERS.contains(&register) {
                continue;
            }
            *held = value;
            if let Some(start) = started {
                writes.push(((tick - start) * 1000 / 120, register, value));
            }
        }
        tick += 1;
    }
    (state, writes)
}

/// Compares song `number` with its capture: what differs, or how close the timing came.
fn compare(number: u8, capture: &Capture, state: &[(u16, u8)], ours: &[Timed]) -> Result<String, String> {
    let mut theirs = capture.state.clone();
    theirs.sort_unstable();
    if state != theirs {
        let missing: Vec<_> = theirs
            .iter()
            .filter(|write| !state.contains(write))
            .take(8)
            .collect();
        let extra: Vec<_> = state
            .iter()
            .filter(|write| !theirs.contains(write))
            .take(8)
            .collect();
        return Err(format!(
            "song {number}: the registers at the first note differ; S2 has {missing:x?}, ours {extra:x?}"
        ));
    }
    let compared = ours.len().min(capture.writes.len());
    let differs =
        (0..compared).find(|&at| (ours[at].1, ours[at].2) != (capture.writes[at].1, capture.writes[at].2));
    if let Some(at) = differs {
        let show = |list: &[Timed]| {
            list.iter()
                .skip(at.saturating_sub(4))
                .take(12)
                .map(|(time, register, value)| format!("{time}:{register:03x}={value:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        return Err(format!(
            "song {number}: write {at} of {} differs\n  S2    {}\n  ours  {}",
            capture.writes.len(),
            show(&capture.writes),
            show(ours)
        ));
    }
    let late = (0..compared)
        .map(|at| ours[at].0.abs_diff(capture.writes[at].0))
        .max()
        .unwrap_or_default();
    Ok(format!(
        "song {number}: {compared} writes match, each within {late} ms"
    ))
}

#[test]
fn the_driver_writes_what_v151_wrote() {
    let (Some(captures), Some(data)) = (
        std::env::var_os("S2_OPL_CAPTURES"),
        std::env::var_os("S2_DATA_PLAIN"),
    ) else {
        eprintln!("skipped: set S2_OPL_CAPTURES and S2_DATA_PLAIN to run this test");
        return;
    };
    let data = DataSet::discover(data).unwrap();
    let bank = TimbreBank::parse(&data.read("DRIVERS/SETT_SND.OPL").unwrap()).unwrap();
    let only: Option<u8> = std::env::var("S2_OPL_SONG")
        .ok()
        .map(|song| song.parse().unwrap());
    let mut failures = Vec::new();
    for number in (1..=14u8).filter(|&number| only.is_none_or(|only| only == number)) {
        let Ok(bytes) = std::fs::read(PathBuf::from(&captures).join(format!("song{number}.dro"))) else {
            continue;
        };
        let capture = read_dro(&bytes);
        let song = Xmidi::parse(
            &data
                .read(&format!("DATA/SOUNDDAT/SNG/SNG_{number:04}.DAT"))
                .unwrap(),
        )
        .unwrap()
        .sequences
        .remove(0);
        let last = capture.writes.last().map_or(0, |write| write.0);
        let (state, ours) = play(&bank, &song, last);
        match compare(number, &capture, &state, &ours) {
            Ok(report) => println!("{report}"),
            Err(failure) => failures.push(failure),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
