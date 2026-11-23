//! A hash of the game state that comes out the same on every machine.

use std::hash::{Hash, Hasher};

/// 64-bit FNV-1a over a fixed byte encoding. Each integer is written little-endian at its own
/// width, and `usize`/`isize` (lengths, enum discriminants) as 64 bits. The result is the same
/// whatever the target's word size or byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHasher(u64);

impl StateHasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;

    /// A hasher with nothing written yet.
    #[must_use]
    pub const fn new() -> Self {
        Self(Self::OFFSET)
    }

    /// The hash of `value`.
    #[must_use]
    pub fn of<T: Hash + ?Sized>(value: &T) -> u64 {
        let mut hasher = Self::new();
        value.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for StateHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(Self::PRIME);
        }
    }

    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_le_bytes());
    }

    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }

    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }

    fn write_u128(&mut self, i: u128) {
        self.write(&i.to_le_bytes());
    }

    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    fn write_i16(&mut self, i: i16) {
        self.write(&i.to_le_bytes());
    }

    fn write_i32(&mut self, i: i32) {
        self.write(&i.to_le_bytes());
    }

    fn write_i64(&mut self, i: i64) {
        self.write(&i.to_le_bytes());
    }

    fn write_i128(&mut self, i: i128) {
        self.write(&i.to_le_bytes());
    }

    fn write_isize(&mut self, i: isize) {
        self.write_i64(i as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_hash_as_fnv1a() {
        let mut hasher = StateHasher::new();
        hasher.write(b"a");
        assert_eq!(hasher.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn integers_are_little_endian_at_a_fixed_width() {
        let mut bytes = StateHasher::new();
        bytes.write(&[0x34, 0x12, 0, 0, 0, 0, 0, 0]);
        assert_eq!(StateHasher::of(&0x1234_usize), bytes.finish());
        let mut word = StateHasher::new();
        word.write(&[0x34, 0x12]);
        assert_eq!(StateHasher::of(&0x1234_u16), word.finish());
    }
}
