//! The eight slots, the twelve voices and what happens between a request and a sound starting.
//!
//! The original splits the work in two. A request takes a slot and works out the volume and the
//! rate; a later pass hands the slot to one of the twelve voices the library allocated at start-up.
//! A request with no slot free is dropped rather than cutting an older sound short, so the ninth
//! concurrent effect is lost.

use crate::rand::Rand;
use crate::table::{Effect, Effects, LAST_EFFECT};

/// How many slots there are (`0x2165e0`).
pub const SLOTS: usize = 8;

/// How many voices there are (`0x216574`).
pub const VOICES: usize = 12;

/// The percentage every request passes, which the service pass sorts by.
pub const PRIORITY: u16 = 100;

/// The pan every request passes, which is the centre.
pub const CENTRE: u16 = 64;

/// The lowest volume a jittered one is held to.
const QUIETEST: i32 = 0;

/// The highest (`0x7b408`).
const LOUDEST: i32 = 127;

/// The lowest rate a jittered one is held to (`0x7b498`).
const SLOWEST: i32 = 1000;

/// The highest (`0x7b3c4`).
const FASTEST: i32 = 44100;

/// What the jitter amplitude is multiplied by to get the spread.
const SPREAD: i32 = 50;

/// What the spread is divided by, truncating, to get the offset.
const SPREAD_DIVISOR: i32 = 100;

/// What the master volume is divided by after it scales a slot's own.
const MASTER_FULL: u32 = 100;

/// Which switches the library has thrown (`0x216550`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Switches {
    /// The library is started, which is bit 1.
    pub started: bool,
    /// The effects driver went in, bit 3.
    pub driver: bool,
    /// Effects are switched on, bit 6.
    pub effects: bool,
}

impl Switches {
    /// Everything on, which is what a working install gives.
    #[must_use]
    pub const fn ready() -> Self {
        Self {
            started: true,
            driver: true,
            effects: true,
        }
    }

    /// Whether a request gets as far as a slot. `0x7a0b6` wants all three.
    #[must_use]
    pub const fn playable(self) -> bool {
        self.started && self.driver && self.effects
    }
}

/// What a slot is doing, which the original keeps in its first byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// Byte 0: nothing in it.
    #[default]
    Free,
    /// Byte 1: a request filled it and it is waiting for a voice.
    Queued,
    /// Byte 2: a voice has it and the sound has started.
    Playing,
}

/// One of the eight slots a request lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Slot {
    /// What it is doing.
    pub state: State,
    /// Which effect id asked for it.
    pub effect: u8,
    /// The percentage the request passed, which decides the service order.
    pub priority: u16,
    /// The volume after the jitter and the clamp.
    pub volume: u16,
    /// The rate after the jitter and the clamp.
    pub rate: u16,
    /// Where it sits in the stereo field, always the centre.
    pub pan: u16,
    /// Whether it loops, which nothing the game asks for does.
    pub looping: bool,
}

/// One of the twelve voices, each holding a library handle for the life of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Voice {
    /// Which slot it plays for, where it has one.
    pub slot: Option<usize>,
}

/// Why a request got no slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The library is not started, the driver is not in, or effects are off.
    Silent,
    /// The id is 0 or above [`LAST_EFFECT`].
    NoSuchEffect,
    /// All eight slots are busy.
    Busy,
}

/// The effect side of the sound library.
#[derive(Debug, Clone)]
pub struct Mixer {
    /// Which switches are thrown.
    pub switches: Switches,
    /// The table requests read their volume and rate from.
    pub effects: Effects,
    /// The master volume a slot's own is scaled by (`0x1fdf2c`).
    pub master: u16,
    slots: [Slot; SLOTS],
    voices: [Voice; VOICES],
    rand: Rand,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// A library with nothing playing, every switch off and the master volume at full.
    #[must_use]
    pub fn new() -> Self {
        Self {
            switches: Switches::default(),
            effects: Effects::ORIGINAL,
            master: 100,
            slots: [Slot::default(); SLOTS],
            voices: [Voice::default(); VOICES],
            rand: Rand::new(),
        }
    }

    /// The slots as they stand.
    #[must_use]
    pub const fn slots(&self) -> &[Slot; SLOTS] {
        &self.slots
    }

    /// The voices as they stand.
    #[must_use]
    pub const fn voices(&self) -> &[Voice; VOICES] {
        &self.voices
    }

    /// The jitter generator, for a caller that keeps the sequence across a save.
    #[must_use]
    pub const fn rand(&self) -> Rand {
        self.rand
    }

    /// Asks for effect `id` (`0x23b0e` into `0x7a0b6`), giving back which slot took it.
    ///
    /// The volume and the rate are drawn here, so the generator moves on whether or not a voice
    /// is free afterwards.
    ///
    /// # Errors
    ///
    /// [`Refused`] says which of the original's three gates turned the request away.
    pub fn request(&mut self, id: u8) -> Result<usize, Refused> {
        self.queue(id, true)
    }

