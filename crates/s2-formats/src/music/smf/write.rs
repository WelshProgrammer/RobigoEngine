//! Rendering a decoded sequence as a Standard MIDI File.

use super::super::midi::{
    CHANNEL_PRESSURE, CHANNELS, CONTROLLER, DATA_MASK, META, META_END_OF_TRACK, META_MARKER, META_TEMPO,
    META_TEXT, NOTE_OFF, NOTE_ON, PITCH_BEND, POLY_PRESSURE, PROGRAM_CHANGE, STATUS_BIT, SYSEX, SYSEX_ESCAPE,
};
use super::super::xmidi::{Event, MilesControl, Sequence};
use super::{DIVISION, HEADER_LEN, LOOP_END, LOOP_START, MAX_DELTA, TEMPO_US_PER_QUARTER};

impl Sequence {
    /// Renders the sequence as a format 0 Standard MIDI File that plays at the same speed.
    ///
    /// The file uses 60 ticks per quarter note and a single 500,000 µs tempo, so one SMF tick
    /// equals one XMIDI tick (1/120 s). Changes from the XMIDI stream:
    ///
    /// - Note durations become explicit note-off messages with velocity 64.
    /// - XMIDI tempo events are dropped: Miles ignores them and the delays already encode them.
    /// - Miles controllers 108..=119 stay as controller messages, which a General MIDI synth
    ///   ignores. A for-loop (116) also gets a `loopStart` marker just before it, and a
    ///   next-loop (117) with a value of 64 or more a `loopEnd` marker, which several MIDI
    ///   players honour for looping.
    /// - The end-of-track event sits at [`Sequence::length_ticks`].
    ///
    /// [`Sequence::from_smf`] reads the file back to the same events, the markers and the one
    /// tempo event aside.
    #[must_use]
    pub fn to_smf(&self) -> Vec<u8> {
        let mut track = Track::default();
        track.meta(0, META_TEMPO, &TEMPO_US_PER_QUARTER.to_be_bytes()[1..]);
        for timed in &self.events {
            track.event(timed.tick, &timed.event);
        }
        track.meta(self.length_ticks(), META_END_OF_TRACK, &[]);

        let mut out = Vec::with_capacity(track.bytes.len() + 22);
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&HEADER_LEN.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&DIVISION.to_be_bytes());
        out.extend_from_slice(b"MTrk");
        let len = u32::try_from(track.bytes.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&track.bytes);
        out
    }
}

#[derive(Default)]
struct Track {
    bytes: Vec<u8>,
    tick: u32,
}

impl Track {
    fn delta(&mut self, tick: u32) {
        let mut delta = tick.saturating_sub(self.tick);
        self.tick = self.tick.max(tick);
        while delta > MAX_DELTA {
            // A delta past the 4-byte varint limit goes out in full steps, each carrying an empty
            // text meta event.
            write_varint(&mut self.bytes, MAX_DELTA);
            self.bytes.extend_from_slice(&[META, META_TEXT, 0]);
            delta -= MAX_DELTA;
        }
        write_varint(&mut self.bytes, delta);
    }

    fn channel(&mut self, tick: u32, status: u8, channel: u8, data: &[u8]) {
        self.delta(tick);
        self.bytes.push(status | (channel % CHANNELS));
        self.bytes.extend_from_slice(data);
    }

    fn meta(&mut self, tick: u32, kind: u8, data: &[u8]) {
        self.delta(tick);
        self.bytes.extend_from_slice(&[META, kind]);
        self.blob(data);
    }

    fn blob(&mut self, data: &[u8]) {
        write_varint(&mut self.bytes, u32::try_from(data.len()).unwrap_or(MAX_DELTA));
        self.bytes.extend_from_slice(data);
    }

