//! The Miles sequencer linked into `S2.EXE`: one song played a tick at a time, its loops, its
//! queue of notes waiting for their note-off, and its end (`docs/formats/music.md`).
//!
//! Each tick the sequencer counts down the 32 slots of its note queue from the first, sends a
//! note-off for every slot that reaches 0, then sends the stream's events up to the tick. A
//! note-on takes the lowest free slot for its duration; a song that fills all 32 stops. A
//! for-loop and its next-loop jump within the tick, so the events after the for-loop play in the
//! same tick as the next-loop. At the end of the stream the song stops, which sends the note-off
//! of every queued note in slot order, as `0xa3740` does: S2 starts every song with a loop count
//! of 1 (`0xa5054`), so a song without a next-loop plays once.

use s2_formats::music::{Event, MilesControl, Sequence};

/// Slots in the note queue (`+0x558` of the sequence record).
pub const NOTE_SLOTS: usize = 32;

/// Loops that nest; a fifth for-loop is ignored.
const MOST_LOOPS: usize = 4;

/// The lowest next-loop value that jumps back.
const LOOP_JUMP: u8 = 64;

/// A song as the sequencer reads it: the stream's events at their ticks, each note with its
/// duration rather than a note-off of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    steps: Vec<Step>,
    end: u32,
}

/// One event of a [`Score`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    tick: u32,
    event: Event,
    /// For a note-on, how many ticks until its note-off.
    duration: u32,
}

impl Score {
    /// The score of `sequence`. Each note-on takes the first note-off after it on the same
    /// channel and key, or a note-on there with velocity 0, as its end. A note that never ends
    /// lasts until the end of the song. Meta events are left out. The song ends at its
    /// end-of-track, or at its last event other than a note-off when that comes later; a note
    /// still sounding then is cut off.
    #[must_use]
    pub fn new(sequence: &Sequence) -> Self {
        let last = sequence
            .events
            .iter()
            .filter(|timed| !matches!(timed.event, Event::NoteOff { .. }))
            .map(|timed| timed.tick)
            .max()
            .unwrap_or_default();
        let end = sequence.end_tick.max(last);
        let mut steps: Vec<Step> = Vec::with_capacity(sequence.events.len());
        let mut open: Vec<(u8, u8, usize)> = Vec::new();
        for timed in &sequence.events {
            let tick = timed.tick;
            let ends = match timed.event {
                Event::NoteOff { channel, key, .. } => Some((channel, key)),
                Event::NoteOn {
                    channel,
                    key,
                    velocity: 0,
                } if open.iter().any(|&(c, k, _)| (c, k) == (channel, key)) => Some((channel, key)),
                _ => None,
            };
            if let Some(note) = ends
                && let Some(at) = open.iter().position(|&(c, k, _)| (c, k) == note)
            {
                let (_, _, index) = open.remove(at);
                steps[index].duration = tick - steps[index].tick;
                continue;
            }
            if let Event::NoteOn { channel, key, .. } = timed.event {
                open.push((channel, key, steps.len()));
            } else if matches!(timed.event, Event::Meta { .. }) {
                continue;
            }
            steps.push(Step {
                tick,
                event: timed.event.clone(),
                duration: 0,
            });
        }
        for (_, _, index) in open {
            steps[index].duration = end.saturating_sub(steps[index].tick);
        }
        Self { steps, end }
    }

    /// The tick of the end of the stream.
    #[must_use]
    pub const fn end(&self) -> u32 {
        self.end
    }
}

/// One queued note: its channel and key, and the ticks left until its note-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Queued {
    channel: u8,
    key: u8,
    left: i64,
}

/// An open loop: the passes left (0 forever), where the body starts in the score and the tick
/// of its for-loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Open {
    count: u8,
    body: usize,
    at: u32,
}

/// A song playing.
#[derive(Debug, Clone)]
pub struct Sequencer {
    score: Score,
    next: usize,
    /// The ticks played so far.
    now: u64,
    /// How far ahead of the score's own ticks playing has got through jumps back.
    shift: u64,
    queue: [Option<Queued>; NOTE_SLOTS],
    loops: Vec<Open>,
    playing: bool,
    /// The ticks at which a next-loop jumped back.
    jumps: Vec<u64>,
}

impl Sequencer {
    /// Starts `score` from its first event.
    #[must_use]
    pub fn new(score: Score) -> Self {
        Self {
            score,
            next: 0,
            now: 0,
            shift: 0,
            queue: [None; NOTE_SLOTS],
            loops: Vec::new(),
            playing: true,
            jumps: Vec::new(),
        }
    }

