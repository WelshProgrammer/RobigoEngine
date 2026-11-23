//! Units for the terrain code: whole pixels, 1/16 pixels, fixed-point attributes and the packed
//! accumulators the filler walks edges with.

/// A position on the surface being drawn, in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    /// Pixels right of the surface's left edge.
    pub x: i32,
    /// Pixels below the surface's top edge.
    pub y: i32,
}

impl Point {
    /// The point at (`x`, `y`).
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// This point moved by (`dx`, `dy`).
    #[must_use]
    pub const fn offset(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

/// A coordinate in 1/16 pixels, the precision the filler clips and walks edges at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Subpixel(i32);

impl Subpixel {
    /// The raw value, in 1/16 pixels.
    #[must_use]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// The left or top edge of pixel `pixel`. Wraps like the original's shift.
    #[must_use]
    pub const fn from_pixels(pixel: i32) -> Self {
        Self(pixel.wrapping_shl(4))
    }

    /// The raw value, in 1/16 pixels.
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// The pixel column or row containing this position (rounds towards negative infinity).
    #[must_use]
    pub const fn pixel(self) -> i32 {
        self.0 >> 4
    }
}

/// An 8.8 fixed-point attribute: a texel coordinate or a shading-table row.
///
/// Held in 32 bits: clipping interpolates in 32 bits, and the edge walker reads all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed8(i32);

impl Fixed8 {
    /// The raw value, in 1/256 units.
    #[must_use]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// The whole number `whole`.
    #[must_use]
    pub const fn from_int(whole: u8) -> Self {
        Self((whole as i32) << 8)
    }

    /// The raw value, in 1/256 units.
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// This value shifted right by `bits`, as the fog dims shading rows.
    #[must_use]
    pub const fn shr(self, bits: u32) -> Self {
        Self(self.0 >> bits)
    }
}

/// A 16.16 fixed-point step, per row along an edge or per pixel along a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Fixed16(i32);

impl Fixed16 {
    /// The raw value, in 1/65536 units.
    #[must_use]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// The raw value, in 1/65536 units.
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// The integer half.
    #[must_use]
    pub const fn high(self) -> u32 {
        self.0.cast_unsigned() >> 16
    }

    /// The fraction half.
    #[must_use]
    pub const fn low(self) -> u32 {
        self.0.cast_unsigned() & LOW_HALF
    }

    /// The low byte of the integer half.
    #[must_use]
    pub const fn int8(self) -> u8 {
        low8(self.0.cast_unsigned() >> 16)
    }
}

/// One row of an edge as the scan converter stores it: four 16-bit values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EdgeEntry {
    /// Pixel column of the edge.
    pub x: u16,
    /// Texture column, 8.8.
    pub u: u16,
    /// Texture row, 8.8.
    pub v: u16,
    /// Shading row, 8.8.
    pub light: u16,
}

/// The four 32-bit words an edge's x, u, v and light are walked in.
///
/// Each word holds one value's 16-bit integer part in its low half and the next value's
/// fraction in its high half:
///
/// | Word | High half | Low half |
/// |---|---|---|
/// | 0 | u fraction | x |
/// | 1 | v fraction | u (8.8) |
/// | 2 | light fraction | v (8.8) |
/// | 3 | x fraction | light (8.8) |
///
/// [`Self::step`] adds word by word. Each carry runs into the next word and the last carry back
/// into the first, so an overflowing low half also nudges the fraction above it.
/// Those leaks show in the original's output and are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Packed([u32; 4]);

impl Packed {
    /// The four words, in walking order.
    #[must_use]
    pub const fn new(words: [u32; 4]) -> Self {
        Self(words)
    }

    /// The four words, in walking order.
    #[must_use]
    pub const fn words(self) -> [u32; 4] {
        self.0
    }

    /// An edge's starting accumulators: x in 1/16 pixels and u, v and light as given.
    #[must_use]
    pub const fn start(x: Subpixel, u: Fixed8, v: Fixed8, light: Fixed8) -> Self {
        let x = x.raw().cast_unsigned();
        // Whole pixels of x go in word 0; its four 1/16 bits become the top of word 3's x fraction.
        Self([
            x.cast_signed().wrapping_shr(4).cast_unsigned(),
            u.raw().cast_unsigned(),
            v.raw().cast_unsigned(),
            (x << 28) | (light.raw().cast_unsigned() & LOW_HALF),
        ])
    }

