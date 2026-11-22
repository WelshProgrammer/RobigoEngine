//! Reading a Standard MIDI File back into a sequence.

use super::super::Error;
use super::super::chunk::Chunks;
use super::super::midi::{META, META_TEMPO, STATUS_BIT, SYSEX, SYSEX_ESCAPE};
use super::super::xmidi::{Event, EventReader, Message, Sequence, TICKS_PER_SECOND, TimedEvent};
use super::{HEADER_LEN, TEMPO_US_PER_QUARTER};
use crate::bytes::{Reader, Truncated, to_usize};

/// The division's top bit, set when it counts SMPTE frames instead of ticks per quarter note.
const SMPTE: u16 = 1 << 15;

/// The one format with independent tracks, which has no single timeline to read.
const SEQUENCES: u16 = 2;

const MICROSECONDS_PER_SECOND: u128 = 1_000_000;

/// One event read from a track, at its time in the file's own ticks; `None` ends the track.
struct Read {
    tick: u64,
    track: usize,
    offset: usize,
    event: Option<Event>,
}

impl Sequence {
    /// Reads a format 0 or format 1 Standard MIDI File whose division counts ticks per quarter
    /// note.
    ///
    /// Times come out in ticks of 1/[`TICKS_PER_SECOND`] s, rounded to the nearest, following
    /// the file's tempo events, which stay in the events as [`Event::Meta`]. A format 1 file's
    /// tracks merge by time, an earlier track first within a tick, and each track keeps its own
    /// order. Controllers 108..=119 come back as [`Event::Miles`]. The sequence has no timbres
    /// or branches, and [`Sequence::end_tick`] is the latest end of track.
    ///
    /// Chunks other than `MThd` and `MTrk` are skipped, running status is honoured, and a track
    /// with no end-of-track event ends at its last event.
    ///
    /// # Errors
    ///
    /// [`Error::UnexpectedChunk`] when the file does not open with `MThd`, [`Error::SmfFormat`]
    /// for format 2, [`Error::SmfDivision`] for SMPTE time or a division of zero,
    /// [`Error::TickOverflow`] for a time past 32 bits of ticks, and the event errors
    /// [`super::super::Xmidi::parse`] gives.
    pub fn from_smf(data: &[u8]) -> Result<Self, Error> {
        let mut chunks = Chunks::new(data, 0);
        let header = chunks
            .next()
            .unwrap_or(Err(Error::Truncated(Truncated { offset: 0, needed: 8 })))?
            .expect(*b"MThd")?;
        let mut fields = Reader::new(header.body);
        let short = |_| Truncated {
            offset: header.body_offset,
            needed: to_usize(HEADER_LEN),
        };
        let format = fields.u16_be().map_err(short)?;
        fields.u16_be().map_err(short)?;
        let division = fields.u16_be().map_err(short)?;
        if format == SEQUENCES {
            return Err(Error::SmfFormat { format });
        }
        if division == 0 || division & SMPTE != 0 {
            return Err(Error::SmfDivision { division });
        }
        let mut read = Vec::new();
        for (track, chunk) in chunks
            .filter(|chunk| chunk.as_ref().map_or(true, |chunk| chunk.id == *b"MTrk"))
            .enumerate()
        {
            let chunk = chunk?;
            read_track(chunk.body, chunk.body_offset, track, &mut read)?;
        }
        read.sort_by_key(|event| (event.tick, event.track));
        let mut clock = Clock::new(division);
        let mut events = Vec::with_capacity(read.len());
        let mut end_tick = 0;
        for Read {
            tick, offset, event, ..
        } in read
        {
            let at = clock.at(tick).ok_or(Error::TickOverflow { offset })?;
            end_tick = end_tick.max(at);
            let Some(event) = event else { continue };
            if let Event::Meta {
                kind: META_TEMPO,
                data,
            } = &event
                && let [high, middle, low] = data[..]
            {
                clock.tempo = u32::from_be_bytes([0, high, middle, low]);
            }
            events.push(TimedEvent { tick: at, event });
        }
        Ok(Self {
            timbres: Vec::new(),
            branches: Vec::new(),
            events,
            end_tick,
        })
    }
}

