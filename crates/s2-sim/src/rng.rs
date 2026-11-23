//! The game's random number generator (`S2.EXE` `0x8d7d0`–`0x8d918`, object `0x201724`).

/// The seed a static initialiser gives the game generator when the program starts.
pub const START_SEED: u32 = 1234;

/// The game generator: a two-word xor-shift with one carry bit, 16 bits out per step.
///
/// Every reduction is a plain remainder of the 16-bit output, modulo bias included, as in the
/// original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GameRng {
    a: u16,
    b: u16,
    carry: bool,
}

impl GameRng {
    /// The generator seeded with `seed` (`0x8d7eb`): `A` the high word, `B` the low word, the
    /// carry set.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        let [b0, b1, a0, a1] = seed.to_le_bytes();
        Self {
            a: u16::from_le_bytes([a0, a1]),
            b: u16::from_le_bytes([b0, b1]),
            carry: true,
        }
    }

    /// The state the game starts every run in.
    #[must_use]
    pub const fn at_start() -> Self {
        Self::new(START_SEED)
    }

    /// The two words and the carry, as `S2.EXE` holds them at `+0x04`, `+0x06` and bit 15 of
    /// `+0x0a` of its generator object.
    #[must_use]
    pub const fn words(&self) -> (u16, u16, bool) {
        (self.a, self.b, self.carry)
    }

    /// One step (`0x8d80a`): returns `A` as it was, then `B' = A`,
    /// `A' = ((A ^ B) >> 1) | carry << 15`, `carry' = (A ^ B) & 1`.
    pub fn step(&mut self) -> u16 {
        let out = self.a;
        let mixed = self.a ^ self.b;
        self.b = self.a;
        self.a = (mixed >> 1) | (u16::from(self.carry) << 15);
        self.carry = mixed & 1 == 1;
        out
    }

    /// A value from 0 to `max` inclusive (`0x8d885`): the output modulo `max + 1`.
    pub fn up_to(&mut self, max: u16) -> u16 {
        let out = self.step();
        out % max.wrapping_add(1).max(1)
    }

    /// A value from `low` to `high` inclusive (`0x8d8ac`): `low` plus the output modulo
    /// `high − low + 1`.
    pub fn between(&mut self, low: u16, high: u16) -> u16 {
        let out = self.step();
        let span = high.wrapping_sub(low).wrapping_add(1).max(1);
        low.wrapping_add(out % span)
    }
}

impl Default for GameRng {
    fn default() -> Self {
        Self::at_start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sixteen_outputs_from_the_start_seed() {
        let mut rng = GameRng::at_start();
        let out: Vec<u16> = (0..16).map(|_| rng.step()).collect();
        assert_eq!(
            out,
            [
                0, 33385, 16692, 57774, 53325, 6385, 58462, 32343, 52484, 55721, 35414, 43519, 37332, 39957,
                34528, 36218
            ]
        );
    }

    #[test]
    fn the_first_coin_flips_match_the_notes() {
        let mut rng = GameRng::at_start();
        let flips: Vec<u16> = (0..6).map(|_| rng.up_to(1)).collect();
        assert_eq!(flips, [0, 1, 0, 0, 1, 1]);
    }

    #[test]
    fn ranges_are_inclusive_remainders() {
        let mut a = GameRng::at_start();
        let mut b = GameRng::at_start();
        a.step();
        b.step();
        assert_eq!(a.between(10, 100), 10 + 33385 % 91);
        assert_eq!(b.up_to(2000), 33385 % 2001);
        assert_eq!(
            GameRng::new((1 << 16) | 2),
            GameRng {
                a: 1,
                b: 2,
                carry: true
            }
        );
    }
}
