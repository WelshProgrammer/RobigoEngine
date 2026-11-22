//! XMIDI containers and event streams.

use super::Error;
use super::chunk::{Chunk, Chunks};
use super::midi::{
    CHANNEL_PRESSURE, CHANNELS, CONTROLLER, DATA_MASK, KIND_MASK, META, META_END_OF_TRACK, NOTE_OFF, NOTE_ON,
    PITCH_BEND, POLY_PRESSURE, PROGRAM_CHANGE, STATUS_BIT, SYSEX, SYSEX_ESCAPE,
};
use crate::bytes::{Reader, Truncated, to_usize};

/// XMIDI event times count at this fixed rate, whatever tempo events say.
///
/// A delay byte in the stream waits its value in ticks. A zero byte waits one, as in the Miles
/// sequencer.
pub const TICKS_PER_SECOND: u32 = 120;

/// Velocity given to the note-offs synthesised from XMIDI note durations.
const NOTE_OFF_VELOCITY: u8 = 64;

/// A parsed XMIDI file: one or more independent sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xmidi {
    /// The sequences in file order.
    pub sequences: Vec<Sequence>,
}

/// One XMIDI sequence (a `FORM XMID`), decoded to absolute times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    /// The instruments the sequence uses, from its `TIMB` chunk; empty when there is none.
    pub timbres: Vec<TimbreRef>,
    /// Branch targets from its `RBRN` chunk; empty when there is none.
    pub branches: Vec<Branch>,
    /// Events sorted by tick, then in stream order. Each XMIDI note becomes an
    /// [`Event::NoteOn`] and an [`Event::NoteOff`] `max(duration, 1)` ticks later. Within a
    /// tick, note-offs come first, as the Miles sequencer sends them.
    pub events: Vec<TimedEvent>,
    /// Tick of the end-of-track meta event, or of the last delay when the stream lacks one.
    pub end_tick: u32,
}

/// A bank and patch pair naming one timbre in a [`super::TimbreBank`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimbreRef {
    /// Patch (program) number; for bank 127 this is the percussion key.
    pub patch: u8,
    /// Bank number: 0 for melodic instruments, 127 for percussion in Settlers II.
    pub bank: u8,
}

/// An `RBRN` chunk entry, a target a sequence branch controller can jump to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Branch {
    /// The marker number, matched against the value of [`MilesControl::SequenceBranch`].
    pub id: u16,
    /// Byte offset of the target inside the `EVNT` chunk body.
    pub offset: u32,
}

/// An event at an absolute time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedEvent {
    /// Time in ticks of 1/[`TICKS_PER_SECOND`] s from the start of the sequence.
    pub tick: u32,
    /// What happens.
    pub event: Event,
}

/// A decoded XMIDI event. Channels are zero-based (0..=15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Releases a note; most come from XMIDI note durations.
    NoteOff {
        /// Channel.
        channel: u8,
        /// Key number.
        key: u8,
        /// Release velocity.
        velocity: u8,
    },
    /// Starts a note.
    NoteOn {
        /// Channel.
        channel: u8,
        /// Key number.
        key: u8,
        /// Attack velocity.
        velocity: u8,
    },
    /// Polyphonic key pressure.
    PolyPressure {
        /// Channel.
        channel: u8,
        /// Key number.
        key: u8,
        /// Pressure.
        pressure: u8,
    },
    /// A standard MIDI controller change.
    Controller {
        /// Channel.
        channel: u8,
        /// Controller number, outside the Miles range 108..=119.
        controller: u8,
        /// New value.
        value: u8,
    },
    /// A Miles extension controller (numbers 108..=119).
    Miles {
        /// Channel.
        channel: u8,
        /// Which extension.
        control: MilesControl,
        /// Controller value.
        value: u8,
    },
    /// Selects the patch for a channel, in the bank set by [`MilesControl::PatchBankSelect`].
    ProgramChange {
        /// Channel.
        channel: u8,
        /// Patch number.
        program: u8,
    },
    /// Channel pressure.
    ChannelPressure {
        /// Channel.
        channel: u8,
        /// Pressure.
        pressure: u8,
    },
    /// Pitch wheel.
    PitchBend {
        /// Channel.
        channel: u8,
        /// 14-bit value; 8192 is centre.
        value: u16,
    },
    /// A system exclusive message; the bytes after the length, usually ending in an end-of-exclusive byte.
    SysEx(Vec<u8>),
    /// A system exclusive continuation or escape.
    SysExEscape(Vec<u8>),
    /// A meta event other than end-of-track, which becomes [`Sequence::end_tick`].
    Meta {
        /// Meta type, such as the tempo type.
        kind: u8,
        /// Payload.
        data: Vec<u8>,
    },
}

