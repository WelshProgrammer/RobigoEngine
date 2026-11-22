//! Binary string tables (`TXT/ONGAME.ENG`, `TXT/NAMES.ENG`, `TXT/MISS_nnn.ENG`, `TXT3/ONGAME.ENG`).

use crate::bytes::{Reader, Truncated, to_usize};

use super::cp437;

/// Type tag that opens every string table.
pub const MAGIC: u16 = 64_999;

/// Type tag of the raw byte block that holds the offsets and strings.
const BLOCK_TAG: u16 = 1;

/// Type tag of the null object, which stands for no block at all.
const NULL_TAG: u16 = 0;

/// Something unusual that decoding accepted. The shipped tables produce none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// The block size is 0, read as "the rest of the file".
    UnsizedBlock {
        /// Where the block starts.
        offset: usize,
        /// Bytes taken as the block.
        len: usize,
    },
    /// The block is the null object (tag 0), so every slot is empty.
    NoBlock,
}

/// Why a string table could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The input ended inside the header, the offset table or the declared block.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The file does not start with [`MAGIC`].
    #[error("not a string table: tag {0:#06x}")]
    BadMagic(u16),
    /// The nested block is neither the raw byte block (tag 1) nor the null object (tag 0).
    #[error("unsupported block tag {0}")]
    BadBlock(u16),
    /// A slot's offset points past the end of the block.
    #[error("slot {slot}: offset {offset} is past the end of the block")]
    OffsetOutOfRange {
        /// Slot number.
        slot: usize,
        /// Offset from the start of the block.
        offset: u32,
    },
    /// A slot's string runs to the end of the block without a NUL.
    #[error("slot {slot}: string has no terminating NUL")]
    Unterminated {
        /// Slot number.
        slot: usize,
    },
}

/// A table of numbered text slots, each either empty or a NUL-terminated CP437 string.
///
/// Layout (little-endian): the tag [`MAGIC`] (`u16`), the slot count (`u16`), the block tag 1
/// (`u16`), the block size (`u32`), then the block itself. The block starts with one `u32` offset
/// per slot, measured from the start of the block; offset 0 marks an empty slot. The strings
/// follow in any order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringTable {
    block: Vec<u8>,
    len: usize,
    slots: Vec<Option<usize>>,
    quirks: Vec<Quirk>,
}

impl StringTable {
    /// Parses a whole string table file. Bytes after the declared block are ignored, as the game
    /// ignores them. A block size of 0 takes the rest of the file as the block, and a null
    /// block (tag 0) leaves every slot empty; [`StringTable::quirks`] reports both. Slots may
    /// share strings, and the strings need not fill the block.
    ///
    /// # Errors
    ///
    /// See [`Error`]: a wrong tag, a block that does not fit, or a slot that points outside the
    /// block or never ends.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let magic = r.u16()?;
        if magic != MAGIC {
            return Err(Error::BadMagic(magic));
        }
        let count = usize::from(r.u16()?);
        let mut quirks = Vec::new();
        match r.u16()? {
            BLOCK_TAG => {}
            NULL_TAG => {
                return Ok(Self {
                    block: Vec::new(),
                    len: count,
                    slots: Vec::new(),
                    quirks: vec![Quirk::NoBlock],
                });
            }
            tag => return Err(Error::BadBlock(tag)),
        }
        let size = match to_usize(r.u32()?) {
            0 => {
                quirks.push(Quirk::UnsizedBlock {
                    offset: r.position(),
                    len: r.remaining(),
                });
                r.remaining()
            }
            size => size,
        };
        let block = r.bytes(size)?;