    /// The same as [`Self::request`] with the table's own volume and rate, no jitter and no
    /// draw, for hearing an effect as its row stands.
    ///
    /// # Errors
    ///
    /// As [`Self::request`].
    pub fn request_unjittered(&mut self, id: u8) -> Result<usize, Refused> {
        self.queue(id, false)
    }

    fn queue(&mut self, id: u8, jitter: bool) -> Result<usize, Refused> {
        if !self.switches.playable() {
            return Err(Refused::Silent);
        }
        if id == 0 || id > LAST_EFFECT {
            return Err(Refused::NoSuchEffect);
        }
        let place = self
            .slots
            .iter()
            .position(|slot| slot.state == State::Free)
            .ok_or(Refused::Busy)?;
        let mut row = self.effects.get(id);
        if !jitter {
            row.jitter = 0;
        }
        let (volume, rate) = self.jittered(row);
        self.slots[place] = Slot {
            state: State::Queued,
            effect: id,
            priority: PRIORITY,
            volume,
            rate,
            pan: CENTRE,
            looping: false,
        };
        Ok(place)
    }

    /// The volume and the rate after the jitter (`0x7a3f2`).
    ///
    /// A volume of 0 in the table becomes 100 and a rate of 0 becomes 11,000, neither of which
    /// any reachable row needs. Above a jitter of 0 each of the two takes its own draw, so they
    /// wander apart.
    fn jittered(&mut self, row: Effect) -> (u16, u16) {
        let volume = if row.volume == 0 {
            100
        } else {
            i32::from(row.volume)
        };
        let rate = if row.rate == 0 {
            11_000
        } else {
            i32::from(row.rate)
        };
        if row.jitter == 0 {
            return (
                clamped(volume, QUIETEST, LOUDEST),
                clamped(rate, SLOWEST, FASTEST),
            );
        }
        let spread = i32::from(row.jitter) * SPREAD;
        let volume = volume + self.offset(spread);
        let rate = rate + self.offset(spread);
        (
            clamped(volume, QUIETEST, LOUDEST),
            clamped(rate, SLOWEST, FASTEST),
        )
    }

    /// One draw's worth of offset, inside plus or minus half the jitter.
    fn offset(&mut self, spread: i32) -> i32 {
        let draw = i32::try_from(self.rand.next()).unwrap_or_default();
        let span = spread * 2;
        let wrapped = if span == 0 { 0 } else { draw % span };
        (wrapped - spread) / SPREAD_DIVISOR
    }

    /// Hands the queued slots to free voices (`0x7a7bf`), giving back how many started.
    pub fn service(&mut self) -> usize {
        let mut started = 0;
        for place in service_order(&self.slots) {
            if self.slots[place].state != State::Queued {
                continue;
            }
            if self.voices.iter().any(|voice| voice.slot == Some(place)) {
                continue;
            }
            let Some(voice) = self.voices.iter().position(|voice| voice.slot.is_none()) else {
                continue;
            };
            self.voices[voice].slot = Some(place);
            self.slots[place].state = State::Playing;
            started += 1;
        }
        started
    }

    /// The volume the library is handed for `place`: the slot's own scaled by the master
    /// (`0x7ab45`).
    #[must_use]
    pub fn played_volume(&self, place: usize) -> u16 {
        let Some(slot) = self.slots.get(place) else {
            return 0;
        };
        let scaled = u32::from(slot.volume) * u32::from(self.master) / MASTER_FULL;
        u16::try_from(scaled).unwrap_or(u16::MAX)
    }

    /// Frees the slot `place` and whatever voice held it, which is what the end of a sound does.
    pub fn finished(&mut self, place: usize) {
        if let Some(slot) = self.slots.get_mut(place) {
            *slot = Slot::default();
        }
        for voice in &mut self.voices {
            if voice.slot == Some(place) {
                voice.slot = None;
            }
        }
    }
}

/// The order the service pass walks the busy slots in (`0x7a962`).
///
/// The original sorts by the slot's priority, lowest first, and on a tie puts a slot that is
/// already playing ahead of a queued one. Every request passes the same priority, so in the
/// shipped game the tie rule is the whole of it.
#[must_use]
pub fn service_order(slots: &[Slot]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..slots.len())
        .filter(|&place| slots[place].state != State::Free)
        .collect();
    order.sort_by_key(|&place| {
        let slot = slots[place];
        (slot.priority, u8::from(slot.state != State::Playing), place)
    });
    order
}