/// The Miles controller extensions handled by the AIL 3.03 sequencer in S2.EXE.
///
/// The discriminant is the controller number. Older AIL documentation puts the sequence branch
/// on 120; this build uses 109 and passes 120 through as an ordinary controller. The sound
/// driver handles 112..=114.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MilesControl {
    /// The application's prefix callback supplies the value of the next controller on this
    /// channel.
    CallbackPrefix = 108,
    /// Jump to the `RBRN` marker whose id equals the value.
    SequenceBranch = 109,
    /// Values 64 and up lock a physical channel for this sequence; lower values release it.
    ChannelLock = 110,
    /// Values 64 and up protect the channel from being locked by others; lower values unprotect.
    ChannelProtect = 111,
    /// Keep the channel's voices from being stolen (driver-level).
    VoiceProtect = 112,
    /// Keep the channel's timbre cached (driver-level).
    TimbreProtect = 113,
    /// Bank used by the following program changes on this channel (driver-level).
    PatchBankSelect = 114,
    /// The next controller on this channel takes its value from an application array at this
    /// index.
    IndirectPrefix = 115,
    /// Start of a loop. The body after this event plays `value` times in all; 0 means forever.
    /// Up to four loops nest, and a fifth is ignored.
    ForLoop = 116,
    /// End of the innermost loop. Values below 64 are ignored; 64 and up count down that loop
    /// and jump back to the event after its [`MilesControl::ForLoop`] until the count runs out.
    NextLoop = 117,
    /// Reset the beat and bar counters.
    ClearBeatBar = 118,
    /// Call the application's trigger callback with the value.
    Callback = 119,
}

impl MilesControl {
    /// Maps a controller number to its Miles meaning, if it has one.
    #[must_use]
    pub fn from_controller(number: u8) -> Option<Self> {
        Some(match number {
            108 => Self::CallbackPrefix,
            109 => Self::SequenceBranch,
            110 => Self::ChannelLock,
            111 => Self::ChannelProtect,
            112 => Self::VoiceProtect,
            113 => Self::TimbreProtect,
            114 => Self::PatchBankSelect,
            115 => Self::IndirectPrefix,
            116 => Self::ForLoop,
            117 => Self::NextLoop,
            118 => Self::ClearBeatBar,
            119 => Self::Callback,
            _ => return None,
        })
    }

    /// The controller number this extension uses.
    #[must_use]
    pub fn controller(self) -> u8 {
        self as u8
    }
}

impl Event {
    /// The channel of a channel message; `None` for system exclusive and meta events.
    #[must_use]
    pub fn channel(&self) -> Option<u8> {
        match *self {
            Self::NoteOff { channel, .. }
            | Self::NoteOn { channel, .. }
            | Self::PolyPressure { channel, .. }
            | Self::Controller { channel, .. }
            | Self::Miles { channel, .. }
            | Self::ProgramChange { channel, .. }
            | Self::ChannelPressure { channel, .. }
            | Self::PitchBend { channel, .. } => Some(channel),
            Self::SysEx(_) | Self::SysExEscape(_) | Self::Meta { .. } => None,
        }
    }
}