        let mut offsets = Reader::new(block);
        let last_nul = block.iter().rposition(|&b| b == 0);
        let mut slots = Vec::with_capacity(count.min(block.len() / 4));
        for slot in 0..count {
            let offset = offsets.u32()?;
            slots.push(Self::locate(block, last_nul, slot, offset)?);
        }
        Ok(Self {
            block: block.to_vec(),
            len: count,
            slots,
            quirks,
        })
    }

    /// What was unusual about the table; empty for every shipped table.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Checks that a slot's string starts inside the block and that a NUL follows it somewhere.
    /// Reading the slot finds the end, so many slots pointing into one long string do not each
    /// scan it here.
    fn locate(
        block: &[u8],
        last_nul: Option<usize>,
        slot: usize,
        offset: u32,
    ) -> Result<Option<usize>, Error> {
        if offset == 0 {
            return Ok(None);
        }
        let start = to_usize(offset);
        if start >= block.len() {
            return Err(Error::OffsetOutOfRange { slot, offset });
        }
        if last_nul.is_none_or(|nul| nul < start) {
            return Err(Error::Unterminated { slot });
        }
        Ok(Some(start))
    }

    /// A table of `len` slots holding `strings`, each a slot number and its bytes. A slot not
    /// given is empty, a slot at `len` or past it is left out, and a string ends at its first
    /// NUL, as one read from a file would.
    #[must_use]
    pub fn from_slots(len: usize, strings: impl IntoIterator<Item = (usize, Vec<u8>)>) -> Self {
        let mut block = Vec::new();
        let mut slots = vec![None; len];
        for (slot, string) in strings {
            let Some(place) = slots.get_mut(slot) else {
                continue;
            };
            *place = Some(block.len());
            block.extend(string.into_iter().take_while(|&byte| byte != 0));
            block.push(0);
        }
        Self {
            block,
            len,
            slots,
            quirks: Vec::new(),
        }
    }

    /// Number of slots, empty ones included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the table has no slots at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The raw bytes of slot `index` without the NUL, or `None` if the slot is empty or out of
    /// range. A present slot can still hold a zero-length string.
    #[must_use]
    pub fn raw(&self, index: usize) -> Option<&[u8]> {
        let tail = self.block.get((*self.slots.get(index)?)?..)?;
        tail.split(|&b| b == 0).next()
    }

    /// Slot `index` decoded from CP437. `@` line breaks are kept; see [`super::lines`].
    #[must_use]
    pub fn get(&self, index: usize) -> Option<String> {
        self.raw(index).map(cp437::decode)
    }

    /// The present slots in slot order, as `(index, raw bytes)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &[u8])> {
        (0..self.slots.len()).filter_map(|i| self.raw(i).map(|s| (i, s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER_LEN: usize = 10;

    /// Builds a table file from slots. Strings are written in reverse slot order, since the
    /// shipped files do not sort them by slot.
    fn build(slots: &[Option<&[u8]>]) -> Vec<u8> {
        let count = u16::try_from(slots.len()).unwrap();
        let mut offsets = vec![0u32; slots.len()];
        let mut strings = Vec::new();
        let table_len = 4 * slots.len();
        for (i, s) in slots.iter().enumerate().rev() {
            if let Some(s) = s {
                offsets[i] = u32::try_from(table_len + strings.len()).unwrap();
                strings.extend_from_slice(s);
                strings.push(0);
            }
        }
        let mut block: Vec<u8> = offsets.iter().flat_map(|o| o.to_le_bytes()).collect();
        block.extend_from_slice(&strings);
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&BLOCK_TAG.to_le_bytes());
        out.extend_from_slice(&u32::try_from(block.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&block);
        out
    }

    #[test]
    fn reads_present_empty_and_blank_slots() {
        let file = build(&[Some(b"Alpha@Beta"), None, Some(b""), Some(&[0x81, b'x'])]);
        let table = StringTable::parse(&file).unwrap();
        assert_eq!(table.len(), 4);
        assert_eq!(table.raw(0), Some(&b"Alpha@Beta"[..]));
        assert_eq!(table.raw(1), None);
        assert_eq!(table.raw(2), Some(&b""[..]));
        assert_eq!(table.get(3).as_deref(), Some("üx"));
        assert_eq!(table.raw(4), None);
        let present: Vec<usize> = table.iter().map(|(i, _)| i).collect();
        assert_eq!(present, [0, 2, 3]);
    }

    #[test]
    fn empty_table_is_valid() {
        let table = StringTable::parse(&build(&[])).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn trailing_bytes_are_ignored() {
        let mut file = build(&[Some(b"x")]);
        file.extend_from_slice(b"junk");
        assert_eq!(StringTable::parse(&file).unwrap().raw(0), Some(&b"x"[..]));
    }

    #[test]
    fn rejects_bad_headers() {
        let mut file = build(&[Some(b"x")]);
        file[0] = 0;
        assert_eq!(StringTable::parse(&file), Err(Error::BadMagic(64_768)));
        let mut file = build(&[Some(b"x")]);
        file[4] = 2;
        assert_eq!(StringTable::parse(&file), Err(Error::BadBlock(2)));
    }

    #[test]
    fn size_zero_takes_the_rest_of_the_file() {
        let mut file = build(&[Some(b"ab"), Some(b"c")]);
        assert!(
            StringTable::parse(&file).unwrap().quirks().is_empty(),
            "{:?}",
            StringTable::parse(&file).unwrap().quirks()
        );
        file[6..10].fill(0);
        let table = StringTable::parse(&file).unwrap();
        assert_eq!(table.raw(0), Some(&b"ab"[..]));
        assert_eq!(table.raw(1), Some(&b"c"[..]));
        assert_eq!(
            table.quirks(),
            [Quirk::UnsizedBlock {
                offset: HEADER_LEN,
                len: file.len() - HEADER_LEN
            }]
        );
    }

    #[test]
    fn null_block_leaves_every_slot_empty() {
        let mut file = build(&[Some(b"x"), Some(b"y")]);
        file[4] = 0;
        let table = StringTable::parse(&file).unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(table.iter().count(), 0);
        assert_eq!(table.quirks(), [Quirk::NoBlock]);
        assert_eq!(StringTable::parse(&file[..6]).unwrap().quirks(), [Quirk::NoBlock]);
    }

    #[test]
    fn null_block_does_not_allocate_its_slot_count() {
        let mut file = MAGIC.to_le_bytes().to_vec();
        file.extend(u16::MAX.to_le_bytes());
        file.extend(NULL_TAG.to_le_bytes());
        let table = StringTable::parse(&file).unwrap();
        assert_eq!(table.len(), usize::from(u16::MAX));
        assert_eq!(table.raw(usize::from(u16::MAX) - 1), None);
        assert_eq!(table.slots.capacity(), 0);
    }

    #[test]
    fn slots_may_share_strings() {
        let mut file = build(&[Some(b"same"), None]);
        let first = file[HEADER_LEN..HEADER_LEN + 4].to_vec();
        file[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&first);
        let table = StringTable::parse(&file).unwrap();
        assert_eq!(table.raw(0), table.raw(1));
        let found = table.quirks();
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn many_slots_into_one_long_string_parse_without_rescanning_it() {
        const SLOTS: u16 = u16::MAX;
        const LEN: usize = 1 << 18;
        let table = usize::from(SLOTS) * 4;
        let mut block: Vec<u8> = (0..usize::from(SLOTS))
            .flat_map(|slot| u32::try_from(table + slot % 2 * 4).unwrap().to_le_bytes())
            .collect();
        block.resize(table + LEN, b'a');
        block.extend([0, b'b', b'c']);
        let mut file = MAGIC.to_le_bytes().to_vec();
        file.extend(SLOTS.to_le_bytes());
        file.extend(BLOCK_TAG.to_le_bytes());
        file.extend(u32::try_from(block.len()).unwrap().to_le_bytes());
        file.extend(&block);
        let parsed = StringTable::parse(&file).unwrap();
        assert_eq!(parsed.raw(0).map(<[u8]>::len), Some(LEN));
        assert_eq!(parsed.raw(1).map(<[u8]>::len), Some(LEN - 4));

        let mut tail = file.clone();
        let at = HEADER_LEN + table + LEN + 1;
        tail[HEADER_LEN..HEADER_LEN + 4]
            .copy_from_slice(&u32::try_from(at - HEADER_LEN).unwrap().to_le_bytes());
        assert_eq!(StringTable::parse(&tail), Err(Error::Unterminated { slot: 0 }));
    }

    #[test]
    fn rejects_bad_slots() {
        let mut file = build(&[Some(b"x")]);
        file[HEADER_LEN..HEADER_LEN + 4].copy_from_slice(&100u32.to_le_bytes());
        assert_eq!(
            StringTable::parse(&file),
            Err(Error::OffsetOutOfRange { slot: 0, offset: 100 })
        );
        let mut file = build(&[Some(b"x")]);
        let last = file.len() - 1;
        file[last] = b'y';
        assert_eq!(StringTable::parse(&file), Err(Error::Unterminated { slot: 0 }));
    }

    #[test]
    fn truncation_never_panics() {
        let file = build(&[Some(b"first"), None, Some(b"second")]);
        for len in 0..file.len() {
            assert!(StringTable::parse(&file[..len]).is_err(), "length {len}");
        }
        for at in 0..file.len() {
            for value in [0, 1, u8::MAX] {
                let mut bad = file.clone();
                bad[at] = value;
                let _ = StringTable::parse(&bad);
            }
        }
    }

    #[test]
    fn huge_counts_do_not_allocate_without_data() {
        let mut file = build(&[]);
        file[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(matches!(StringTable::parse(&file), Err(Error::Truncated(_))));
    }

    #[test]
    fn a_table_built_from_slots_reads_like_a_parsed_one() {
        let parsed =
            StringTable::parse(&build(&[Some(b"one"), None, Some(b""), Some(b"four@line")])).unwrap();
        let built = StringTable::from_slots(
            4,
            [
                (0, b"one".to_vec()),
                (2, Vec::new()),
                (3, b"four@line".to_vec()),
                (9, b"gone".to_vec()),
            ],
        );
        assert_eq!(built.len(), parsed.len());
        assert_eq!(
            built.iter().collect::<Vec<_>>(),
            parsed.iter().collect::<Vec<_>>()
        );
        assert_eq!(built.raw(1), None);
        assert_eq!(
            StringTable::from_slots(1, [(0, b"cut\0here".to_vec())]).raw(0),
            Some(&b"cut"[..])
        );
    }
}