/// `value` held between `least` and `most`.
fn clamped(value: i32, least: i32, most: i32) -> u16 {
    u16::try_from(value.clamp(least, most)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Mixer {
        let mut mixer = Mixer::new();
        mixer.switches = Switches::ready();
        mixer
    }

    #[test]
    fn a_request_needs_every_switch_and_a_real_id() {
        let mut mixer = Mixer::new();
        assert_eq!(mixer.request(1), Err(Refused::Silent));
        mixer.switches = Switches::ready();
        assert_eq!(mixer.request(0), Err(Refused::NoSuchEffect));
        assert_eq!(mixer.request(LAST_EFFECT + 1), Err(Refused::NoSuchEffect));
        mixer.switches.effects = false;
        assert_eq!(mixer.request(1), Err(Refused::Silent));
    }

    #[test]
    fn an_effect_with_no_jitter_plays_its_table_values() {
        let mut mixer = ready();
        assert_eq!(mixer.request(1), Ok(0));
        let slot = mixer.slots()[0];
        assert_eq!(slot.state, State::Queued);
        assert_eq!(slot.volume, 60);
        assert_eq!(slot.rate, 9000);
        assert_eq!(slot.pan, CENTRE);
        assert!(!slot.looping);
        assert_eq!(mixer.rand(), Rand::new(), "no jitter takes no draw");
    }

    #[test]
    fn an_unjittered_request_plays_the_row_and_takes_no_draw() {
        let mut mixer = ready();
        let place = mixer.request_unjittered(0x22).unwrap();
        let row = Effects::ORIGINAL.get(0x22);
        assert_eq!(mixer.slots()[place].volume, u16::from(row.volume));
        assert_eq!(mixer.slots()[place].rate, row.rate);
        assert_eq!(mixer.rand(), Rand::new());
    }

    #[test]
    fn jitter_moves_the_volume_and_the_rate_apart() {
        let mut mixer = ready();
        assert_eq!(mixer.request(46), Ok(0));
        let slot = mixer.slots()[0];
        let mut rand = Rand::new();
        let spread = 80 * SPREAD;
        let first = (i32::try_from(rand.next()).unwrap() % (spread * 2) - spread) / 100;
        let second = (i32::try_from(rand.next()).unwrap() % (spread * 2) - spread) / 100;
        assert_ne!(first, second, "the two draws differ for this id");
        assert_eq!(i32::from(slot.volume), 40 + first);
        assert_eq!(i32::from(slot.rate), 8000 + second);
    }

    #[test]
    fn the_ninth_sound_is_lost_rather_than_cutting_one_short() {
        let mut mixer = ready();
        for place in 0..SLOTS {
            assert_eq!(mixer.request(1), Ok(place));
        }
        assert_eq!(mixer.request(1), Err(Refused::Busy));
        assert!(mixer.slots().iter().all(|slot| slot.state == State::Queued));
    }

    #[test]
    fn the_service_pass_gives_each_queued_slot_a_voice() {
        let mut mixer = ready();
        for _ in 0..SLOTS {
            mixer.request(1).expect("a slot is free");
        }
        assert_eq!(mixer.service(), SLOTS);
        assert!(mixer.slots().iter().all(|slot| slot.state == State::Playing));
        assert_eq!(
            mixer.voices().iter().filter(|voice| voice.slot.is_some()).count(),
            SLOTS
        );
        assert_eq!(mixer.service(), 0, "a second pass starts nothing again");
    }

    #[test]
    fn the_service_order_takes_the_quietest_priority_and_a_playing_slot_first() {
        let slot = |state, priority| Slot {
            state,
            priority,
            ..Slot::default()
        };
        let slots = [
            slot(State::Queued, 100),
            slot(State::Free, 0),
            slot(State::Queued, 40),
            slot(State::Playing, 100),
            slot(State::Playing, 40),
        ];
        assert_eq!(service_order(&slots), [4, 2, 3, 0]);
    }

    #[test]
    fn there_are_more_voices_than_slots_so_nothing_queued_waits() {
        const _: () = assert!(VOICES >= SLOTS, "the voice array cannot run dry");
        let mut mixer = ready();
        for _ in 0..SLOTS {
            mixer.request(1).expect("a slot is free");
        }
        assert_eq!(mixer.service(), SLOTS);
        assert!(mixer.slots().iter().all(|slot| slot.state == State::Playing));
    }

    #[test]
    fn the_master_volume_scales_what_the_library_is_handed() {
        let mut mixer = ready();
        mixer.master = 50;
        mixer.request(1).expect("a slot is free");
        assert_eq!(mixer.played_volume(0), 30);
        mixer.master = 100;
        assert_eq!(mixer.played_volume(0), 60);
        assert_eq!(mixer.played_volume(SLOTS), 0);
    }

    #[test]
    fn a_finished_sound_frees_its_slot_and_its_voice() {
        let mut mixer = ready();
        let place = mixer.request(1).expect("a slot is free");
        mixer.service();
        mixer.finished(place);
        assert_eq!(mixer.slots()[place], Slot::default());
        assert!(mixer.voices().iter().all(|voice| voice.slot.is_none()));
        assert_eq!(mixer.request(1), Ok(place), "the slot comes back");
    }
}
