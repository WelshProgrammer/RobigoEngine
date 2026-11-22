//! The generator the sound effects draw their jitter from.
//!
//! This is the C library's `rand` as the original's compiler built it. It is nothing to do with
//! the simulation's generator and must never be: the sim's sequence has to depend on sim steps
//! alone for replays and lockstep to hold.

/// What the state is multiplied by each draw (`0x41c64e6d`).
const FACTOR: u32 = 1_103_515_245;

/// What is added after the multiply.
const OFFSET: u32 = 12_345;

/// How far the result is shifted down before the top bit goes.
const SHIFT: u32 = 16;

/// The largest value a draw can give.
pub const LARGEST: u32 = 32_767;

/// What the state starts at (`0x1fee2c`). `srand` exists at `0x9b3f2` and is never called, so
/// every run of the original draws the same sequence from the first effect onwards.
const SEED: u32 = 1;

/// The C library generator (`0x9b3ce`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rand {
    state: u32,
}

impl Default for Rand {
    fn default() -> Self {
        Self::new()
    }
}

impl Rand {
    /// The generator as a fresh run of the original has it.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: SEED }
    }

    /// A generator at `state`, for picking a sequence up where it left off.
    #[must_use]
    pub const fn at(state: u32) -> Self {
        Self { state }
    }

    /// The state it holds.
    #[must_use]
    pub const fn state(self) -> u32 {
        self.state
    }

    /// The next draw, 0 to [`LARGEST`]: bits 16 to 30 of the new state.
    pub const fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(FACTOR).wrapping_add(OFFSET);
        (self.state >> SHIFT) & LARGEST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequence_starts_where_the_original_starts() {
        let mut rand = Rand::new();
        let first: Vec<u32> = (0..5).map(|_| rand.next()).collect();
        assert_eq!(first, [16838, 5758, 10113, 17515, 31051]);
    }

    #[test]
    fn every_draw_fits_in_fifteen_bits() {
        let mut rand = Rand::new();
        for _ in 0..10_000 {
            assert!(rand.next() <= LARGEST);
        }
    }

    #[test]
    fn a_generator_can_be_picked_up_from_its_state() {
        let mut rand = Rand::new();
        rand.next();
        rand.next();
        let mut again = Rand::at(rand.state());
        assert_eq!(again.next(), {
            let mut carry = rand;
            carry.next()
        });
    }
}