    fn event(&mut self, tick: u32, event: &Event) {
        match *event {
            Event::NoteOff {
                channel,
                key,
                velocity,
            } => self.channel(tick, NOTE_OFF, channel, &[key, velocity]),
            Event::NoteOn {
                channel,
                key,
                velocity,
            } => self.channel(tick, NOTE_ON, channel, &[key, velocity]),
            Event::PolyPressure {
                channel,
                key,
                pressure,
            } => self.channel(tick, POLY_PRESSURE, channel, &[key, pressure]),
            Event::Controller {
                channel,
                controller,
                value,
            } => self.channel(tick, CONTROLLER, channel, &[controller, value]),
            Event::ProgramChange { channel, program } => {
                self.channel(tick, PROGRAM_CHANGE, channel, &[program]);
            }
            Event::ChannelPressure { channel, pressure } => {
                self.channel(tick, CHANNEL_PRESSURE, channel, &[pressure]);
            }
            Event::PitchBend { channel, value } => {
                let [lsb, msb] = [low7(u32::from(value)), low7(u32::from(value >> 7))];
                self.channel(tick, PITCH_BEND, channel, &[lsb, msb]);
            }
            Event::Miles {
                channel,
                control,
                value,
            } => {
                match control {
                    MilesControl::ForLoop => self.meta(tick, META_MARKER, LOOP_START),
                    MilesControl::NextLoop if value >= super::LOOP_JUMP => {
                        self.meta(tick, META_MARKER, LOOP_END);
                    }
                    _ => {}
                }
                self.channel(tick, CONTROLLER, channel, &[control.controller(), value]);
            }
            Event::SysEx(ref data) => {
                self.delta(tick);
                self.bytes.push(SYSEX);
                self.blob(data);
            }
            Event::SysExEscape(ref data) => {
                self.delta(tick);
                self.bytes.push(SYSEX_ESCAPE);
                self.blob(data);
            }
            Event::Meta { kind, ref data } => {
                if kind != META_TEMPO {
                    self.meta(tick, kind, data);
                }
            }
        }
    }
}

fn write_varint(out: &mut Vec<u8>, value: u32) {
    let value = value.min(MAX_DELTA);
    let mut shift = 21;
    while shift > 0 && value >> shift == 0 {
        shift -= 7;
    }
    while shift > 0 {
        out.push(STATUS_BIT | low7(value >> shift));
        shift -= 7;
    }
    out.push(low7(value));
}

fn low7(value: u32) -> u8 {
    u8::try_from(value & u32::from(DATA_MASK)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::TimedEvent;

    #[test]
    fn writes_varints() {
        let enc = |v| {
            let mut out = Vec::new();
            write_varint(&mut out, v);
            out
        };
        assert_eq!(enc(0), [0]);
        assert_eq!(enc(127), [0x7f]);
        assert_eq!(enc(128), [0x81, 0x00]);
        assert_eq!(enc((1 << 14) - 1), [0xff, 0x7f]);
        assert_eq!(enc(MAX_DELTA), [0xff, 0xff, 0xff, 0x7f]);
    }

    #[test]
    fn renders_format_0() {
        let seq = Sequence {
            timbres: vec![],
            branches: vec![],
            events: vec![
                TimedEvent {
                    tick: 0,
                    event: Event::Miles {
                        channel: 0,
                        control: MilesControl::ForLoop,
                        value: 0,
                    },
                },
                TimedEvent {
                    tick: 0,
                    event: Event::Meta {
                        kind: META_TEMPO,
                        data: vec![9, 39, 192],
                    },
                },
                TimedEvent {
                    tick: 0,
                    event: Event::NoteOn {
                        channel: 2,
                        key: 60,
                        velocity: 100,
                    },
                },
                TimedEvent {
                    tick: 200,
                    event: Event::NoteOff {
                        channel: 2,
                        key: 60,
                        velocity: 64,
                    },
                },
                TimedEvent {
                    tick: 200,
                    event: Event::Miles {
                        channel: 0,
                        control: MilesControl::ChannelLock,
                        value: 1,
                    },
                },
            ],
            end_tick: 210,
        };
        let smf = seq.to_smf();
        let mut expected = b"MThd\0\0\0\x06\0\0\0\x01\0\x3cMTrk".to_vec();
        let track: &[u8] = &[
            0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // tempo
            0x00, 0xFF, 0x06, 0x09, b'l', b'o', b'o', b'p', b'S', b't', b'a', b'r', b't', //
            0x00, 0xB0, 116, 0, // for-loop
            0x00, 0x92, 60, 100, // note on
            0x81, 0x48, 0x82, 60, 64, // note off after 200
            0x00, 0xB0, 110, 1, // channel lock
            0x0a, 0xFF, 0x2F, 0x00, // end
        ];
        expected.extend_from_slice(&u32::try_from(track.len()).unwrap().to_be_bytes());
        expected.extend_from_slice(track);
        assert_eq!(smf, expected);
    }

    #[test]
    fn splits_huge_deltas() {
        let seq = Sequence {
            timbres: vec![],
            branches: vec![],
            events: vec![],
            end_tick: u32::MAX,
        };
        let smf = seq.to_smf();
        let filler = smf
            .windows(7)
            .filter(|w| *w == [0xff, 0xff, 0xff, 0x7f, 0xff, 0x01, 0x00]);
        assert_eq!(filler.count(), 16);
    }
}