    /// The ticks, counted from the song's start, at which a next-loop has jumped back so far.
    #[must_use]
    pub fn jumps(&self) -> &[u64] {
        &self.jumps
    }

    /// Whether the song is still playing.
    #[must_use]
    pub const fn playing(&self) -> bool {
        self.playing
    }

    /// Plays one tick, adding the events it sends to `out` in order.
    pub fn tick(&mut self, out: &mut Vec<Event>) {
        if !self.playing {
            return;
        }
        for slot in &mut self.queue {
            if let Some(note) = slot {
                note.left -= 1;
                if note.left <= 0 {
                    out.push(note_off(note.channel, note.key));
                    *slot = None;
                }
            }
        }
        while let Some(step) = self.score.steps.get(self.next) {
            if u64::from(step.tick) + self.shift > self.now {
                break;
            }
            self.next += 1;
            let (event, duration) = (step.event.clone(), step.duration);
            if !self.play(event, duration, out) {
                return;
            }
        }
        if self.next >= self.score.steps.len() && u64::from(self.score.end) + self.shift <= self.now {
            self.stop(out);
            return;
        }
        self.now += 1;
    }

    /// Stops the song, sending the note-off of every queued note in slot order.
    pub fn stop(&mut self, out: &mut Vec<Event>) {
        for slot in &mut self.queue {
            if let Some(note) = slot.take() {
                out.push(note_off(note.channel, note.key));
            }
        }
        self.playing = false;
    }

    /// Plays one stream event; `false` when it stopped the song.
    fn play(&mut self, event: Event, duration: u32, out: &mut Vec<Event>) -> bool {
        match event {
            Event::NoteOn { channel, key, .. } => {
                let Some(slot) = self.queue.iter_mut().find(|slot| slot.is_none()) else {
                    self.stop(out);
                    return false;
                };
                *slot = Some(Queued {
                    channel,
                    key,
                    left: i64::from(duration),
                });
                out.push(event);
            }
            Event::Miles {
                control: MilesControl::ForLoop,
                value,
                ..
            } => {
                if self.loops.len() < MOST_LOOPS {
                    let at = self.score.steps[self.next - 1].tick;
                    self.loops.push(Open {
                        count: value,
                        body: self.next,
                        at,
                    });
                }
            }
            Event::Miles {
                control: MilesControl::NextLoop,
                value,
                ..
            } => {
                if value >= LOOP_JUMP {
                    self.next_loop();
                }
            }
            Event::Miles {
                control: MilesControl::ClearBeatBar | MilesControl::Callback | MilesControl::CallbackPrefix,
                ..
            }
            | Event::Meta { .. } => {}
            other => out.push(other),
        }
        true
    }

    /// The innermost loop's next-loop: counts it down and jumps back while it has passes left,
    /// always for a loop of 0.
    fn next_loop(&mut self) {
        let Some(open) = self.loops.last_mut() else {
            return;
        };
        if open.count != 0 {
            open.count -= 1;
            if open.count == 0 {
                self.loops.pop();
                return;
            }
        }
        let (body, at) = (open.body, open.at);
        self.next = body;
        self.shift = self.now - u64::from(at);
        self.jumps.push(self.now);
    }
}

/// The note-off Miles sends for a queued note: velocity 0.
const fn note_off(channel: u8, key: u8) -> Event {
    Event::NoteOff {
        channel,
        key,
        velocity: 0,
    }
}

#[cfg(test)]
mod tests {
    use s2_formats::music::TimedEvent;

    use super::*;

    fn on(tick: u32, key: u8) -> TimedEvent {
        TimedEvent {
            tick,
            event: Event::NoteOn {
                channel: 0,
                key,
                velocity: 100,
            },
        }
    }

    fn off(tick: u32, key: u8) -> TimedEvent {
        TimedEvent {
            tick,
            event: Event::NoteOff {
                channel: 0,
                key,
                velocity: 64,
            },
        }
    }

    fn miles(tick: u32, control: MilesControl, value: u8) -> TimedEvent {
        TimedEvent {
            tick,
            event: Event::Miles {
                channel: 10,
                control,
                value,
            },
        }
    }

    fn song(events: Vec<TimedEvent>, end_tick: u32) -> Sequencer {
        Sequencer::new(Score::new(&Sequence {
            timbres: Vec::new(),
            branches: Vec::new(),
            events,
            end_tick,
        }))
    }

