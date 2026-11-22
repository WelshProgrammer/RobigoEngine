//! The ambient sounds the view asks for once a frame.
//!
//! The original draws these from the game's own generator, outside the simulation step, which
//! makes the sim's sequence depend on how many frames the host managed. Robigo gives them a
//! generator of their own, seeded apart from the sim's, so the sim is touched by sim steps alone.
//! Nothing audible changes: in the original these rolls already vary with the frame rate.

use s2_sim::GameRng;

/// The effect a duck in view plays.
pub const DUCK: u8 = 45;

/// The three the pigs in view can play, in draw order. The fourth outcome is silence.
pub const PIGS: [u8; 3] = [36, 42, 43];

/// The five trees in view can play. The last covers two of the six draws.
pub const TREES: [u8; 5] = [37, 38, 39, 40, 41];

/// The three the terrain counter can play. The fourth outcome is silence.
pub const TERRAIN: [u8; 3] = [48, 49, 50];

/// The effect a sheep in view plays.
pub const SHEEP: u8 = 44;

/// The effect a ship being drawn plays, one draw in 31 (`0x1b299`).
pub const SHIP: u8 = 27;

/// How often a ship being drawn is given its chance.
pub const SHIP_ODDS: u16 = 30;

/// The effect a mill at phase 6 of its work plays, one draw in 5 (`0x1a75b`).
pub const MILL: u8 = 27;

/// How often a working mill at its creaking phase is given its chance.
pub const MILL_ODDS: u16 = 4;

/// The effect a fire at its crackling phase asks for every time it is drawn (`0x93eb7`).
pub const FIRE: u8 = 46;

/// The effect a boat at its splashing frame asks for every time it is drawn (`0x1bc2e`).
pub const BOAT: u8 = 34;

/// The chance when a counter is low, as the argument `0x8d885` takes.
const LONGEST: u16 = 50;

/// The chances as the count climbs past 3, 6, 9 and 12 times `k`.
const SHORTER: [u16; 4] = [20, 15, 10, 7];

/// What the count is compared against, in multiples of `k`.
const STEPS: [u16; 4] = [3, 6, 9, 12];

/// How many of its own units each counter counts in, in the order they are tried.
const WEIGHTS: [u16; 5] = [1, 1, 2, 3, 3];

/// The counters a frame fills, which `0x23b60` clears once the picker has read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// Ducks in view (`0x201674`).
    pub ducks: u16,
    /// Pigs drawn at finished pig farms (`0x201678`).
    pub pigs: u16,
    /// Trees in view (`0x201670`).
    pub trees: u16,
    /// The terrain class counter (`0x20166c`).
    pub terrain: u16,
    /// Sheep in view (`0x201676`).
    pub sheep: u16,
}

impl Counters {
    /// The five in the order the picker tries them, each with the units it counts in.
    #[must_use]
    pub const fn tried(self) -> [(u16, u16); 5] {
        [
            (self.ducks, WEIGHTS[0]),
            (self.pigs, WEIGHTS[1]),
            (self.trees, WEIGHTS[2]),
            (self.terrain, WEIGHTS[3]),
            (self.sheep, WEIGHTS[4]),
        ]
    }
}

/// The generator the ambient and draw-time rolls draw from.
///
/// It is the same generator the simulation uses, kept apart from it. Seeding it from the sim's
/// seed would tie the two together again, so a caller gives it one of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ambient {
    rng: GameRng,
}

