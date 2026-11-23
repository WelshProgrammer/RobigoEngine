//! Named indexes: `.IDX` files that name and locate tagged LST items in a matching `.DAT`.
//!
//! The interface graphics and fonts live in these pairs: `DATA/IO/IO` and `DATA/RESOURCE` in
//! every edition, plus `DATA/IO/EDITIO` and `DATA/EDITRES` for the Gold Edition map editor. Each
//! index entry holds a name, an offset into the DAT and a type id; the bytes at that offset are
//! one tagged item, decoded with [`Item::parse_tagged`].
//!
//! Entry names are stored as raw bytes. The game's character set is CP437, and every shipped
//! name is ASCII. [`Entry::name`] returns a `&str` only for an all-ASCII name; a CP437 view
//! belongs with the `text` module.
//!
//! See `docs/formats/idx.md` for the byte layout.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::bytes::{Reader, Truncated, to_usize};
use crate::lst::{self, Budget, Item, ItemType};

/// Bytes before the first entry: a `u32` entry count.
pub const HEADER_SIZE: usize = 4;

/// Bytes in each index entry.
pub const ENTRY_SIZE: usize = 28;

/// Bytes reserved for an entry's NUL-padded name.
pub const NAME_SIZE: usize = 16;

/// Bytes between an entry's offset and its type id, kept as found.
pub const RESERVED_SIZE: usize = 6;

/// The most bytes the items at distinct offsets may cover in total, as a multiple of the DAT's
/// length. Overlapping items past this are refused, which stops a small pair decoding the same
/// bytes without bound.
pub const MAX_COVERAGE: usize = 4;

/// Everything that can go wrong while decoding an IDX/DAT pair.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The index ended inside its header or an entry.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// An entry points past the end of the DAT.
    #[error("entry {index} points at offset {offset}, past the end of the {len}-byte DAT")]
    Offset {
        /// Entry position in the index.
        index: u32,
        /// The stored offset.
        offset: u32,
        /// DAT length.
        len: usize,
    },
    /// Overlapping items cover the DAT more than [`MAX_COVERAGE`] times over.
    #[error("entry {index} brings the decoded items to {covered} bytes of a {len}-byte DAT")]
    Coverage {
        /// The entry that went past the limit.
        index: u32,
        /// DAT bytes the items decoded so far cover, counting overlaps each time.
        covered: usize,
        /// DAT length.
        len: usize,
    },
    /// The item failed to decode. Truncation offsets inside `source` count from the entry's
    /// offset in the DAT.
    #[error("entry {index}: {source}")]
    Item {
        /// Entry position in the index.
        index: u32,
        /// What went wrong.
        source: lst::Error,
    },
}

/// Something unusual that decoding accepted. The shipped pairs produce none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// The index holds bytes past the entries its count announces; they are ignored.
    TrailingBytes {
        /// Where they start in the index.
        offset: usize,
        /// How many there are.
        len: usize,
    },
    /// The index type id differs from the tag on the item in the DAT. The entry is kept with no
    /// item (see [`Entry::is_skipped`]).
    TypeMismatch {
        /// Entry position in the index.
        index: u32,
        /// Type id from the index entry.
        indexed: u16,
        /// Tag read at the entry's offset in the DAT.
        stored: u16,
    },
    /// Two entries point at the same item, which is decoded once and shared.
    SharedOffset {
        /// The entry listed first in the index.
        first: u32,
        /// The entry listed later.
        second: u32,
        /// The offset they share.
        offset: u32,
    },
    /// An item runs on into the next entry's item.
    Overlap {
        /// Entry position in the index.
        index: u32,
        /// DAT offset one past the item's last byte.
        end: usize,
        /// DAT offset of the next item.
        next: usize,
    },
    /// A quirk inside an entry's item.
    Item {
        /// Entry position in the index.
        index: u32,
        /// What was unusual.
        quirk: lst::Quirk,
    },
}

/// One named item from an IDX/DAT pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    name: [u8; NAME_SIZE],
    offset: u32,
    reserved: [u8; RESERVED_SIZE],
    kind: u16,
    size: usize,
    skipped: bool,
    item: Arc<Item>,
}

