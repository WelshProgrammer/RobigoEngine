//! Songs played through an instrument into audio, ready to bake: the FM driver into an emulated
//! OPL3, or a General MIDI rendering through a sound font.
//!
//! Each tick of the sequencer sends its events to the instrument at the first frame of the tick,
//! and the instrument then makes its frames until the next tick: at `r` frames a second, tick `n`
//! starts at frame `⌊n · r / 120⌋`.

use std::sync::Arc;

use nuked_opl3::Opl3Chip;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use s2_formats::music::{Event, TimbreBank};

use crate::fm::FmDriver;
use crate::sequencer::{Score, Sequencer};
use crate::stream::CHANNELS;

/// The OPL3's own sample rate: its 14.318 MHz clock divided by 288.
pub const OPL_RATE: u32 = 49_716;

/// The rate the General MIDI rendering is made at.
pub const MIDI_RATE: u32 = 44_100;

/// Ticks of the Miles sequencer a second.
const TICKS_PER_SECOND: u64 = 120;

/// How long a stopped song's notes may ring on, in seconds.
const MOST_TAIL: usize = 5;

/// How much silence ends the tail: a tenth of a second.
const SILENCE_PER_SECOND: usize = 10;

/// The frames a loop seam is judged over: about 5 ms.
const SEAM: usize = 256;

/// A seam this close to going on, in sample units, about −66 dBFS, is taken as soon as one
/// comes; failing that, the closest seam of the pass.
const CLOSE_ENOUGH: i32 = 16;

/// The frames a seam that is not close enough is blended over: about 46 ms at 44,100 Hz.
const BLEND: usize = 2048;

/// What turns a song's events into frames.
pub trait Instrument {
    /// Frames a second.
    fn rate(&self) -> u32;

    /// Takes one event at the current frame.
    fn play(&mut self, event: &Event);

    /// Makes frames until `frames` holds `until` of them.
    fn sound(&mut self, frames: &mut Vec<[i16; CHANNELS]>, until: usize);
}

/// S2's FM driver over an emulated OPL3: the register writes the driver makes reach the chip as
/// it makes them.
pub struct Opl3 {
    driver: FmDriver,
    chip: Opl3Chip,
}

impl std::fmt::Debug for Opl3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opl3").finish_non_exhaustive()
    }
}

impl Opl3 {
    /// The driver over the instruments of `bank`, its start's writes already on the chip.
    #[must_use]
    pub fn new(bank: TimbreBank) -> Self {
        let mut instrument = Self {
            driver: FmDriver::new(bank),
            chip: Opl3Chip::new(OPL_RATE),
        };
        instrument.write();
        instrument
    }

    fn write(&mut self) {
        for (register, value) in self.driver.take_writes() {
            self.chip.write_register(register, value);
        }
    }
}

impl Instrument for Opl3 {
    fn rate(&self) -> u32 {
        OPL_RATE
    }

    fn play(&mut self, event: &Event) {
        self.driver.send(event);
        self.write();
    }

    fn sound(&mut self, frames: &mut Vec<[i16; CHANNELS]>, until: usize) {
        let mut frame = [0; CHANNELS];
        while frames.len() < until {
            let _ = self.chip.generate(&mut frame);
            frames.push(frame);
        }
    }
}

/// A General MIDI synth playing a sound font, set up as the Miles library sets every channel
/// before a song: volume and expression 127, pan 64, the bend centred and a bend range of 2.
pub struct GeneralMidi {
    synth: Synthesizer,
    left: Vec<f32>,
    right: Vec<f32>,
}

/// The controllers every channel starts with: volume, pan, expression, then the bend range
/// through registered parameter 0.
const CHANNEL_START: [(i32, i32); 7] = [(7, 127), (10, 64), (11, 127), (101, 0), (100, 0), (6, 2), (38, 0)];

/// The status bytes of the messages the synth takes.
const NOTE_OFF: i32 = 0x80;
const NOTE_ON: i32 = 0x90;
const POLY_PRESSURE: i32 = 0xa0;
const CONTROLLER: i32 = 0xb0;
const PROGRAM_CHANGE: i32 = 0xc0;
const CHANNEL_PRESSURE: i32 = 0xd0;
const PITCH_BEND: i32 = 0xe0;

impl std::fmt::Debug for GeneralMidi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneralMidi").finish_non_exhaustive()
    }
}

