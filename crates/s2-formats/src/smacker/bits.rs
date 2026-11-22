//! LSB-first bit reading, as every Smacker bitstream uses.

use super::Error;

/// Reads bits from the least significant end of each byte first.
#[derive(Debug, Clone)]
pub(super) struct BitReader<'a> {
    data: &'a [u8],
    next: usize,
    buf: u64,
    avail: u32,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            next: 0,
            buf: 0,
            avail: 0,
        }
    }

    /// Tops up the buffer a byte at a time while another byte fits in its 64 bits.
    fn refill(&mut self) {
        while self.avail <= 56 {
            let Some(&byte) = self.data.get(self.next) else {
                break;
            };
            self.buf |= u64::from(byte) << self.avail;
            self.next += 1;
            self.avail += 8;
        }
    }

    /// Reads `count` bits (at most 32) as an unsigned value, first bit lowest.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the value is masked to at most 32 bits"
    )]
    pub(super) fn bits(&mut self, count: u32) -> Result<u32, Error> {
        debug_assert!(count <= 32);
        if self.avail < count {
            self.refill();
            if self.avail < count {
                return Err(Error::OutOfBits);
            }
        }
        let value = self.buf & ((1u64 << count) - 1);
        self.buf >>= count;
        self.avail -= count;
        Ok(value as u32)
    }

    pub(super) fn bit(&mut self) -> Result<bool, Error> {
        self.bits(1).map(|b| b != 0)
    }

    pub(super) fn u8(&mut self) -> Result<u8, Error> {
        self.bits(8).map(|v| v.to_le_bytes()[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, Error> {
        self.bits(16).map(|v| {
            let [lo, hi, ..] = v.to_le_bytes();
            u16::from_le_bytes([lo, hi])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_low_bits_first() {
        let mut bits = BitReader::new(&[0b1010_0110, 0b1111_1111]);
        assert_eq!(bits.bit(), Ok(false));
        assert_eq!(bits.bits(3), Ok(0b011));
        assert_eq!(bits.bits(8), Ok(0b1111_1010));
        assert_eq!(bits.bits(4), Ok(0b1111));
        assert_eq!(bits.bit(), Err(Error::OutOfBits));
    }

    #[test]
    fn reads_long_runs() {
        let data: Vec<u8> = (0..32).collect();
        let mut bits = BitReader::new(&data);
        for &pair in data.as_chunks::<2>().0 {
            assert_eq!(bits.u16(), Ok(u16::from_le_bytes(pair)));
        }
        assert_eq!(bits.u8(), Err(Error::OutOfBits));
    }
}
