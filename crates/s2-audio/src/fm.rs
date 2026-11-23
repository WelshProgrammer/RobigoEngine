//! The OPL3 music driver S2 ships as `DRIVERS/OPL3.MDI`: MIDI channel messages in, OPL register
//! writes out. Played through [`crate::Sequencer`], it makes every register write S2 makes for
//! each song, in order.
//!
//! The driver keeps 20 voices and the chip's 18 two-operator channels. A note takes the first free
//! voice and the next free channel round the 18; with none free it may take one from a quieter
//! claim, or wait silent. Each voice holds its timbre's register values and a set of flags naming
//! the register groups due to be written; a message sets flags and the voice writes the groups in
//! a fixed order. Only two-operator timbres play: a four-operator timbre is never in S2's bank.

use s2_formats::music::{Event, Opl2Timbre, TimbreBank, TimbreData, TimbreRef};

/// The voices the driver plays at once.
pub const VOICES: usize = 20;

/// The chip's two-operator channels, nine in each register bank.
pub const OPL_CHANNELS: usize = 18;

/// One register write: the register, with the second bank's at 256 and up, and its value.
pub type Write = (u16, u8);

/// MIDI channels.
const CHANNELS: usize = 16;

/// The channel whose keys pick drum timbres from bank 127.
const PERCUSSION: u8 = 9;

/// The bank drum timbres sit in, one a key.
const DRUM_BANK: u8 = 127;

/// What a voice's level takes from the velocity, a step every eight (`0xc04` in the driver).
const VELOCITY: [u8; 16] = [
    82, 85, 88, 91, 94, 97, 100, 103, 106, 109, 112, 115, 118, 121, 124, 127,
];

/// F-numbers by semitone and sixteenth of a semitone (`0x7da`). A negative entry is the low 10
/// bits of an F-number for the next block up.
const FNUM: [i16; 192] = [
    690, 692, 695, 697, 700, 702, 705, 707, 710, 713, 715, 718, 720, 723, 726, 728, 731, 733, 736, 739, 741,
    744, 747, 749, 752, 755, 758, 760, 763, 766, 769, 771, 774, 777, 780, 783, 785, 788, 791, 794, 797, 800,
    803, 806, 809, 811, 814, 817, 820, 823, 826, 829, 832, 835, 838, 841, 844, 847, 850, 854, 857, 860, 863,
    866, 869, 872, 875, 879, 882, 885, 888, 891, 895, 898, 901, 904, 908, 911, 914, 917, 921, 924, 927, 931,
    934, 937, 941, 944, 948, 951, 955, 958, 961, 965, 968, 972, 975, 979, 983, 986, 990, 993, 997, 1000,
    1004, 1008, 1011, 1015, 1019, 1022, -511, -509, -507, -505, -504, -502, -500, -498, -496, -494, -492,
    -490, -488, -486, -484, -482, -480, -479, -477, -475, -473, -471, -469, -467, -465, -463, -460, -458,
    -456, -454, -452, -450, -448, -446, -444, -442, -440, -438, -436, -433, -431, -429, -427, -425, -423,
    -420, -418, -416, -414, -412, -409, -407, -405, -403, -401, -398, -396, -394, -391, -389, -387, -385,
    -382, -380, -378, -375, -373, -371, -368, -366, -363, -361, -359, -356, -354, -351, -349, -347, -344,
    -342, -339, -337,
];

/// Each channel's two operators, as operator slots 0 to 35.
const CHANNEL_OPERATORS: [(u8, u8); OPL_CHANNELS] = [
    (0, 3),
    (1, 4),
    (2, 5),
    (6, 9),
    (7, 10),
    (8, 11),
    (12, 15),
    (13, 16),
    (14, 17),
    (18, 21),
    (19, 22),
    (20, 23),
    (24, 27),
    (25, 28),
    (26, 29),
    (30, 33),
    (31, 34),
    (32, 35),
];

/// The register offset of each operator slot within its bank.
const OPERATOR_OFFSET: [u8; 18] = [0, 1, 2, 3, 4, 5, 8, 9, 10, 11, 12, 13, 16, 17, 18, 19, 20, 21];

/// The second register bank.
const SECOND_BANK: u16 = 0x100;

/// The levels a controller or pedal takes as "on".
const ON: u8 = 64;

/// Key-on in register `0xb0`.
const KEY_ON: u8 = 0x20;