impl Xmidi {
    /// Parses an XMIDI file: either `FORM XDIR` followed by `CAT XMID`, or a lone `FORM XMID`.
    ///
    /// Bytes after the last chunk are ignored, so an XMIDI embedded in a larger container can
    /// be passed with whatever follows it.
    ///
    /// # Errors
    ///
    /// [`Error`] on truncated or malformed chunks or event streams, or when the `XDIR`
    /// sequence count does not match the catalogue.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut top = Chunks::new(data, 0);
        let form = next_chunk(&mut top)?.expect(*b"FORM")?;
        if form.body.starts_with(b"XMID") {
            let sequence = Sequence::parse(&form.form_type(*b"XMID")?, 0)?;
            return Ok(Self {
                sequences: vec![sequence],
            });
        }
        let xdir = form.form_type(*b"XDIR")?;
        let mut xdir_chunks = xdir.children();
        let info = next_chunk_in(&mut xdir_chunks, xdir.body_offset)?.expect(*b"INFO")?;
        let declared = Reader::new(info.body).u16().map_err(|_| Truncated {
            offset: info.body_offset,
            needed: 2,
        })?;
        let cat = next_chunk(&mut top)?.expect(*b"CAT ")?.form_type(*b"XMID")?;
        let sequences = cat
            .children()
            .enumerate()
            .map(|(index, chunk)| {
                let form = chunk?.expect(*b"FORM")?.form_type(*b"XMID")?;
                Sequence::parse(&form, index)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if sequences.len() != usize::from(declared) {
            return Err(Error::SequenceCount {
                declared,
                found: sequences.len(),
            });
        }
        Ok(Self { sequences })
    }
}

fn next_chunk<'a>(chunks: &mut Chunks<'a>) -> Result<Chunk<'a>, Error> {
    let offset = chunks.end_offset();
    next_chunk_in(chunks, offset)
}

fn next_chunk_in<'a>(chunks: &mut Chunks<'a>, offset: usize) -> Result<Chunk<'a>, Error> {
    chunks
        .next()
        .unwrap_or(Err(Error::Truncated(Truncated { offset, needed: 8 })))
}

impl Sequence {
    fn parse(form: &Chunk<'_>, index: usize) -> Result<Self, Error> {
        let mut timbres = Vec::new();
        let mut branches = Vec::new();
        let mut decoded = None;
        for chunk in form.children() {
            let chunk = chunk?;
            match &chunk.id {
                b"TIMB" => timbres = parse_timb(&chunk)?,
                b"RBRN" => branches = parse_rbrn(&chunk)?,
                b"EVNT" => decoded = Some(decode_events(chunk.body, chunk.body_offset)?),
                _ => {}
            }
        }
        let (events, end_tick) = decoded.ok_or(Error::MissingEvents { sequence: index })?;
        Ok(Self {
            timbres,
            branches,
            events,
            end_tick,
        })
    }

    /// Length in ticks: the later of [`Self::end_tick`] and the last event.
    #[must_use]
    pub fn length_ticks(&self) -> u32 {
        self.events
            .last()
            .map_or(self.end_tick, |e| e.tick.max(self.end_tick))
    }

    /// Bit `n` is set when channel `n` carries any channel message.
    #[must_use]
    pub fn channel_mask(&self) -> u16 {
        self.events
            .iter()
            .filter_map(|e| e.event.channel())
            .fold(0, |mask, ch| mask | (1 << (ch % CHANNELS)))
    }
}

fn rebase(base: usize) -> impl Fn(Truncated) -> Truncated {
    move |e| Truncated {
        offset: e.offset.saturating_add(base),
        needed: e.needed,
    }
}

fn parse_timb(chunk: &Chunk<'_>) -> Result<Vec<TimbreRef>, Error> {
    let mut r = Reader::new(chunk.body);
    let count = r.u16().map_err(rebase(chunk.body_offset))?;
    let pairs = r
        .bytes(usize::from(count) * 2)
        .map_err(rebase(chunk.body_offset))?;
    Ok(pairs
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| TimbreRef {
            patch: p[0],
            bank: p[1],
        })
        .collect())
}

fn parse_rbrn(chunk: &Chunk<'_>) -> Result<Vec<Branch>, Error> {
    let mut r = Reader::new(chunk.body);
    let count = r.u16().map_err(rebase(chunk.body_offset))?;
    let entries = r
        .bytes(usize::from(count) * 6)
        .map_err(rebase(chunk.body_offset))?;
    Ok(entries
        .as_chunks::<6>()
        .0
        .iter()
        .map(|e| Branch {
            id: u16::from_le_bytes([e[0], e[1]]),
            offset: u32::from_le_bytes([e[2], e[3], e[4], e[5]]),
        })
        .collect())
}