/// Reads one track's events into `read`, ending with its end-of-track event, or with an end at
/// its last event when it has none.
fn read_track(body: &[u8], base: usize, track: usize, read: &mut Vec<Read>) -> Result<(), Error> {
    let mut input = EventReader::new(body, base, false);
    let mut tick = 0u64;
    let mut running = None;
    while !input.r.is_empty() {
        tick += u64::from(input.varint()?);
        let offset = input.offset();
        let mut status = input.byte()?;
        if status < STATUS_BIT {
            status = running.ok_or(Error::BadStatus { offset, byte: status })?;
            input.r.seek(input.r.position() - 1)?;
        }
        running = match status {
            SYSEX | SYSEX_ESCAPE | META => None,
            _ => Some(status),
        };
        let event = match input.message(status, offset)? {
            Message::Event(event) => event,
            Message::Note {
                channel,
                key,
                velocity,
                ..
            } => Event::NoteOn {
                channel,
                key,
                velocity,
            },
            Message::End => break,
        };
        read.push(Read {
            tick,
            track,
            offset,
            event: Some(event),
        });
    }
    read.push(Read {
        tick,
        track,
        offset: input.offset(),
        event: None,
    });
    Ok(())
}

/// Turns the file's ticks into 120 Hz ticks as the tempo changes.
struct Clock {
    division: u128,
    tempo: u32,
    /// The file tick of the last conversion.
    tick: u64,
    /// Time up to [`Clock::tick`], in microseconds times the division.
    elapsed: u128,
}

impl Clock {
    fn new(division: u16) -> Self {
        Self {
            division: u128::from(division),
            tempo: TEMPO_US_PER_QUARTER,
            tick: 0,
            elapsed: 0,
        }
    }