/// The vibrato bit of register `0x20`, which the mod wheel sets.
const VIBRATO: u8 = 0x40;

/// Register `0xc0`'s two output bits: left and right.
const LEFT: u8 = 0x10;
const RIGHT: u8 = 0x20;

/// The pan at or below which a voice sounds on the left alone, and at or above which on the
/// right alone.
const PAN_LEFT: u8 = 27;
const PAN_RIGHT: u8 = 100;

/// The level a voice of an unprotected channel claims a chip channel with.
const CLAIM: u16 = 0x7fff;

/// What the library sets every channel's volume, pan, bend range and expression to before a
/// song; it also centres the bend.
const LIBRARY_VOLUME: u8 = 127;
const LIBRARY_EXPRESSION: u8 = 127;
const LIBRARY_PAN: u8 = 64;
const LIBRARY_BEND_RANGE: u8 = 2;

/// The bend wheel's centre.
const BEND_CENTRE: u16 = 0x2000;

/// The register groups a voice writes, in the order it writes them.
mod due {
    pub const CHARACTER: u8 = 0x80;
    pub const LEVEL: u8 = 0x40;
    pub const ENVELOPE: u8 = 0x20;
    pub const WAVEFORM: u8 = 0x10;
    pub const OUTPUT: u8 = 0x08;
    pub const PITCH: u8 = 0x01;
    /// What a voice writes when it takes a chip channel.
    pub const ALL: u8 = CHARACTER | LEVEL | ENVELOPE | WAVEFORM | OUTPUT | PITCH;
}

/// What a MIDI channel holds.
#[derive(Debug, Clone, Copy, Default)]
struct Channel {
    bend_range: u8,
    volume: u8,
    pan: u8,
    bend: u16,
    expression: u8,
    modulation: u8,
    sustain: u8,
    protect: u8,
    bank: u8,
    program: Option<u8>,
    timbre: Option<TimbreRef>,
    /// Voices with a chip channel.
    claims: u8,
}

/// One voice.
#[derive(Debug, Clone, Copy)]
struct Voice {
    channel: u8,
    /// The key the note-on named, which a note-off matches.
    key: u8,
    /// The key it sounds at: the note's, or a drum timbre's own.
    pitch: u8,
    transpose: i8,
    velocity: u8,
    sustained: bool,
    timbre: Opl2Timbre,
    opl: Option<u8>,
    key_on: bool,
    /// What register `0xb0` last got.
    block: u8,
    due: u8,
}

/// The driver.
#[derive(Debug, Clone)]
pub struct FmDriver {
    bank: TimbreBank,
    channels: [Channel; CHANNELS],
    voices: [Option<Voice>; VOICES],
    /// Which MIDI channel holds each chip channel.
    owners: [Option<u8>; OPL_CHANNELS],
    /// The chip channel the last search stopped at.
    last: usize,
    writes: Vec<Write>,
}

impl FmDriver {
    /// The driver over the instruments of `bank`, with the chip set up as the driver's start does
    /// (OPL3 mode on, then every register from the driver's start values) and every channel as
    /// the library leaves it before a song: volume and expression 127, pan 64, the bend centred
    /// and a bend range of 2.
    #[must_use]
    pub fn new(bank: TimbreBank) -> Self {
        let channel = Channel {
            volume: LIBRARY_VOLUME,
            pan: LIBRARY_PAN,
            bend_range: LIBRARY_BEND_RANGE,
            expression: LIBRARY_EXPRESSION,
            bend: BEND_CENTRE,
            ..Channel::default()
        };
        let mut driver = Self {
            bank,
            channels: [channel; CHANNELS],
            voices: [None; VOICES],
            owners: [None; OPL_CHANNELS],
            last: OPL_CHANNELS - 1,
            writes: Vec::new(),
        };
        driver.writes.extend(start_writes());
        driver
    }

    /// Takes the register writes made since the last call, in order.
    pub fn take_writes(&mut self) -> Vec<Write> {
        std::mem::take(&mut self.writes)
    }