    /// Plays `ticks` ticks and gives back the keys switched on and off, by tick.
    fn run(sequencer: &mut Sequencer, ticks: u32) -> Vec<(u32, String)> {
        let mut heard = Vec::new();
        for tick in 0..ticks {
            let mut out = Vec::new();
            sequencer.tick(&mut out);
            for event in out {
                match event {
                    Event::NoteOn { key, .. } => heard.push((tick, format!("on {key}"))),
                    Event::NoteOff { key, .. } => heard.push((tick, format!("off {key}"))),
                    _ => {}
                }
            }
        }
        heard
    }

    #[test]
    fn notes_end_after_their_duration_in_slot_order() {
        let mut sequencer = song(
            vec![
                on(0, 60),
                on(1, 62),
                off(3, 60),
                off(3, 62),
                on(3, 64),
                off(4, 64),
            ],
            6,
        );
        assert_eq!(
            run(&mut sequencer, 8),
            [
                (0, "on 60".into()),
                (1, "on 62".into()),
                (3, "off 60".into()),
                (3, "off 62".into()),
                (3, "on 64".into()),
                (4, "off 64".into()),
            ]
        );
        assert!(!sequencer.playing(), "the song stops at its end");
    }

    #[test]
    fn a_freed_slot_is_taken_first_so_offs_follow_the_slots() {
        let mut sequencer = song(
            vec![
                on(0, 60),
                on(0, 62),
                off(1, 60),
                on(1, 64),
                off(2, 62),
                off(2, 64),
            ],
            3,
        );
        let heard = run(&mut sequencer, 4);
        assert_eq!(
            &heard[4..],
            [(2, "off 64".into()), (2, "off 62".into())],
            "64 took slot 0, which 60 freed, so its off goes before 62's in slot 1"
        );
    }

    #[test]
    fn an_endless_loop_jumps_within_the_tick_and_notes_keep_counting() {
        let mut sequencer = song(
            vec![
                miles(2, MilesControl::ForLoop, 0),
                on(2, 60),
                off(3, 60),
                on(4, 62),
                miles(5, MilesControl::NextLoop, 90),
                off(7, 62),
            ],
            8,
        );
        let heard = run(&mut sequencer, 12);
        assert_eq!(
            heard,
            [
                (2, "on 60".into()),
                (3, "off 60".into()),
                (4, "on 62".into()),
                (5, "on 60".into()),
                (6, "off 60".into()),
                (7, "off 62".into()),
                (7, "on 62".into()),
                (8, "on 60".into()),
                (9, "off 60".into()),
                (10, "off 62".into()),
                (10, "on 62".into()),
                (11, "on 60".into()),
            ],
            "the body after the for-loop plays in the next-loop's tick; 62 keeps its three ticks"
        );
        assert!(sequencer.playing());
    }

    #[test]
    fn a_counted_loop_plays_its_passes_then_carries_on() {
        let mut sequencer = song(
            vec![
                miles(0, MilesControl::ForLoop, 2),
                on(0, 60),
                off(1, 60),
                miles(1, MilesControl::NextLoop, 127),
                on(2, 70),
                off(3, 70),
            ],
            3,
        );
        let ons: Vec<_> = run(&mut sequencer, 6)
            .into_iter()
            .filter(|(_, what)| what.starts_with("on"))
            .collect();
        assert_eq!(
            ons,
            [(0, "on 60".into()), (1, "on 60".into()), (3, "on 70".into())],
            "two passes in all, then the rest a tick later than written"
        );
    }

    #[test]
    fn a_next_loop_below_64_and_a_fifth_for_loop_are_ignored() {
        let mut events = vec![miles(0, MilesControl::NextLoop, 63)];
        events.extend((0..5).map(|_| miles(0, MilesControl::ForLoop, 0)));
        events.push(on(0, 60));
        let mut sequencer = song(events, 2);
        assert_eq!(sequencer.loops.len(), 0);
        run(&mut sequencer, 1);
        assert_eq!(sequencer.loops.len(), MOST_LOOPS);
    }

    #[test]
    fn the_end_sends_the_offs_still_queued_and_a_full_queue_stops_the_song() {
        let mut sequencer = song(vec![on(0, 60), off(9, 60)], 2);
        assert_eq!(
            run(&mut sequencer, 4),
            [(0, "on 60".into()), (2, "off 60".into())]
        );

        let crowd: Vec<_> = (0..=32).map(|key| on(0, key)).collect();
        let mut sequencer = song(crowd, 10);
        let heard = run(&mut sequencer, 2);
        assert_eq!(
            heard.iter().filter(|(_, what)| what.starts_with("on")).count(),
            32
        );
        assert_eq!(
            heard.iter().filter(|(_, what)| what.starts_with("off")).count(),
            32
        );
        assert!(!sequencer.playing());
    }
}