impl Entry {
    /// The stored name up to its first NUL, or all 16 bytes when there is none.
    #[must_use]
    pub fn name_bytes(&self) -> &[u8] {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_SIZE);
        self.name.get(..len).unwrap_or(&self.name)
    }

    /// The name as text when every byte is ASCII, as in all shipped entries; `None` otherwise.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        let bytes = self.name_bytes();
        if bytes.is_ascii() {
            std::str::from_utf8(bytes).ok()
        } else {
            None
        }
    }

    /// Whether the name equals `name`, ignoring ASCII case.
    #[must_use]
    pub fn is_named(&self, name: impl AsRef<[u8]>) -> bool {
        self.name_bytes().eq_ignore_ascii_case(name.as_ref())
    }

    /// Where the tagged item starts in the DAT.
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Bytes the tagged item occupies in the DAT, its `u16` tag included; 0 for a skipped
    /// entry.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// The type id the index entry stores.
    #[must_use]
    pub fn indexed_kind(&self) -> u16 {
        self.kind
    }

    /// Whether the entry was skipped because its index type id disagrees with the DAT, as
    /// [`Quirk::TypeMismatch`] reports. A skipped entry's item is [`Item::Empty`].
    #[must_use]
    pub fn is_skipped(&self) -> bool {
        self.skipped
    }

    /// The six bytes between the offset and the type id, not interpreted. S2.EXE writes runtime
    /// values here (what looks like a memory address and a small count) and saves the index
    /// back, so they vary between installs and play sessions.
    #[must_use]
    pub fn reserved(&self) -> [u8; RESERVED_SIZE] {
        self.reserved
    }

    /// The decoded item.
    #[must_use]
    pub fn item(&self) -> &Item {
        &self.item
    }

    /// The item's type, or `None` for an empty (type 0) item.
    #[must_use]
    pub fn kind(&self) -> Option<ItemType> {
        self.item.kind()
    }

    /// Takes the item out of the entry, copying it when other entries share it.
    #[must_use]
    pub fn into_item(self) -> Item {
        Arc::unwrap_or_clone(self.item)
    }
}

/// A decoded IDX/DAT pair: every entry in index order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Index {
    entries: Vec<Entry>,
    quirks: Vec<Quirk>,
}

impl Index {
    /// Decodes an index and the DAT it points into.
    ///
    /// Items may sit in any order with gaps between them. Entries that share an offset share
    /// one decoded item, and items that run into the next one are kept. An entry whose index
    /// type disagrees with the DAT is skipped, and bytes after the last index entry are
    /// ignored. [`Index::quirks`] reports each of these. The items at distinct offsets may
    /// cover at most [`MAX_COVERAGE`] times the DAT, which keeps the decoded size in proportion
    /// to the input however the entries overlap.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] when the index is shorter than its entry count, [`Error::Offset`]
    /// for an offset past the DAT, [`Error::Coverage`] when overlaps exceed the limit, and
    /// [`Error::Item`] when an item fails to decode.
    pub fn parse(idx: &[u8], dat: &[u8]) -> Result<Self, Error> {
        let mut quirks = Vec::new();
        let raw = read_entries(idx, &mut quirks)?;
        let mut budget = Budget::for_input(dat.len());
        let mut decoded: BTreeMap<u32, usize> = BTreeMap::new();
        let mut covered = 0usize;
        let mut entries: Vec<Entry> = Vec::with_capacity(raw.len());
        for (index, raw) in (0..).zip(raw) {
            let prior = decoded
                .get(&raw.offset)
                .and_then(|&first| Some((first, entries.get(first)?)));
            let entry = if let Some((first, prior)) = prior {
                quirks.push(Quirk::SharedOffset {
                    first: u32::try_from(first).unwrap_or(u32::MAX),
                    second: index,
                    offset: raw.offset,
                });
                raw.into_entry(Arc::clone(&prior.item), prior.size, false)
            } else {
                let entry = raw.decode(index, dat, &mut budget, &mut quirks)?;
                covered = covered.saturating_add(entry.size);
                if covered > dat.len().saturating_mul(MAX_COVERAGE) {
                    return Err(Error::Coverage {
                        index,
                        covered,
                        len: dat.len(),
                    });
                }
                if !entry.skipped {
                    decoded.insert(entry.offset, entries.len());
                }
                entry
            };
            entries.push(entry);
        }
        report_overlaps(&entries, dat.len(), &mut quirks);
        Ok(Self { entries, quirks })
    }

    /// What was unusual about the pair, in the order found; empty for every shipped pair.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// The first entry named `name`, ignoring ASCII case, by linear scan. Shipped indexes hold
    /// a few hundred entries at most.
    #[must_use]
    pub fn get(&self, name: impl AsRef<[u8]>) -> Option<&Entry> {
        let name = name.as_ref();
        self.entries.iter().find(|e| e.is_named(name))
    }