/// An event plus the keys that put it in playback order.
struct Pending {
    tick: u32,
    /// 0 for note-offs, which Miles sends before a tick's stream events; 1 for the rest.
    class: u8,
    event: Event,
}

/// A cursor over an event stream that reports absolute offsets.
pub(super) struct EventReader<'a> {
    pub(super) r: Reader<'a>,
    base: usize,
    /// Whether a note-on carries its duration, as in XMIDI, or stands alone, as in SMF.
    durations: bool,
}

impl<'a> EventReader<'a> {
    /// Reads `body`, which starts `base` bytes into the file.
    pub(super) fn new(body: &'a [u8], base: usize, durations: bool) -> Self {
        Self {
            r: Reader::new(body),
            base,
            durations,
        }
    }
}

impl EventReader<'_> {
    pub(super) fn offset(&self) -> usize {
        self.base.saturating_add(self.r.position())
    }

    pub(super) fn byte(&mut self) -> Result<u8, Error> {
        let offset = self.offset();
        self.r
            .u8()
            .map_err(|_| Error::Truncated(Truncated { offset, needed: 1 }))
    }

    fn data(&mut self) -> Result<u8, Error> {
        let offset = self.offset();
        let byte = self.byte()?;
        if byte & STATUS_BIT == 0 {
            Ok(byte)
        } else {
            Err(Error::BadDataByte { offset, byte })
        }
    }

    pub(super) fn varint(&mut self) -> Result<u32, Error> {
        let offset = self.offset();
        let mut value = 0u32;
        for _ in 0..4 {
            let byte = self.byte()?;
            value = (value << 7) | u32::from(byte & DATA_MASK);
            if byte & STATUS_BIT == 0 {
                return Ok(value);
            }
        }
        Err(Error::BadVarint { offset })
    }

    fn blob(&mut self) -> Result<Vec<u8>, Error> {
        let len = to_usize(self.varint()?);
        let offset = self.offset();
        let bytes = self.r.bytes(len).map_err(|_| Truncated { offset, needed: len })?;
        Ok(bytes.to_vec())
    }
}

/// One decoded message before it is placed in time.
pub(super) enum Message {
    Event(Event),
    Note {
        channel: u8,
        key: u8,
        velocity: u8,
        duration: u32,
    },
    End,
}

impl EventReader<'_> {
    pub(super) fn message(&mut self, status: u8, offset: usize) -> Result<Message, Error> {
        let channel = status % CHANNELS;
        let event = match status & KIND_MASK {
            NOTE_OFF => Event::NoteOff {
                channel,
                key: self.data()?,
                velocity: self.data()?,
            },
            NOTE_ON if !self.durations => Event::NoteOn {
                channel,
                key: self.data()?,
                velocity: self.data()?,
            },
            NOTE_ON => {
                return Ok(Message::Note {
                    channel,
                    key: self.data()?,
                    velocity: self.data()?,
                    duration: self.varint()?,
                });
            }
            POLY_PRESSURE => Event::PolyPressure {
                channel,
                key: self.data()?,
                pressure: self.data()?,
            },
            CONTROLLER => {
                let controller = self.data()?;
                let value = self.data()?;
                match MilesControl::from_controller(controller) {
                    Some(control) => Event::Miles {
                        channel,
                        control,
                        value,
                    },
                    None => Event::Controller {
                        channel,
                        controller,
                        value,
                    },
                }
            }
            PROGRAM_CHANGE => Event::ProgramChange {
                channel,
                program: self.data()?,
            },
            CHANNEL_PRESSURE => Event::ChannelPressure {
                channel,
                pressure: self.data()?,
            },
            PITCH_BEND => {
                let lsb = self.data()?;
                let msb = self.data()?;
                Event::PitchBend {
                    channel,
                    value: u16::from(lsb) | (u16::from(msb) << 7),
                }
            }
            _ => match status {
                SYSEX => Event::SysEx(self.blob()?),
                SYSEX_ESCAPE => Event::SysExEscape(self.blob()?),
                META => {
                    let kind = self.byte()?;
                    let data = self.blob()?;
                    if kind == META_END_OF_TRACK {
                        return Ok(Message::End);
                    }
                    Event::Meta { kind, data }
                }
                byte => return Err(Error::BadStatus { offset, byte }),
            },
        };
        Ok(Message::Event(event))
    }
}

