//! Bounds-checked little-endian reading over byte slices.

/// The input ended before a read could be satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("input truncated: needed {needed} bytes at offset {offset}")]
pub struct Truncated {
    /// Where the read started.
    pub offset: usize,
    /// How many bytes it wanted.
    pub needed: usize,
}

/// A cursor over a byte slice whose reads fail with [`Truncated`] instead of panicking.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Starts reading at the beginning of `data`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The current offset from the start of the slice.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Bytes left after the current position.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Whether the cursor has reached the end.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// The whole underlying slice.
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Moves the cursor to `pos`, which may be at most the slice length.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when `pos` lies past the end.
    pub fn seek(&mut self, pos: usize) -> Result<(), Truncated> {
        if pos > self.data.len() {
            return Err(Truncated {
                offset: pos,
                needed: 0,
            });
        }
        self.pos = pos;
        Ok(())
    }

    /// Skips `len` bytes.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than `len` bytes remain.
    pub fn skip(&mut self, len: usize) -> Result<(), Truncated> {
        self.bytes(len).map(|_| ())
    }

    /// Returns the next `len` bytes and advances past them.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than `len` bytes remain.
    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8], Truncated> {
        let out = slice(self.data, self.pos, len)?;
        self.pos += len;
        Ok(out)
    }

    /// Returns the next `N` bytes as an array.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than `N` bytes remain.
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Truncated> {
        let mut out = [0; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }

    /// Reads a byte.
    ///
    /// # Errors
    ///
    /// [`Truncated`] at the end of the input.
    pub fn u8(&mut self) -> Result<u8, Truncated> {
        self.array::<1>().map(|[b]| b)
    }

    /// Reads a signed byte.
    ///
    /// # Errors
    ///
    /// [`Truncated`] at the end of the input.
    pub fn i8(&mut self) -> Result<i8, Truncated> {
        self.array().map(i8::from_le_bytes)
    }

    /// Reads a little-endian `u16`.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than 2 bytes remain.
    pub fn u16(&mut self) -> Result<u16, Truncated> {
        self.array().map(u16::from_le_bytes)
    }

    /// Reads a little-endian `i16`.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than 2 bytes remain.
    pub fn i16(&mut self) -> Result<i16, Truncated> {
        self.array().map(i16::from_le_bytes)
    }

    /// Reads a little-endian `u32`.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than 4 bytes remain.
    pub fn u32(&mut self) -> Result<u32, Truncated> {
        self.array().map(u32::from_le_bytes)
    }

    /// Reads a little-endian `i32`.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than 4 bytes remain.
    pub fn i32(&mut self) -> Result<i32, Truncated> {
        self.array().map(i32::from_le_bytes)
    }

    /// Reads a big-endian `u16`, as IFF and XMIDI chunks use.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than 2 bytes remain.
    pub fn u16_be(&mut self) -> Result<u16, Truncated> {
        self.array().map(u16::from_be_bytes)
    }

    /// Reads a big-endian `u32`, as IFF and XMIDI chunks use.
    ///
    /// # Errors
    ///
    /// [`Truncated`] when fewer than 4 bytes remain.
    pub fn u32_be(&mut self) -> Result<u32, Truncated> {
        self.array().map(u32::from_be_bytes)
    }
}

/// Returns `len` bytes of `data` starting at `offset`.
///
/// # Errors
///
/// [`Truncated`] when the range runs past the end, including on overflow.
pub fn slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8], Truncated> {
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or(Truncated { offset, needed: len })
}

/// Widens a 32-bit file field to `usize`, saturating on 16-bit targets.
#[must_use]
pub fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_and_big_endian() {
        let mut r = Reader::new(&[0x34, 0x12, 0x12, 0x34, 0xff]);
        assert_eq!(r.u16(), Ok(0x1234));
        assert_eq!(r.u16_be(), Ok(0x1234));
        assert_eq!(r.i8(), Ok(-1));
        assert!(r.is_empty());
    }

    #[test]
    fn truncation_is_an_error() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.u32(), Err(Truncated { offset: 0, needed: 4 }));
        assert_eq!(r.position(), 0);
        assert!(slice(&[0; 4], usize::MAX, 2).is_err());
    }
}