    /// Acts on one channel message; anything else is left alone, as the driver skips system
    /// exclusive messages.
    pub fn send(&mut self, event: &Event) {
        match *event {
            Event::NoteOn {
                channel,
                key,
                velocity,
            } if velocity > 0 => self.note_on(channel, key, velocity),
            Event::NoteOn { channel, key, .. } | Event::NoteOff { channel, key, .. } => {
                self.note_off(channel, key);
            }
            Event::PitchBend { channel, value } => {
                self.channel(channel).bend = value;
                self.refresh(channel, due::PITCH);
            }
            Event::ProgramChange { channel, program } => {
                let held = self.channel(channel);
                held.program = Some(program);
                let wanted = TimbreRef {
                    patch: program,
                    bank: held.bank,
                };
                held.timbre = None;
                if self.timbre(wanted).is_some() {
                    self.channel(channel).timbre = Some(wanted);
                }
            }
            Event::Controller {
                channel,
                controller,
                value,
            } => self.controller(channel, controller, value),
            Event::Miles {
                channel,
                control,
                value,
            } => self.controller(channel, control.controller(), value),
            _ => {}
        }
    }

    fn channel(&mut self, channel: u8) -> &mut Channel {
        &mut self.channels[usize::from(channel % 16)]
    }

    fn timbre(&self, wanted: TimbreRef) -> Option<Opl2Timbre> {
        self.bank.timbres.iter().find_map(|timbre| match timbre.data {
            TimbreData::Opl2(data) if (timbre.bank, timbre.patch) == (wanted.bank, wanted.patch) => {
                Some(data)
            }
            _ => None,
        })
    }

    fn controller(&mut self, channel: u8, controller: u8, value: u8) {
        match controller {
            114 => self.channel(channel).bank = value,
            112 => self.channel(channel).protect = value,
            1 => {
                self.channel(channel).modulation = value;
                self.refresh(channel, due::CHARACTER);
            }
            7 => {
                self.channel(channel).volume = value;
                self.refresh(channel, due::LEVEL);
            }
            11 => {
                self.channel(channel).expression = value;
                self.refresh(channel, due::LEVEL);
            }
            10 => {
                self.channel(channel).pan = value;
                self.refresh(channel, due::OUTPUT);
            }
            64 => {
                self.channel(channel).sustain = value;
                if value < ON {
                    self.release_sustained(channel);
                }
            }
            121 => {
                let held = self.channel(channel);
                held.sustain = 0;
                self.release_sustained(channel);
                let held = self.channel(channel);
                held.modulation = 0;
                held.expression = 127;
                held.bend = BEND_CENTRE;
                self.refresh(channel, due::CHARACTER | due::LEVEL | due::PITCH);
            }
            123 => {
                for place in 0..VOICES {
                    if let Some(voice) = self.voices[place]
                        && voice.channel == channel
                    {
                        self.note_off(channel, voice.pitch);
                    }
                }
            }
            6 => self.channel(channel).bend_range = value,
            _ => {}
        }
    }

    /// Marks `due` on every voice of `channel` and writes what each can.
    fn refresh(&mut self, channel: u8, due: u8) {
        for place in 0..VOICES {
            if let Some(voice) = self.voices[place].as_mut()
                && voice.channel == channel
            {
                voice.due |= due;
                self.update(place);
            }
        }
    }

    fn note_on(&mut self, channel: u8, key: u8, velocity: u8) {
        let wanted = if channel == PERCUSSION {
            Some(TimbreRef {
                patch: key,
                bank: DRUM_BANK,
            })
        } else {
            self.channel(channel).timbre
        };
        let Some(timbre) = wanted.and_then(|wanted| self.timbre(wanted)) else {
            return;
        };
        let Some(place) = self.voices.iter().position(Option::is_none) else {
            return;
        };
        let (pitch, transpose) = if channel == PERCUSSION {
            (timbre.transpose, 0)
        } else {
            (key, i8::from_le_bytes([timbre.transpose]))
        };
        self.voices[place] = Some(Voice {
            channel,
            key,
            pitch,
            transpose,
            velocity: VELOCITY[usize::from(velocity / 8)],
            sustained: false,
            timbre,
            opl: None,
            key_on: true,
            block: 0,
            due: due::ALL,
        });
        self.claim(place);
    }

    /// Gives voice `place` the next free chip channel round the 18, or failing that one taken from
    /// a quieter claim.
    fn claim(&mut self, place: usize) {
        for _ in 0..OPL_CHANNELS {
            self.last = (self.last + 1) % OPL_CHANNELS;
            if self.owners[self.last].is_none() {
                self.give(place, self.last);
                return;
            }
        }
        self.steal();
    }