    /// Per-row steps for x, u, v and light, packed the same way as the accumulators.
    #[must_use]
    pub const fn steps(x: Fixed16, u: Fixed16, v: Fixed16, light: Fixed16) -> Self {
        Self([
            (u.low() << 16) | x.high(),
            (v.low() << 16) | u.high(),
            (light.low() << 16) | v.high(),
            (x.low() << 16) | light.high(),
        ])
    }

    /// One row down the edge: adds `step` with the carries described on [`Packed`]. The carry
    /// added back into the first word is itself dropped if it overflows.
    #[must_use]
    pub const fn step(self, step: Self) -> Self {
        let [a0, a1, a2, a3] = self.0;
        let [s0, s1, s2, s3] = step.0;
        let (a0, c0) = a0.overflowing_add(s0);
        let (a1, c1) = add_with_carry(a1, s1, c0);
        let (a2, c2) = add_with_carry(a2, s2, c1);
        let (a3, c3) = add_with_carry(a3, s3, c2);
        Self([a0.wrapping_add(c3 as u32), a1, a2, a3])
    }

    /// The four low halves: what the scan converter stores for this row.
    #[must_use]
    pub const fn entry(self) -> EdgeEntry {
        let [x, u, v, light] = self.0;
        EdgeEntry {
            x: low16(x),
            u: low16(u),
            v: low16(v),
            light: low16(light),
        }
    }
}

const fn add_with_carry(a: u32, b: u32, carry: bool) -> (u32, bool) {
    let (sum, first) = a.overflowing_add(b);
    let (sum, second) = sum.overflowing_add(carry as u32);
    (sum, first || second)
}

/// The bits of a word below its high half.
pub(crate) const LOW_HALF: u32 = (1 << 16) - 1;

/// The low 16 bits.
#[allow(
    clippy::cast_possible_truncation,
    reason = "keeping the low half is the point"
)]
pub(crate) const fn low16(value: u32) -> u16 {
    value as u16
}

/// The low 8 bits.
#[allow(
    clippy::cast_possible_truncation,
    reason = "keeping the low byte is the point"
)]
pub(crate) const fn low8(value: u32) -> u8 {
    value as u8
}