fn decode_events(body: &[u8], base: usize) -> Result<(Vec<TimedEvent>, u32), Error> {
    let mut input = EventReader::new(body, base, true);
    let mut pending = Vec::new();
    let mut tick = 0u32;
    while !input.r.is_empty() {
        let offset = input.offset();
        let status = input.byte()?;
        if status < STATUS_BIT {
            // A data byte in status position is a delay in ticks; a zero waits one, as in the
            // Miles sequencer.
            tick = tick
                .checked_add(u32::from(status.max(1)))
                .ok_or(Error::TickOverflow { offset })?;
            continue;
        }
        let event = match input.message(status, offset)? {
            Message::Event(event) => event,
            Message::Note {
                channel,
                key,
                velocity,
                duration,
            } => {
                pending.push(Pending {
                    tick: tick
                        .checked_add(duration.max(1))
                        .ok_or(Error::TickOverflow { offset })?,
                    class: 0,
                    event: Event::NoteOff {
                        channel,
                        key,
                        velocity: NOTE_OFF_VELOCITY,
                    },
                });
                Event::NoteOn {
                    channel,
                    key,
                    velocity,
                }
            }
            Message::End => break,
        };
        pending.push(Pending {
            tick,
            class: 1,
            event,
        });
    }
    pending.sort_by_key(|p| (p.tick, p.class));
    let events = pending
        .into_iter()
        .map(|p| TimedEvent {
            tick: p.tick,
            event: p.event,
        })
        .collect();
    Ok((events, tick))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    fn form(kind: [u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        body.extend(children.concat());
        chunk(*b"FORM", &body)
    }

    fn sequence(evnt: &[u8]) -> Vec<u8> {
        form(
            *b"XMID",
            &[chunk(*b"TIMB", &[2, 0, 48, 0, 41, 127]), chunk(*b"EVNT", evnt)],
        )
    }

    fn xdir(sequences: &[Vec<u8>]) -> Vec<u8> {
        let count = u16::try_from(sequences.len()).unwrap().to_le_bytes();
        let mut out = form(*b"XDIR", &[chunk(*b"INFO", &count)]);
        let mut cat = b"XMID".to_vec();
        cat.extend(sequences.concat());
        out.extend(chunk(*b"CAT ", &cat));
        out
    }

    const EVENTS: &[u8] = &[
        0xC0, 48, // program
        0xB9, 116, 0, // for loop forever
        0x90, 60, 100, 0x81, 0x00, // note, 128 ticks
        0x7f, 0x01, // delay 128
        0x90, 60, 90, 0x10, // same key restruck as the first ends
        0x99, 41, 127, 0,    // zero-length drum, which Miles holds for one tick
        0x10, // delay 16
        0xB9, 117, 127, // next
        0xFF, 0x51, 3, 0x07, 0xA1, 0x20, // tempo
        0xF0, 3, 0x7e, 0x7f, 0xf7, // sysex
        0xE0, 0x00, 0x40, // pitch bend centre
        0xFF, 0x2F, 0,    // end
        0x90, // ignored after end of track
    ];

    #[test]
    fn decodes_xdir_container() {
        let xmidi = Xmidi::parse(&xdir(&[sequence(EVENTS), sequence(&[0xFF, 0x2F, 0])])).unwrap();
        assert_eq!(xmidi.sequences.len(), 2);
        let seq = &xmidi.sequences[0];
        assert_eq!(
            seq.timbres,
            [
                TimbreRef { patch: 48, bank: 0 },
                TimbreRef { patch: 41, bank: 127 }
            ]
        );
        assert_eq!(seq.end_tick, 144);
        assert_eq!(seq.length_ticks(), 144);
        assert_eq!(seq.channel_mask(), 0b10_0000_0001);
        let at = |tick: u32| -> Vec<&Event> {
            seq.events
                .iter()
                .filter(|e| e.tick == tick)
                .map(|e| &e.event)
                .collect()
        };
        assert_eq!(at(0).len(), 3);
        assert_eq!(
            at(128),
            [
                &Event::NoteOff {
                    channel: 0,
                    key: 60,
                    velocity: NOTE_OFF_VELOCITY
                },
                &Event::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 90
                },
                &Event::NoteOn {
                    channel: 9,
                    key: 41,
                    velocity: 127
                },
            ]
        );
        assert_eq!(
            at(129),
            [&Event::NoteOff {
                channel: 9,
                key: 41,
                velocity: NOTE_OFF_VELOCITY
            }]
        );
        let last = at(144);
        assert_eq!(last.len(), 5);
        assert_eq!(
            last[0],
            &Event::NoteOff {
                channel: 0,
                key: 60,
                velocity: NOTE_OFF_VELOCITY
            }
        );
        assert_eq!(
            last[1],
            &Event::Miles {
                channel: 9,
                control: MilesControl::NextLoop,
                value: 127
            }
        );
        assert_eq!(last[3], &Event::SysEx(vec![0x7e, 0x7f, 0xf7]));
        assert_eq!(
            last[4],
            &Event::PitchBend {
                channel: 0,
                value: 8192
            }
        );
    }

    #[test]
    fn decodes_lone_form() {
        let xmidi = Xmidi::parse(&sequence(&[0x05, 0x00, 0x05, 0xFF, 0x2F, 0])).unwrap();
        assert_eq!(xmidi.sequences.len(), 1);
        assert_eq!(xmidi.sequences[0].end_tick, 11);
        assert!(
            xmidi.sequences[0].events.is_empty(),
            "{:?}",
            xmidi.sequences[0].events
        );
    }

    #[test]
    fn reads_branches() {
        let data = form(
            *b"XMID",
            &[chunk(*b"RBRN", &[1, 0, 7, 0, 16, 0, 0, 0]), chunk(*b"EVNT", &[])],
        );
        let xmidi = Xmidi::parse(&data).unwrap();
        assert_eq!(xmidi.sequences[0].branches, [Branch { id: 7, offset: 16 }]);
    }

    #[test]
    fn rejects_count_mismatch_and_missing_events() {
        let mut data = xdir(&[sequence(&[])]);
        data[20] = 2;
        assert_eq!(
            Xmidi::parse(&data),
            Err(Error::SequenceCount {
                declared: 2,
                found: 1
            })
        );
        let data = form(*b"XMID", &[chunk(*b"TIMB", &[0, 0])]);
        assert_eq!(Xmidi::parse(&data), Err(Error::MissingEvents { sequence: 0 }));
    }

    #[test]
    fn rejects_bad_events() {
        let lone = |evnt: &[u8]| Xmidi::parse(&form(*b"XMID", &[chunk(*b"EVNT", evnt)]));
        assert!(matches!(lone(&[0xF3]), Err(Error::BadStatus { byte: 0xF3, .. })));
        assert!(matches!(
            lone(&[0x90, 0x80, 1, 1]),
            Err(Error::BadDataByte { byte: 0x80, .. })
        ));
        assert!(matches!(
            lone(&[0x90, 1, 1, 0xff, 0xff, 0xff, 0xff, 0x7f]),
            Err(Error::BadVarint { .. })
        ));
        assert!(matches!(lone(&[0x90, 1]), Err(Error::Truncated(_))));
        assert!(matches!(lone(&[0xF0, 0x7f, 1]), Err(Error::Truncated(_))));
    }

    #[test]
    fn never_panics_on_prefixes_or_corruption() {
        let data = xdir(&[sequence(EVENTS)]);
        for len in 0..data.len() {
            let _ = Xmidi::parse(&data[..len]);
        }
        for i in 0..data.len() {
            for flip in [1, 1 << 7, u8::MAX] {
                let mut bad = data.clone();
                bad[i] ^= flip;
                let _ = Xmidi::parse(&bad);
            }
        }
    }
}