    /// Hands chip channel `opl` to voice `place` and writes all of it.
    fn give(&mut self, place: usize, opl: usize) {
        let Some(voice) = self.voices[place].as_mut() else {
            return;
        };
        voice.opl = Some(u8::try_from(opl).unwrap_or_default());
        voice.due = due::ALL;
        let channel = voice.channel;
        self.owners[opl] = Some(channel);
        self.channel(channel).claims += 1;
        self.update(place);
    }

    /// Lets waiting voices take chip channels from voices with weaker claims (`0x36a6`).
    fn steal(&mut self) {
        let claims: Vec<Option<u16>> = self
            .voices
            .iter()
            .map(|voice| {
                voice.map(|voice| {
                    let held = self.channels[usize::from(voice.channel % 16)];
                    let base = if held.protect >= ON { u16::MAX } else { CLAIM };
                    base.saturating_sub(u16::from(held.claims))
                })
            })
            .collect();
        let mut turns = claims.iter().filter(|claim| claim.is_some()).count();
        while turns > 0 {
            let mut waiting = (0, None);
            let mut weakest = (u16::MAX, None);
            for (place, claim) in claims.iter().enumerate() {
                let (Some(claim), Some(voice)) = (*claim, self.voices[place]) else {
                    continue;
                };
                if voice.opl.is_none() {
                    if claim >= waiting.0 {
                        waiting = (claim, Some(place));
                    }
                } else if claim <= weakest.0 {
                    weakest = (claim, Some(place));
                }
            }
            let (Some(taker), Some(victim)) = (waiting.1, weakest.1) else {
                return;
            };
            if waiting.0 < weakest.0 || waiting.0 == 0 {
                return;
            }
            let Some(opl) = self.voices[victim].and_then(|voice| voice.opl) else {
                return;
            };
            self.release(victim);
            self.give(taker, usize::from(opl));
            turns -= 1;
        }
    }

    fn note_off(&mut self, channel: u8, key: u8) {
        let sustain = self.channel(channel).sustain;
        for place in 0..VOICES {
            let Some(voice) = self.voices[place].as_mut() else {
                continue;
            };
            if voice.key != key || voice.channel != channel {
                continue;
            }
            if sustain >= ON {
                voice.sustained = true;
            } else {
                self.release(place);
            }
        }
    }

    fn release_sustained(&mut self, channel: u8) {
        for place in 0..VOICES {
            if let Some(voice) = self.voices[place]
                && voice.channel == channel
                && voice.sustained
            {
                self.note_off(channel, voice.pitch);
            }
        }
    }

    /// Keys voice `place` off, frees its chip channel and the voice (`0x30d7`).
    fn release(&mut self, place: usize) {
        let Some(voice) = self.voices[place].as_mut() else {
            return;
        };
        if let Some(opl) = voice.opl {
            voice.key_on = false;
            voice.due |= due::PITCH;
            let channel = voice.channel;
            self.update(place);
            self.channel(channel).claims = self.channel(channel).claims.saturating_sub(1);
            self.owners[usize::from(opl)] = None;
        }
        self.voices[place] = None;
    }

