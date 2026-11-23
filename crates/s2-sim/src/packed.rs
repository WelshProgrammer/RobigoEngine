//! Where the fields `S2.EXE` packs into its bytes and words sit. Only the conversions between
//! those raw values and the sim's types use it.

/// A run of bits inside a byte or word.
#[derive(Clone, Copy)]
pub(crate) struct Bits {
    first: u32,
    width: u32,
}

impl Bits {
    /// The `width` bits from bit `first` up.
    pub(crate) const fn new(first: u32, width: u32) -> Self {
        Self { first, width }
    }

    /// One bit.
    pub(crate) const fn bit(at: u32) -> Self {
        Self::new(at, 1)
    }

    /// The highest value the field holds.
    #[expect(clippy::cast_possible_truncation, reason = "fields are at most 8 bits wide")]
    pub(crate) const fn max(self) -> u8 {
        ((1_u16 << self.width) - 1) as u8
    }

    /// The field's value in `word`.
    pub(crate) const fn read(self, word: u16) -> u16 {
        (word >> self.first) % (1 << self.width)
    }

    /// The field's value in `word`, for fields up to 8 bits wide.
    #[expect(clippy::cast_possible_truncation, reason = "fields are at most 8 bits wide")]
    pub(crate) const fn byte(self, word: u16) -> u8 {
        self.read(word) as u8
    }

    /// The field's value in byte `byte`.
    pub(crate) const fn get(self, byte: u8) -> u8 {
        self.byte(byte as u16)
    }

    /// Whether a one-bit field is set in byte `byte`.
    pub(crate) const fn is_set(self, byte: u8) -> bool {
        self.get(byte) != 0
    }

    /// `value` placed in the field, the rest zero.
    pub(crate) const fn write(self, value: u16) -> u16 {
        (value % (1 << self.width)) << self.first
    }

    /// `value` placed in the field of a word.
    pub(crate) const fn put(self, value: u8) -> u16 {
        self.write(value as u16)
    }

    /// `value` placed in the field of a byte.
    #[expect(clippy::cast_possible_truncation, reason = "byte fields end by bit 7")]
    pub(crate) const fn set(self, value: u8) -> u8 {
        self.put(value) as u8
    }

    /// A one-bit field of a byte, set when `on`.
    pub(crate) const fn flag(self, on: bool) -> u8 {
        self.set(on as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_count_from_their_first_bit() {
        let field = Bits::new(5, 2);
        assert_eq!(field.get(0b0110_0000), 0b11);
        assert_eq!(field.set(0b01), 0b0010_0000);
        assert_eq!(field.max(), 3);
        assert!(Bits::bit(3).is_set(0b1000));
    }
}
