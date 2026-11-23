//! A minimal walker over big-endian IFF chunks, as XMIDI uses.

use super::Error;
use crate::bytes::{Reader, Truncated, slice, to_usize};

/// One chunk: its identifier and body, with offsets into the whole input.
#[derive(Debug, Clone, Copy)]
pub(super) struct Chunk<'a> {
    pub id: [u8; 4],
    /// Offset of the chunk header.
    pub offset: usize,
    /// Offset of the first body byte.
    pub body_offset: usize,
    pub body: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// Reads the form type of a `FORM` or `CAT ` chunk and checks it.
    pub fn form_type(&self, expected: [u8; 4]) -> Result<Chunk<'a>, Error> {
        let found: [u8; 4] = self.body.first_chunk().copied().ok_or(Truncated {
            offset: self.body_offset,
            needed: 4,
        })?;
        if found != expected {
            return Err(Error::UnexpectedFormType {
                offset: self.body_offset,
                expected,
                found,
            });
        }
        let body = self.body.get(4..).unwrap_or_default();
        Ok(Chunk {
            body_offset: self.body_offset + 4,
            body,
            ..*self
        })
    }

    /// Fails unless this chunk has identifier `expected`.
    pub fn expect(self, expected: [u8; 4]) -> Result<Self, Error> {
        if self.id == expected {
            Ok(self)
        } else {
            Err(Error::UnexpectedChunk {
                offset: self.offset,
                expected,
                found: self.id,
            })
        }
    }

    /// Walks the chunks nested in this chunk's body.
    pub fn children(&self) -> Chunks<'a> {
        Chunks::new(self.body, self.body_offset)
    }
}

/// Iterates over consecutive chunks in a byte range.
///
/// After an odd-length chunk, a zero byte is skipped as padding. Chunk identifiers never start
/// with zero, so files from padded and unpadded writers both read.
#[derive(Debug, Clone)]
pub(super) struct Chunks<'a> {
    data: &'a [u8],
    base: usize,
    pos: usize,
}

impl<'a> Chunks<'a> {
    /// Walks `data`, whose first byte sits at `base` in the whole input.
    pub fn new(data: &'a [u8], base: usize) -> Self {
        Self { data, base, pos: 0 }
    }

    /// Offset in the whole input just past the last chunk read.
    pub fn end_offset(&self) -> usize {
        self.base + self.pos
    }

    fn read(&mut self) -> Result<Chunk<'a>, Error> {
        let base = self.base;
        let rebase = |e: Truncated| Truncated {
            offset: e.offset.saturating_add(base),
            needed: e.needed,
        };
        let mut header = Reader::new(slice(self.data, self.pos, 8).map_err(rebase)?);
        let id = header.array()?;
        let len = to_usize(header.u32_be()?);
        let body_pos = self.pos + 8;
        let body = slice(self.data, body_pos, len).map_err(rebase)?;
        let mut next = body_pos + len;
        if len % 2 == 1 && self.data.get(next) == Some(&0) {
            next += 1;
        }
        let chunk = Chunk {
            id,
            offset: self.base + self.pos,
            body_offset: self.base + body_pos,
            body,
        };
        self.pos = next;
        Ok(chunk)
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let item = self.read();
        if item.is_err() {
            self.pos = self.data.len();
        }
        Some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_padded_and_unpadded_chunks() {
        let data = [
            b'A', b'B', b'C', b'D', 0, 0, 0, 1, 7, 0, // padded
            b'E', b'F', b'G', b'H', 0, 0, 0, 1, 8, // unpadded, at the end
        ];
        let chunks: Vec<_> = Chunks::new(&data, 100).collect::<Result<_, _>>().unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].id, *b"ABCD");
        assert_eq!(chunks[0].body, [7]);
        assert_eq!(chunks[1].offset, 110);
        assert_eq!(chunks[1].body_offset, 118);
    }

    #[test]
    fn truncated_body_is_an_error() {
        let data = [b'A', b'B', b'C', b'D', 0, 0, 0, 9, 1];
        let mut chunks = Chunks::new(&data, 0);
        assert!(matches!(chunks.next(), Some(Err(Error::Truncated(_)))));
        assert!(chunks.next().is_none());
    }
}