impl GeneralMidi {
    /// A synth over `font`.
    ///
    /// # Errors
    ///
    /// What the synth says when it cannot run at [`MIDI_RATE`].
    pub fn new(font: &Arc<SoundFont>) -> Result<Self, String> {
        let settings = SynthesizerSettings::new(i32::try_from(MIDI_RATE).unwrap_or(i32::MAX));
        let mut synth = Synthesizer::new(font, &settings).map_err(|error| error.to_string())?;
        for channel in 0..16 {
            for (controller, value) in CHANNEL_START {
                synth.process_midi_message(channel, CONTROLLER, controller, value);
            }
            synth.process_midi_message(channel, PITCH_BEND, 0, 64);
        }
        Ok(Self {
            synth,
            left: Vec::new(),
            right: Vec::new(),
        })
    }
}

impl Instrument for GeneralMidi {
    fn rate(&self) -> u32 {
        MIDI_RATE
    }

    fn play(&mut self, event: &Event) {
        let (channel, command, first, second) = match *event {
            Event::NoteOn {
                channel,
                key,
                velocity,
            } => (channel, NOTE_ON, key, velocity),
            Event::NoteOff { channel, key, .. } => (channel, NOTE_OFF, key, 0),
            Event::PolyPressure {
                channel,
                key,
                pressure,
            } => (channel, POLY_PRESSURE, key, pressure),
            Event::Controller {
                channel,
                controller,
                value,
            } => (channel, CONTROLLER, controller, value),
            Event::ProgramChange { channel, program } => (channel, PROGRAM_CHANGE, program, 0),
            Event::ChannelPressure { channel, pressure } => (channel, CHANNEL_PRESSURE, pressure, 0),
            Event::PitchBend { channel, value } => {
                let [low, high] =
                    [value % 128, value / 128].map(|part| u8::try_from(part).unwrap_or_default());
                (channel, PITCH_BEND, low, high)
            }
            _ => return,
        };
        self.synth
            .process_midi_message(i32::from(channel), command, i32::from(first), i32::from(second));
    }

    fn sound(&mut self, frames: &mut Vec<[i16; CHANNELS]>, until: usize) {
        let count = until.saturating_sub(frames.len());
        if count == 0 {
            return;
        }
        self.left.resize(count, 0.0);
        self.right.resize(count, 0.0);
        self.synth.render(&mut self.left, &mut self.right);
        frames.extend(
            self.left
                .iter()
                .zip(&self.right)
                .map(|(&left, &right)| [left, right].map(sample)),
        );
    }
}

/// A synth sample as 16 bits, held to the range.
fn sample(value: f32) -> i16 {
    #[expect(clippy::cast_possible_truncation, reason = "held to the 16-bit range first")]
    let held = (value * f32::from(i16::MAX))
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
    held
}

/// A song as the instrument played it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The frames at the instrument's rate, left then right.
    pub frames: Vec<[i16; CHANNELS]>,
    /// The frame of each jump back the song made, in order. Between two jumps lies one pass of
    /// the song's loop.
    pub jumps: Vec<usize>,
}

/// Plays `score` through `instrument`. A song that loops stops at its `passes`-th jump back; one
/// that ends rings on until a tenth of a second of silence, or for at most 5 seconds.
#[must_use]
pub fn render(instrument: &mut dyn Instrument, score: Score, passes: usize) -> Rendered {
    let rate = u64::from(instrument.rate());
    let frame_of = |tick: u64| usize::try_from(tick * rate / TICKS_PER_SECOND).unwrap_or(usize::MAX);
    let mut sequencer = Sequencer::new(score);
    let mut frames = Vec::new();
    let mut tick = 0u64;
    let mut events = Vec::new();
    while sequencer.playing() && sequencer.jumps().len() < passes {
        events.clear();
        sequencer.tick(&mut events);
        for event in &events {
            instrument.play(event);
        }
        tick += 1;
        instrument.sound(&mut frames, frame_of(tick));
    }
    let jumps: Vec<usize> = sequencer.jumps().iter().map(|&jump| frame_of(jump)).collect();
    if let Some(&last) = jumps.get(passes.wrapping_sub(1)) {
        frames.truncate(last);
    } else {
        ring_out(instrument, &mut frames);
    }
    Rendered { frames, jumps }
}

/// Lets the notes still sounding ring until a tenth of a second of silence, or for 5 seconds at
/// most, then cuts the silence off.
fn ring_out(instrument: &mut dyn Instrument, frames: &mut Vec<[i16; CHANNELS]>) {
    let rate = usize::try_from(instrument.rate()).unwrap_or(usize::MAX);
    let start = frames.len();
    let mut quiet = 0;
    while quiet < rate / SILENCE_PER_SECOND && frames.len() - start < MOST_TAIL * rate {
        let at = frames.len();
        instrument.sound(frames, at + 1);
        quiet = if frames[at] == [0; CHANNELS] { quiet + 1 } else { 0 };
    }
    let sounding = frames
        .iter()
        .rposition(|&frame| frame != [0; CHANNELS])
        .map_or(0, |at| at + 1);
    frames.truncate(sounding.max(start));
}