impl Ambient {
    /// A picker seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self {
            rng: GameRng::new(seed),
        }
    }

    /// The generator as it stands.
    #[must_use]
    pub const fn rng(&self) -> GameRng {
        self.rng
    }

    /// Which effect this frame asks for, if any (`0x23c28`).
    ///
    /// The counters are tried in order and the first whose chance fires decides the pass, even
    /// when its own second draw lands on silence.
    pub fn pick(&mut self, counters: Counters) -> Option<u8> {
        for (place, (count, weight)) in counters.tried().into_iter().enumerate() {
            if !self.fires(count, weight) {
                continue;
            }
            return match place {
                0 => Some(DUCK),
                1 => self.one_or_silence(&PIGS, 3),
                2 => self.one_or_last(&TREES, 5),
                3 => self.one_or_silence(&TERRAIN, 3),
                _ => Some(SHEEP),
            };
        }
        None
    }

    /// Whether a ship being drawn plays its sound (`0x1b299`).
    pub fn ship(&mut self) -> Option<u8> {
        (self.rng.up_to(SHIP_ODDS) == 0).then_some(SHIP)
    }

    /// Whether a working mill with its sails at phase 6 plays its sound (`0x1a75b`).
    pub fn mill(&mut self) -> Option<u8> {
        (self.rng.up_to(MILL_ODDS) == 0).then_some(MILL)
    }

    /// Whether a counter of `count`, counting in `weight`s, fires (`0x23ba0`).
    fn fires(&mut self, count: u16, weight: u16) -> bool {
        if count == 0 {
            return false;
        }
        let mut odds = LONGEST;
        for (step, shorter) in STEPS.into_iter().zip(SHORTER) {
            if count > step.saturating_mul(weight) {
                odds = shorter;
            }
        }
        self.rng.up_to(odds) == 0
    }

    /// One of `choices` by a draw of 0 to `most`, falling silent when the draw lands past the
    /// list. The pigs and terrain branches each have one such draw out of four.
    fn one_or_silence(&mut self, choices: &[u8], most: u16) -> Option<u8> {
        let draw = usize::from(self.rng.up_to(most));
        choices.get(draw).copied()
    }

    /// One of `choices` by a draw of 0 to `most`, with a draw past the list taking the last entry.
    /// The trees branch draws 0 to 5 over five sounds, so the last covers two of the six.
    fn one_or_last(&mut self, choices: &[u8], most: u16) -> Option<u8> {
        let draw = usize::from(self.rng.up_to(most));
        choices.get(draw).or_else(|| choices.last()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trees thick enough that the try fires often, so the second draw is what shows.
    fn loud() -> Counters {
        Counters {
            trees: 100,
            ..Counters::default()
        }
    }

    #[test]
    fn a_counter_of_nothing_never_fires_and_takes_no_draw() {
        let mut ambient = Ambient::new(1);
        let before = ambient.rng();
        assert_eq!(ambient.pick(Counters::default()), None);
        assert_eq!(ambient.rng(), before, "five empty counters draw nothing");
    }

    #[test]
    fn the_odds_shorten_as_a_counter_climbs() {
        let draws = |count: u16, weight: u16| {
            let mut fired = 0;
            let mut ambient = Ambient::new(305_463_004);
            for _ in 0..6000 {
                if ambient.fires(count, weight) {
                    fired += 1;
                }
            }
            fired
        };
        let low = draws(1, 1);
        let high = draws(100, 1);
        assert!(
            low > 0 && high > low * 4,
            "one in 51 against one in eight: {low} {high}"
        );
        assert_eq!(draws(0, 1), 0);
        assert!(
            draws(7, 1) > draws(7, 2),
            "the same count fires less often when it counts in twos"
        );
    }

    #[test]
    fn the_trees_draw_covers_its_five_sounds_with_the_last_one_twice() {
        let mut seen = [0_u32; 5];
        let mut ambient = Ambient::new(7);
        for _ in 0..20_000 {
            if let Some(effect) = ambient.pick(loud()) {
                let place = TREES.iter().position(|&id| id == effect).expect("a tree sound");
                seen[place] += 1;
            }
        }
        assert!(
            seen.iter().all(|&count| count > 0),
            "every sound turns up: {seen:?}"
        );
        assert!(
            seen[4] > seen[0],
            "the last covers two of the six draws: {seen:?}"
        );
    }

    #[test]
    fn a_counter_above_another_takes_the_pass_off_it() {
        let sheep_only = Counters {
            sheep: 100,
            ..Counters::default()
        };
        let both = Counters {
            pigs: 100,
            ..sheep_only
        };
        let count = |counters| {
            let mut ambient = Ambient::new(99);
            (0..6000)
                .filter(|_| ambient.pick(counters) == Some(SHEEP))
                .count()
        };
        let alone = count(sheep_only);
        let behind = count(both);
        assert!(alone > 0 && behind < alone, "{alone} {behind}");
    }

    #[test]
    fn the_pig_branch_falls_silent_one_draw_in_four() {
        let counters = Counters {
            pigs: 100,
            ..Counters::default()
        };
        let mut heard = 0;
        let mut quiet = 0;
        let mut ambient = Ambient::new(305_463_004);
        for _ in 0..40_000 {
            match ambient.pick(counters) {
                Some(effect) => {
                    assert!(PIGS.contains(&effect), "{effect:#x}");
                    heard += 1;
                }
                None => quiet += 1,
            }
        }
        assert!(
            heard > 0 && quiet > heard,
            "the try itself misses most passes: {heard} {quiet}"
        );
    }

    #[test]
    fn a_duck_and_a_sheep_need_no_second_draw() {
        let mut ambient = Ambient::new(3);
        let ducks = Counters {
            ducks: 100,
            ..Counters::default()
        };
        let mut found = false;
        for _ in 0..200 {
            if ambient.pick(ducks) == Some(DUCK) {
                found = true;
                break;
            }
        }
        assert!(found, "a view full of ducks plays the duck");
    }

    #[test]
    fn the_two_drawing_rolls_fire_at_their_own_odds() {
        let mut ambient = Ambient::new(305_463_004);
        let mut ships = 0;
        let mut mills = 0;
        for _ in 0..6000 {
            if ambient.ship().is_some() {
                ships += 1;
            }
            if ambient.mill().is_some() {
                mills += 1;
            }
        }
        assert!(
            ships > 0 && mills > ships * 2,
            "one in 31 against one in five: {ships} {mills}"
        );
        assert_eq!(SHIP, MILL, "both play the same effect");
    }
}