    /// Writes the register groups voice `place` has due, in the driver's order (`0x3134`).
    fn update(&mut self, place: usize) {
        let Some(mut voice) = self.voices[place] else {
            return;
        };
        let Some(opl) = voice.opl else {
            return;
        };
        let held = self.channels[usize::from(voice.channel % 16)];
        let (first, second) = CHANNEL_OPERATORS[usize::from(opl)];
        let timbre = voice.timbre;
        let (modulator, carrier) = (timbre.modulator, timbre.carrier);
        let additive = timbre.feedback_connection & 1;
        if voice.due & due::CHARACTER != 0 {
            let vibrato = if held.modulation >= ON { VIBRATO } else { 0 };
            self.operator(first, 0x20, modulator.characteristic | vibrato);
            self.operator(second, 0x20, carrier.characteristic | vibrato);
        }
        if voice.due & due::LEVEL != 0 {
            let loudness = scale(scale(held.volume, held.expression), voice.velocity);
            let level = |byte: u8, carries: bool| {
                let mut level = !byte & 63;
                if carries {
                    level = u8::try_from(u16::from(level) * u16::from(loudness) / 127).unwrap_or(u8::MAX);
                }
                (!level & 63) | (byte & 0xc0)
            };
            self.operator(first, 0x40, level(modulator.level, additive == 1));
            self.operator(second, 0x40, level(carrier.level, true));
        }
        if voice.due & due::ENVELOPE != 0 {
            self.operator(first, 0x60, modulator.attack_decay);
            self.operator(second, 0x60, carrier.attack_decay);
            self.operator(first, 0x80, modulator.sustain_release);
            self.operator(second, 0x80, carrier.sustain_release);
        }
        if voice.due & due::WAVEFORM != 0 {
            self.operator(second, 0xe0, carrier.waveform);
            self.operator(first, 0xe0, modulator.waveform);
        }
        if voice.due & due::OUTPUT != 0 {
            let mut output = additive | (timbre.feedback_connection & 0x0e) | LEFT | RIGHT;
            if held.pan <= PAN_LEFT {
                output &= !RIGHT;
            } else if held.pan >= PAN_RIGHT {
                output &= !LEFT;
            }
            self.channel_register(opl, 0xc0, output);
        }
        if voice.due & due::PITCH != 0 {
            if voice.key_on {
                let (fnum, block) = frequency(&voice, &held);
                self.channel_register(opl, 0xa0, fnum);
                voice.block = block | KEY_ON;
                self.channel_register(opl, 0xb0, voice.block);
            } else {
                self.channel_register(opl, 0xb0, voice.block & !KEY_ON);
            }
        }
        voice.due = 0;
        self.voices[place] = Some(voice);
    }

    fn operator(&mut self, slot: u8, register: u8, value: u8) {
        let bank = if slot >= 18 { SECOND_BANK } else { 0 };
        let offset = OPERATOR_OFFSET[usize::from(slot % 18)];
        self.writes.push((bank | u16::from(register + offset), value));
    }

    fn channel_register(&mut self, opl: u8, register: u8, value: u8) {
        let bank = if opl >= 9 { SECOND_BANK } else { 0 };
        self.writes.push((bank | u16::from(register + opl % 9), value));
    }
}

/// `a` scaled by `b` as the driver does: `a·b/128`, then 1 more unless that is 0.
fn scale(a: u8, b: u8) -> u8 {
    let product = u8::try_from((u16::from(a) * u16::from(b) * 2) >> 8).unwrap_or(u8::MAX);
    if product == 0 { 0 } else { product.wrapping_add(1) }
}

/// Registers `0xa0` and `0xb0`, key-on aside, for `voice` under `held`'s bend (`0x35aa`).
fn frequency(voice: &Voice, held: &Channel) -> (u8, u8) {
    let centre = i16::try_from(BEND_CENTRE).unwrap_or_default();
    let bend = (i16::try_from(held.bend).unwrap_or(centre).wrapping_sub(centre) >> 5)
        .wrapping_mul(i16::from(held.bend_range));
    let mut note = i16::from(voice.pitch) + i16::from(voice.transpose) - 24;
    loop {
        note += 12;
        if note >= 0 {
            break;
        }
    }
    note += 12;
    loop {
        note -= 12;
        if note <= 95 {
            break;
        }
    }
    let mut at = bend.wrapping_add(note.wrapping_shl(8)).wrapping_add(8) >> 4;
    at -= 0xc0;
    loop {
        at += 0xc0;
        if at >= 0 {
            break;
        }
    }
    at += 0xc0;
    loop {
        at -= 0xc0;
        if at <= 0x5ff {
            break;
        }
    }
    let note = at >> 4;
    let semitone = usize::try_from(note % 12).unwrap_or_default();
    let fine = usize::try_from(at & 15).unwrap_or_default();
    let mut fnum = FNUM[semitone * 16 + fine];
    let mut block = i16::from(u8::try_from(note / 12).unwrap_or_default()) - 1;
    if fnum < 0 {
        block += 1;
    }
    if block < 0 {
        block += 1;
        fnum >>= 1;
    }
    let [low, high] = fnum.to_le_bytes();
    let block = u8::try_from(block).unwrap_or_default();
    (low, (high & 3) | (block << 2))
}

/// The writes the driver's start makes: OPL3 mode on, four-operator mode off, then every register
/// of the first bank from 1 to 245 and of the second from 257 to 501, all 0 but the start values.
fn start_writes() -> Vec<Write> {
    let mut writes = vec![(0x105, 1), (0x104, 0)];
    for register in 1..=0xf5 {
        writes.push((register, start_value(register)));
    }
    for register in 0x101..=0x1f5 {
        writes.push((register, start_value(register)));
    }
    writes
}