    /// The position of the first entry named `name`, ignoring ASCII case.
    #[must_use]
    pub fn position(&self, name: impl AsRef<[u8]>) -> Option<usize> {
        let name = name.as_ref();
        self.entries.iter().position(|e| e.is_named(name))
    }

    /// The entry at `index` in index order, or `None` past the end.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// Every entry in index order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over the entries in index order.
    pub fn iter(&self) -> std::slice::Iter<'_, Entry> {
        self.entries.iter()
    }

    /// Takes the entries out of the index.
    #[must_use]
    pub fn into_entries(self) -> Vec<Entry> {
        self.entries
    }
}

impl<'a> IntoIterator for &'a Index {
    type Item = &'a Entry;
    type IntoIter = std::slice::Iter<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// An index entry before its item is decoded.
struct RawEntry {
    name: [u8; NAME_SIZE],
    offset: u32,
    reserved: [u8; RESERVED_SIZE],
    kind: u16,
}

impl RawEntry {
    fn read(r: &mut Reader<'_>) -> Result<Self, Truncated> {
        Ok(Self {
            name: r.array()?,
            offset: r.u32()?,
            reserved: r.array()?,
            kind: r.u16()?,
        })
    }

    fn into_entry(self, item: Arc<Item>, size: usize, skipped: bool) -> Entry {
        Entry {
            name: self.name,
            offset: self.offset,
            reserved: self.reserved,
            kind: self.kind,
            size,
            skipped,
            item,
        }
    }

    /// Decodes the item at this entry's offset, or skips it when the DAT's tag disagrees with
    /// the index.
    fn decode(
        self,
        index: u32,
        dat: &[u8],
        budget: &mut Budget,
        quirks: &mut Vec<Quirk>,
    ) -> Result<Entry, Error> {
        let data = dat.get(to_usize(self.offset)..).ok_or(Error::Offset {
            index,
            offset: self.offset,
            len: dat.len(),
        })?;
        if let Some(&[lo, hi]) = data.get(..2) {
            let stored = u16::from_le_bytes([lo, hi]);
            if stored != self.kind {
                quirks.push(Quirk::TypeMismatch {
                    index,
                    indexed: self.kind,
                    stored,
                });
                return Ok(self.into_entry(Arc::new(Item::Empty), 0, true));
            }
        }
        let (item, size) =
            Item::parse_tagged_within(data, budget).map_err(|source| Error::Item { index, source })?;
        quirks.extend(item.quirks().iter().map(|quirk| Quirk::Item {
            index,
            quirk: quirk.clone(),
        }));
        Ok(self.into_entry(Arc::new(item), size, false))
    }
}

/// Reads the entry count and the entries, checking the index length against the count before
/// allocating.
fn read_entries(idx: &[u8], quirks: &mut Vec<Quirk>) -> Result<Vec<RawEntry>, Error> {
    let mut r = Reader::new(idx);
    let count = r.u32()?;
    let needed = to_usize(count).saturating_mul(ENTRY_SIZE);
    if r.remaining() < needed {
        return Err(Truncated {
            offset: HEADER_SIZE,
            needed,
        }
        .into());
    }
    let entries = (0..count)
        .map(|_| RawEntry::read(&mut r).map_err(Error::from))
        .collect::<Result<_, _>>()?;
    if !r.is_empty() {
        quirks.push(Quirk::TrailingBytes {
            offset: r.position(),
            len: r.remaining(),
        });
    }
    Ok(entries)
}

/// Reports each decoded item that runs past the next larger offset in the index.
fn report_overlaps(entries: &[Entry], dat_len: usize, quirks: &mut Vec<Quirk>) {
    let mut offsets: Vec<u32> = entries.iter().map(|e| e.offset).collect();
    offsets.sort_unstable();
    offsets.dedup();
    let mut seen = BTreeSet::new();
    for (index, entry) in (0..).zip(entries) {
        if entry.skipped || !seen.insert(entry.offset) {
            continue;
        }
        let after = offsets.partition_point(|&offset| offset <= entry.offset);
        let next = offsets.get(after).map_or(dat_len, |&offset| to_usize(offset));
        let end = to_usize(entry.offset).saturating_add(entry.size);
        if end > next {
            quirks.push(Quirk::Overlap { index, end, next });
        }
    }
}