/// A 64-bit quotient cut to 32 bits, as `idiv` leaves it in `eax`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the original keeps the low 32 bits"
)]
pub(crate) const fn low32(value: i64) -> i32 {
    value as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The step as one 128-bit add whose carry out is added back into the lowest word only.
    fn reference_step(a: Packed, s: Packed) -> Packed {
        let wide = |p: Packed| {
            p.words()
                .iter()
                .rev()
                .fold(0u128, |acc, &w| (acc << 32) | u128::from(w))
        };
        let (sum, carry) = wide(a).overflowing_add(wide(s));
        let mut words = [0u32; 4];
        for (i, word) in words.iter_mut().enumerate() {
            *word = u32::try_from((sum >> (32 * i)) & u128::from(u32::MAX)).unwrap();
        }
        words[0] = words[0].wrapping_add(u32::from(carry));
        Packed::new(words)
    }

    /// A 16.16 value from its integer and its fraction in 1/65536 units.
    fn fixed(whole: i32, fraction: i32) -> Fixed16 {
        Fixed16::from_raw((whole << 16) + fraction)
    }

    /// A packed word from its high and low halves.
    fn word(high: u32, low: u32) -> u32 {
        (high << 16) | low
    }

    #[test]
    fn subpixels_floor_to_pixels() {
        assert_eq!(Subpixel::from_pixels(3).raw(), 48);
        assert_eq!(Subpixel::from_raw(47).pixel(), 2);
        assert_eq!(Subpixel::from_raw(-1).pixel(), -1);
        assert_eq!(Subpixel::from_raw(-16).pixel(), -1);
        assert_eq!(Subpixel::from_raw(-17).pixel(), -2);
    }

    #[test]
    fn fixed_halves() {
        let step = fixed(-2, 1 << 15);
        assert_eq!(step.high(), u32::from(u16::MAX) - 1);
        assert_eq!(step.low(), 1 << 15);
        assert_eq!(step.int8(), u8::MAX - 1);
        assert_eq!(Fixed8::from_int(64).raw(), 64 << 8);
        assert_eq!(Fixed8::from_int(64).shr(2).raw(), 16 << 8);
    }

    #[test]
    fn start_splits_x_into_integer_and_fraction() {
        let start = Packed::start(
            Subpixel::from_raw(18 * 16 + 3),
            Fixed8::from_int(16),
            Fixed8::from_raw((48 << 8) + 16),
            Fixed8::from_raw((1 << 16) + (64 << 8) + 128),
        );
        assert_eq!(
            start.words(),
            [18, 16 << 8, (48 << 8) + 16, word(3 << 12, (64 << 8) + 128)]
        );
        let negative = Packed::start(
            Subpixel::from_raw(-8),
            Fixed8::default(),
            Fixed8::default(),
            Fixed8::default(),
        );
        assert_eq!(negative.words()[0], u32::MAX, "the x integer is sign-extended");
        assert_eq!(negative.words()[3], word(1 << 15, 0));
    }

    #[test]
    fn steps_pack_each_fraction_above_the_previous_value() {
        let steps = Packed::steps(
            fixed(1, 1 << 15),
            fixed(2, 1 << 14),
            fixed(3, 1 << 13),
            fixed(4, 1 << 12),
        );
        assert_eq!(
            steps.words(),
            [
                word(1 << 14, 1),
                word(1 << 13, 2),
                word(1 << 12, 3),
                word(1 << 15, 4)
            ]
        );
    }

    #[test]
    fn a_plain_step_advances_every_value() {
        let start = Packed::start(
            Subpixel::from_pixels(10),
            Fixed8::from_int(16),
            Fixed8::from_int(48),
            Fixed8::from_int(64),
        );
        let steps = Packed::steps(fixed(1, 1 << 15), fixed(256, 0), fixed(128, 0), fixed(-16, 0));
        let one = start.step(steps);
        assert_eq!(
            one.entry(),
            EdgeEntry {
                x: 11,
                u: 17 << 8,
                v: (48 << 8) + 128,
                light: (63 << 8) + 240
            }
        );
        assert_eq!(
            one.words()[3] >> 16,
            (1 << 15) + 1,
            "the falling light's carry leaks into the x fraction"
        );
        let two = one.step(steps);
        assert_eq!(two.entry().x, 13, "the half pixel carries on the second row");
    }

    #[test]
    fn x_wrapping_past_16_bits_leaks_into_the_u_fraction() {
        let acc = Packed::new([LOW_HALF, 0, 0, 0]);
        let step = Packed::new([1, 0, 0, 0]);
        assert_eq!(acc.step(step).words(), [1 << 16, 0, 0, 0]);
    }

    #[test]
    fn light_wrapping_past_16_bits_leaks_into_the_x_fraction() {
        let acc = Packed::new([0, 0, 0, LOW_HALF]);
        let step = Packed::new([0, 0, 0, 1]);
        assert_eq!(acc.step(step).words(), [0, 0, 0, 1 << 16]);
    }

    #[test]
    fn the_x_fraction_carries_round_into_x() {
        let acc = Packed::new([5, 0, 0, 1 << 31]);
        let step = Packed::new([0, 0, 0, 1 << 31]);
        assert_eq!(acc.step(step).words(), [6, 0, 0, 0]);
    }

    #[test]
    fn carries_ripple_through_every_word() {
        let acc = Packed::new([u32::MAX; 4]);
        let step = Packed::new([1, 0, 0, 0]);
        assert_eq!(acc.step(step).words(), [1, 0, 0, 0]);
    }

    #[test]
    fn the_carry_back_into_the_first_word_is_dropped_when_it_overflows() {
        let acc = Packed::new([u32::MAX, 0, 0, u32::MAX]);
        let step = Packed::new([0, 0, 0, 1]);
        assert_eq!(acc.step(step).words(), [0, 0, 0, 0]);
    }

    #[test]
    fn step_matches_a_wide_add_on_random_words() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            u32::try_from(seed >> 32).unwrap()
        };
        for _ in 0..100_000 {
            let a = Packed::new([next(), next(), next(), next()]);
            let s = Packed::new([next(), next(), next(), next()]);
            assert_eq!(a.step(s), reference_step(a, s), "{a:x?} + {s:x?}");
        }
    }
}