/// A register's start value: test register 1 at 32 (wave select on), the timer mask at 96,
/// every operator's multiplier at 1, level at 63, envelope at 255 and release at 15, register
/// `0xbd` at 192 and OPL3 mode on.
fn start_value(register: u16) -> u8 {
    let low = register & 0xff;
    let first = register < SECOND_BANK;
    let operator = |base: u16| (base..base + 0x16).contains(&low) && !matches!(low - base, 6 | 7 | 14 | 15);
    match low {
        1 if first => 32,
        4 if first => 96,
        5 if !first => 1,
        0xbd if first => 192,
        _ if operator(0x20) => 1,
        _ if operator(0x40) => 63,
        _ if operator(0x60) => 255,
        _ if operator(0x80) => 15,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use s2_formats::music::{Opl2Operator, Timbre};

    use super::*;

    const PIANO: Opl2Timbre = Opl2Timbre {
        transpose: 0,
        modulator: Opl2Operator {
            characteristic: 0x21,
            level: 0x9b,
            attack_decay: 0xb1,
            sustain_release: 0x25,
            waveform: 1,
        },
        feedback_connection: 0x0e,
        carrier: Opl2Operator {
            characteristic: 0x21,
            level: 0x0f,
            attack_decay: 0x72,
            sustain_release: 0x08,
            waveform: 0,
        },
    };

    fn driver() -> FmDriver {
        let drum = Opl2Timbre {
            transpose: 48,
            ..PIANO
        };
        let mut driver = FmDriver::new(TimbreBank {
            timbres: vec![
                Timbre {
                    patch: 0,
                    bank: 0,
                    data: TimbreData::Opl2(PIANO),
                },
                Timbre {
                    patch: 36,
                    bank: DRUM_BANK,
                    data: TimbreData::Opl2(drum),
                },
            ],
        });
        driver.send(&Event::ProgramChange {
            channel: 0,
            program: 0,
        });
        driver.take_writes();
        driver
    }

    fn on(driver: &mut FmDriver, channel: u8, key: u8) -> Vec<Write> {
        driver.send(&Event::NoteOn {
            channel,
            key,
            velocity: 127,
        });
        driver.take_writes()
    }

    fn off(driver: &mut FmDriver, channel: u8, key: u8) -> Vec<Write> {
        driver.send(&Event::NoteOff {
            channel,
            key,
            velocity: 0,
        });
        driver.take_writes()
    }

    fn registers(writes: &[Write]) -> Vec<u16> {
        writes.iter().map(|&(register, _)| register).collect()
    }

    #[test]
    fn a_note_writes_its_groups_in_order_and_lets_go_with_its_key_off() {
        let mut driver = driver();
        let writes = on(&mut driver, 0, 60);
        assert_eq!(
            registers(&writes),
            [
                0x20, 0x23, 0x40, 0x43, 0x60, 0x63, 0x80, 0x83, 0xe3, 0xe0, 0xc0, 0xa0, 0xb0
            ]
        );
        let key_on = writes.last().unwrap().1;
        assert_ne!(key_on & KEY_ON, 0);
        assert_eq!(off(&mut driver, 0, 60), [(0xb0, key_on & !KEY_ON)]);
        assert_eq!(off(&mut driver, 0, 60), [], "nothing left to let go");
    }

    #[test]
    fn channels_go_round_the_18_and_the_second_bank_follows_the_first() {
        let mut driver = driver();
        on(&mut driver, 0, 60);
        off(&mut driver, 0, 60);
        let second = on(&mut driver, 0, 62);
        assert_eq!(
            second.last().unwrap().0,
            0xb1,
            "the next channel, though the first is free"
        );
        for key in 0..7 {
            on(&mut driver, 0, 70 + key);
        }
        assert_eq!(
            on(&mut driver, 0, 90).last().unwrap().0,
            0x1b0,
            "channel 9 is the second bank's first"
        );
    }

    #[test]
    fn a_drum_sounds_at_its_timbres_key() {
        let mut driver = driver();
        let drum = on(&mut driver, PERCUSSION, 36);
        let mut melodic = driver.clone();
        let note = on(&mut melodic, 0, 48);
        let pitch = |writes: &[Write]| {
            writes[writes.len() - 2..]
                .iter()
                .map(|&(_, v)| v)
                .collect::<Vec<_>>()
        };
        assert_eq!(pitch(&drum), pitch(&note));
        assert_eq!(on(&mut driver, PERCUSSION, 37), [], "no timbre for key 37");
    }

    #[test]
    fn the_pan_takes_an_output_away_at_either_end() {
        let output = |pan| {
            let mut driver = driver();
            driver.send(&Event::Controller {
                channel: 0,
                controller: 10,
                value: pan,
            });
            on(&mut driver, 0, 60)
                .into_iter()
                .find(|&(register, _)| register == 0xc0)
                .unwrap()
                .1
        };
        assert_eq!(output(64) & (LEFT | RIGHT), LEFT | RIGHT);
        assert_eq!(output(27) & (LEFT | RIGHT), LEFT);
        assert_eq!(output(28) & (LEFT | RIGHT), LEFT | RIGHT);
        assert_eq!(output(100) & (LEFT | RIGHT), RIGHT);
    }

    #[test]
    fn the_carrier_takes_the_loudness_and_a_volume_change_rewrites_it() {
        let mut driver = driver();
        driver.send(&Event::Controller {
            channel: 0,
            controller: 11,
            value: 127,
        });
        let writes = on(&mut driver, 0, 60);
        let carrier = writes.iter().find(|&&(register, _)| register == 0x43).unwrap().1;
        assert_eq!(carrier & 63, 63 - 48, "full loudness leaves level 48");
        driver.send(&Event::Controller {
            channel: 0,
            controller: 7,
            value: 64,
        });
        let quieter = driver.take_writes();
        assert_eq!(registers(&quieter), [0x40, 0x43]);
        let loudness = scale(scale(64, 127), 127);
        assert_eq!(
            quieter[1].1 & 63,
            63 - u8::try_from(48 * u16::from(loudness) / 127).unwrap()
        );
        assert_eq!(
            quieter[0].1, PIANO.modulator.level,
            "the modulator of an FM timbre keeps its level"
        );
    }

    #[test]
    fn the_pedal_holds_a_note_until_it_comes_up() {
        let mut driver = driver();
        driver.send(&Event::Controller {
            channel: 0,
            controller: 64,
            value: 127,
        });
        on(&mut driver, 0, 60);
        assert_eq!(off(&mut driver, 0, 60), []);
        driver.send(&Event::Controller {
            channel: 0,
            controller: 64,
            value: 0,
        });
        assert_eq!(registers(&driver.take_writes()), [0xb0]);
    }

    #[test]
    fn the_19th_note_takes_the_last_channel_of_the_weakest_claim() {
        let mut driver = driver();
        for key in 0..18 {
            on(&mut driver, 0, 40 + key);
        }
        let writes = on(&mut driver, 0, 70);
        assert_eq!(writes[0].0, 0x1b8, "the holder on channel 17 lets go first");
        assert_eq!(
            writes.last().unwrap().0,
            0x1b8,
            "and the new note takes its channel"
        );
        let mut other = driver.clone();
        let writes = on(&mut other, 1, 70);
        assert_eq!(writes, [], "channel 1 has no timbre");
    }

    #[test]
    fn the_bend_moves_the_frequency_by_its_range() {
        let pitch = |bend: u16| {
            let mut driver = driver();
            driver.send(&Event::PitchBend {
                channel: 0,
                value: bend,
            });
            let writes = on(&mut driver, 0, 60);
            let [(_, low), (_, high)] = writes[writes.len() - 2..] else {
                panic!("no frequency");
            };
            (u16::from(high & 0x1f) << 8) | u16::from(low)
        };
        let up = pitch(0x3fff);
        let centre = pitch(0x2000);
        let two_up = {
            let mut driver = driver();
            driver.send(&Event::PitchBend {
                channel: 0,
                value: 0x2000,
            });
            let writes = on(&mut driver, 0, 62);
            let [(_, low), (_, high)] = writes[writes.len() - 2..] else {
                panic!("no frequency");
            };
            (u16::from(high & 0x1f) << 8) | u16::from(low)
        };
        assert!(up > centre);
        assert_eq!(up, two_up, "a full bend up is the range, 2 semitones");
    }
}