    /// The 120 Hz tick of file tick `tick`, which is no earlier than the last one asked for, at
    /// the tempo set since; `None` past 32 bits.
    fn at(&mut self, tick: u64) -> Option<u32> {
        let since = u128::from(tick.saturating_sub(self.tick));
        self.elapsed = self
            .elapsed
            .saturating_add(since.saturating_mul(u128::from(self.tempo)));
        self.tick = self.tick.max(tick);
        let unit = self.division * MICROSECONDS_PER_SECOND;
        let scaled = self.elapsed.saturating_mul(u128::from(TICKS_PER_SECOND));
        u32::try_from(scaled.saturating_add(unit / 2) / unit).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::MilesControl;
    use crate::music::midi::{META_END_OF_TRACK, META_MARKER};

    fn smf(format: u16, division: u16, tracks: &[&[u8]]) -> Vec<u8> {
        let mut out = b"MThd".to_vec();
        out.extend_from_slice(&HEADER_LEN.to_be_bytes());
        out.extend_from_slice(&format.to_be_bytes());
        out.extend_from_slice(&u16::try_from(tracks.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&division.to_be_bytes());
        for track in tracks {
            out.extend_from_slice(b"MTrk");
            out.extend_from_slice(&u32::try_from(track.len()).unwrap().to_be_bytes());
            out.extend_from_slice(track);
        }
        out
    }

    fn timed(tick: u32, event: Event) -> TimedEvent {
        TimedEvent { tick, event }
    }

    fn note_on(channel: u8, key: u8) -> Event {
        Event::NoteOn {
            channel,
            key,
            velocity: 100,
        }
    }

    #[test]
    fn reads_back_what_the_writer_wrote() {
        let events = vec![
            timed(
                0,
                Event::Miles {
                    channel: 10,
                    control: MilesControl::ForLoop,
                    value: 0,
                },
            ),
            timed(0, Event::SysEx(vec![67, 16, 76, 0, 0, 126, 0, 247])),
            timed(0, note_on(2, 60)),
            timed(
                3,
                Event::PitchBend {
                    channel: 2,
                    value: 9000,
                },
            ),
            timed(
                200,
                Event::NoteOff {
                    channel: 2,
                    key: 60,
                    velocity: 64,
                },
            ),
            timed(
                200,
                Event::Miles {
                    channel: 10,
                    control: MilesControl::NextLoop,
                    value: 90,
                },
            ),
        ];
        let written = Sequence {
            timbres: Vec::new(),
            branches: Vec::new(),
            events: events.clone(),
            end_tick: 210,
        };
        let read = Sequence::from_smf(&written.to_smf()).unwrap();
        let kept: Vec<_> = read
            .events
            .into_iter()
            .filter(|e| {
                !matches!(
                    e.event,
                    Event::Meta {
                        kind: META_TEMPO | META_MARKER,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(kept, events);
        assert_eq!(read.end_tick, 210);
    }

    #[test]
    fn follows_running_status() {
        let track: &[u8] = &[0, 0x90, 60, 100, 10, 62, 100, 0, 0xFF, META_END_OF_TRACK, 0];
        let read = Sequence::from_smf(&smf(0, 60, &[track])).unwrap();
        assert_eq!(read.events, [timed(0, note_on(0, 60)), timed(10, note_on(0, 62))]);
    }

    #[test]
    fn a_data_byte_with_nothing_to_run_on_is_refused() {
        let track: &[u8] = &[0, 60, 100];
        assert!(matches!(
            Sequence::from_smf(&smf(0, 60, &[track])),
            Err(Error::BadStatus { byte: 60, .. })
        ));
    }

    #[test]
    fn merges_tracks_and_follows_the_tempo() {
        // 96 ticks a quarter: 500,000 µs a quarter for the first 96 ticks, then 250,000.
        let tempo: &[u8] = &[
            0,
            0xFF,
            META_TEMPO,
            3,
            7,
            161,
            32,
            96,
            0xFF,
            META_TEMPO,
            3,
            3,
            208,
            144,
            0,
            0xFF,
            META_END_OF_TRACK,
            0,
        ];
        let notes: &[u8] = &[0, 0x91, 60, 100, 0x60, 0x91, 62, 100, 0x60, 0x91, 64, 100];
        let read = Sequence::from_smf(&smf(1, 96, &[tempo, notes])).unwrap();
        let notes: Vec<_> = read
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::NoteOn { .. }))
            .cloned()
            .collect();
        assert_eq!(
            notes,
            [
                timed(0, note_on(1, 60)),
                timed(60, note_on(1, 62)),
                timed(90, note_on(1, 64))
            ]
        );
        assert_eq!(read.end_tick, 90);
    }

    #[test]
    fn refuses_what_it_does_not_take() {
        assert!(matches!(
            Sequence::from_smf(&smf(2, 60, &[])),
            Err(Error::SmfFormat { format: 2 })
        ));
        assert!(matches!(
            Sequence::from_smf(&smf(0, SMPTE + 25, &[])),
            Err(Error::SmfDivision { .. })
        ));
        assert!(matches!(
            Sequence::from_smf(&smf(0, 0, &[])),
            Err(Error::SmfDivision { division: 0 })
        ));
        assert!(matches!(
            Sequence::from_smf(b"RIFF\0\0\0\x06\0\0\0\x01\0\x3c"),
            Err(Error::UnexpectedChunk { .. })
        ));
    }

    #[test]
    fn never_panics_on_prefixes_or_corruption() {
        let track: &[u8] = &[
            0,
            0x90,
            60,
            100,
            10,
            62,
            0,
            0,
            0xF0,
            2,
            1,
            247,
            0,
            0xFF,
            META_END_OF_TRACK,
            0,
        ];
        let data = smf(1, 60, &[track, track]);
        for len in 0..data.len() {
            let _ = Sequence::from_smf(&data[..len]);
        }
        for i in 0..data.len() {
            for flip in [1, 1 << 7, u8::MAX] {
                let mut bad = data.clone();
                bad[i] ^= flip;
                let _ = Sequence::from_smf(&bad);
            }
        }
    }
}