/// A song ready to bake: its frames, and the stretch of them that repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    /// The frames at the instrument's rate, left then right.
    pub frames: Vec<[i16; CHANNELS]>,
    /// The frames that repeat, as the first and how many; `None` for a song that plays once.
    pub repeat: Option<(usize, usize)>,
    /// The largest difference over the seam between what the loop plays and what a third pass
    /// would have played, in sample units, before any blend; 0 for a song that plays once.
    pub seam: i32,
    /// How many frames before the end of the repeat were blended into those before its start,
    /// for a seam that was not close enough; 0 for none.
    pub blended: usize,
}

/// Renders `score` through `instrument` for baking. A song that loops is played for three
/// passes, and one pass of the second repeats, starting at the first point where the second and
/// third passes agree to within 16 sample units over 256 frames, or where they agree most closely
/// if they never do, so that going round the loop sounds as going on would. A seam further out
/// than 16 units, which a synth's free-running chorus or vibrato makes, has the 2,048 frames before
/// the repeat's end blended evenly into the 2,048 before its start, so the last frame leads into
/// the first as the frame before the start does. The frames end where the repeat does. A song
/// that ends plays once and rings out.
#[must_use]
pub fn render_song(instrument: &mut dyn Instrument, score: Score) -> Song {
    let Rendered { mut frames, jumps } = render(instrument, score, 3);
    let [first, second, third, ..] = jumps[..] else {
        return Song {
            frames,
            repeat: None,
            seam: 0,
            blended: 0,
        };
    };
    let length = second - first;
    let span = (third - second).min(length).saturating_sub(SEAM);
    let gap: Vec<i32> = (0..span + SEAM)
        .map(|at| {
            let [a, b] = [frames[first + at], frames[second + at]];
            (i32::from(a[0]) - i32::from(b[0]))
                .abs()
                .max((i32::from(a[1]) - i32::from(b[1])).abs())
        })
        .collect();
    let windows: Vec<i32> = worst_over_windows(&gap, SEAM)
        .into_iter()
        .take(span.max(1))
        .collect();
    let best = windows.iter().copied().min().unwrap_or_default();
    let (offset, seam) = windows
        .iter()
        .copied()
        .enumerate()
        .find(|&(_, worst)| worst <= best.max(CLOSE_ENOUGH))
        .unwrap_or((0, 0));
    frames.truncate(second + offset);
    let start = first + offset;
    let blended = if seam > CLOSE_ENOUGH {
        blend(&mut frames, start)
    } else {
        0
    };
    Song {
        frames,
        repeat: Some((start, length)),
        seam,
        blended,
    }
}

/// Blends the last frames of `frames` evenly into the ones before frame `start`, as many as
/// [`BLEND`] or as `start` allows, ending wholly on the frame before `start`. Gives back how many.
fn blend(frames: &mut [[i16; CHANNELS]], start: usize) -> usize {
    let count = BLEND.min(start).min(frames.len() - start);
    let end = frames.len();
    let whole = i32::try_from(count).unwrap_or(i32::MAX);
    for step in 0..count {
        let into = frames[start - count + step];
        let weight = i32::try_from(step + 1).unwrap_or(i32::MAX);
        let frame = &mut frames[end - count + step];
        for (sample, &target) in frame.iter_mut().zip(&into) {
            let mixed = (i32::from(*sample) * (whole - weight) + i32::from(target) * weight) / whole;
            *sample = i16::try_from(mixed).unwrap_or_default();
        }
    }
    count
}

/// The largest value of each window of `width` values in `values`, from each start.
fn worst_over_windows(values: &[i32], width: usize) -> Vec<i32> {
    let mut window: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut worst = Vec::with_capacity(values.len().saturating_sub(width) + 1);
    for (at, &value) in values.iter().enumerate() {
        while window.back().is_some_and(|&back| values[back] <= value) {
            window.pop_back();
        }
        window.push_back(at);
        if window.front().is_some_and(|&front| front + width <= at) {
            window.pop_front();
        }
        if at + 1 >= width {
            worst.push(window.front().map_or(0, |&front| values[front]));
        }
    }
    worst
}
