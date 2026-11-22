//! The settings files beside the slots: `DIR.DAT`, `MISSION.DAT`, `MISSION2.DAT` and `RES.DAT`.

use super::parts::expect_end;
use super::{Error, FIRST_SLOT};
use crate::bytes::{Reader, Truncated};

/// Entries in the slot directory.
pub const DIRECTORY_SLOTS: usize = 11;

/// Bytes in a slot name field, terminator included.
pub const NAME_LEN: usize = 50;

/// Bytes in `DIR.DAT`.
pub const DIRECTORY_LEN: usize = 4 + DIRECTORY_SLOTS * ENTRY_LEN;

/// Chapters of the Roman campaign, one flag each at the start of `MISSION.DAT`.
pub const CAMPAIGN_CHAPTERS: usize = 10;

/// Bytes the game reads from and writes to `MISSION.DAT`. Bytes past these are ignored.
pub const MISSION_BUFFER_LEN: usize = 30;

/// Bytes in Gold's `MISSION2.DAT`.
pub const WORLD_CAMPAIGN_LEN: usize = 19;

/// The World Campaign's continents, one won word each in `MISSION2.DAT`.
pub const WORLD_CONTINENTS: usize = 9;

const ENTRY_LEN: usize = 7 + NAME_LEN;

/// `DIR.DAT`: a save counter and the names of the [`DIRECTORY_SLOTS`] slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    counter: u32,
    entries: Vec<DirEntry>,
}

impl Directory {
    /// Decodes the directory.
    ///
    /// # Errors
    ///
    /// [`Error::Length`] when the file is not [`DIRECTORY_LEN`] bytes.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() != DIRECTORY_LEN {
            return Err(Error::Length {
                len: data.len(),
                expected: DIRECTORY_LEN,
            });
        }
        let mut r = Reader::new(data);
        let counter = r.u32()?;
        let entries = (FIRST_SLOT..)
            .take(DIRECTORY_SLOTS)
            .map(|slot| DirEntry::read(&mut r, slot))
            .collect::<Result<_, _>>()?;
        expect_end(&r)?;
        Ok(Self { counter, entries })
    }

    /// Saves made so far; each save stores the new value as its entry's sequence number.
    #[must_use]
    pub fn counter(&self) -> u32 {
        self.counter
    }

    /// All entries, used or not, in slot order.
    #[must_use]
    pub fn entries(&self) -> &[DirEntry] {
        &self.entries
    }
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    slot: u16,
    used: u16,
    kind: u8,
    sequence: u32,
    name: [u8; NAME_LEN],
}

impl DirEntry {
    fn read(r: &mut Reader<'_>, slot: u16) -> Result<Self, Truncated> {
        Ok(Self {
            slot,
            used: r.u16()?,
            kind: r.u8()?,
            sequence: r.u32()?,
            name: r.array()?,
        })
    }

    /// The slot number its files carry, such as 900 for `WORLD900.DAT`.
    #[must_use]
    pub fn slot(&self) -> u16 {
        self.slot
    }

    /// Whether the slot holds a save.
    #[must_use]
    pub fn is_used(&self) -> bool {
        self.used != 0
    }

    /// The raw in-use word; the game writes 1.
    #[must_use]
    pub fn used(&self) -> u16 {
        self.used
    }

    /// A byte copied from the running game when saving; meaning unknown.
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// The directory counter at the time of the save; a higher value is a newer save.
    #[must_use]
    pub fn sequence(&self) -> u32 {
        self.sequence
    }

    /// The name up to its terminator, as CP437 bytes. Bytes after the terminator are stale.
    #[must_use]
    pub fn name_bytes(&self) -> &[u8] {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
        self.name.get(..end).unwrap_or_default()
    }

    /// The whole name field, terminator and stale bytes included.
    #[must_use]
    pub fn name_field(&self) -> &[u8; NAME_LEN] {
        &self.name
    }

    /// The name, decoded from CP437.
    #[must_use]
    pub fn name(&self) -> String {
        crate::text::cp437::decode(self.name_bytes())
    }
}

/// `MISSION.DAT`: one `'0'` or `'1'` per campaign chapter, `'1'` meaning unlocked.
///
/// The game reads up to [`MISSION_BUFFER_LEN`] bytes into a buffer, sets flags, then recreates the
/// file and writes the whole buffer back. The shipped file is 13 bytes (ten flags, CR, LF, DOS
/// end-of-file); once the game has written it, the bytes after the original 13 are whatever the
/// buffer held. A longer file comes from an external edit or another version of the game, and the
/// bytes past [`MISSION_BUFFER_LEN`] are dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionProgress {
    bytes: Vec<u8>,
}

impl MissionProgress {
    /// Checks the chapter flags and keeps the first [`MISSION_BUFFER_LEN`] bytes, as the game does.
    ///
    /// # Errors
    ///
    /// [`Error::MissionLength`] when the file is shorter than [`CAMPAIGN_CHAPTERS`], and
    /// [`Error::MissionFlag`] on a flag that is not a digit 0 or 1.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() < CAMPAIGN_CHAPTERS {
            return Err(Error::MissionLength(data.len()));
        }
        let data = data.get(..MISSION_BUFFER_LEN).unwrap_or(data);
        if let Some((chapter, &byte)) = data
            .iter()
            .take(CAMPAIGN_CHAPTERS)
            .enumerate()
            .find(|&(_, &b)| b != b'0' && b != b'1')
        {
            return Err(Error::MissionFlag { chapter, byte });
        }
        Ok(Self { bytes: data.to_vec() })
    }

    /// Whether each chapter is unlocked, in campaign order.
    #[must_use]
    pub fn unlocked(&self) -> [bool; CAMPAIGN_CHAPTERS] {
        let mut out = [false; CAMPAIGN_CHAPTERS];
        for (flag, &byte) in out.iter_mut().zip(&self.bytes) {
            *flag = byte == b'1';
        }
        out
    }

    /// The bytes after the chapter flags, up to [`MISSION_BUFFER_LEN`].
    #[must_use]
    pub fn tail(&self) -> &[u8] {
        self.bytes.get(CAMPAIGN_CHAPTERS..).unwrap_or_default()
    }

    /// The bytes the game reads: the whole file, or its first [`MISSION_BUFFER_LEN`] bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// `MISSION2.DAT`: Gold's World Campaign progress, the block the game copies to and from
/// memory: a little-endian word per continent, non-zero once it is won, then the chosen
/// continent's byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldCampaignProgress {
    bytes: [u8; WORLD_CAMPAIGN_LEN],
}

impl WorldCampaignProgress {
    /// Keeps the block.
    ///
    /// # Errors
    ///
    /// [`Error::Length`] when the file is not [`WORLD_CAMPAIGN_LEN`] bytes.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let bytes = data.try_into().map_err(|_| Error::Length {
            len: data.len(),
            expected: WORLD_CAMPAIGN_LEN,
        })?;
        Ok(Self { bytes })
    }

    /// The block.
    #[must_use]
    pub fn bytes(&self) -> &[u8; WORLD_CAMPAIGN_LEN] {
        &self.bytes
    }

    /// Whether each continent is won, in the table's order.
    #[must_use]
    pub fn won(&self) -> [bool; WORLD_CONTINENTS] {
        let mut won = [false; WORLD_CONTINENTS];
        let (words, _) = self.bytes.as_chunks::<2>();
        for (flag, &word) in won.iter_mut().zip(words) {
            *flag = u16::from_le_bytes(word) != 0;
        }
        won
    }

    /// The continent the world screen marks as chosen.
    #[must_use]
    pub const fn chosen(&self) -> u8 {
        self.bytes[WORLD_CAMPAIGN_LEN - 1]
    }
}

/// `RES.DAT`: the screen mode, stored as an ASCII digit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenMode {
    digit: u8,
    trailer: Vec<u8>,
}

impl ScreenMode {
    /// Reads the digit and keeps any bytes after it.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] on an empty file.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let (&digit, trailer) = data.split_first().ok_or(Truncated { offset: 0, needed: 1 })?;
        Ok(Self {
            digit,
            trailer: trailer.to_vec(),
        })
    }

    /// The mode as the game computes it: the byte minus `'0'`, wrapping as a 16-bit value.
    ///
    /// The game then lowers it to what memory allows: 0 up to 6,000,000 bytes, 1 up to
    /// 6,500,000, otherwise 2.
    #[must_use]
    pub fn mode(&self) -> u16 {
        u16::from(self.digit).wrapping_sub(u16::from(b'0'))
    }

    /// The stored byte.
    #[must_use]
    pub fn digit(&self) -> u8 {
        self.digit
    }

    /// Bytes after the digit, which the game ignores; a NUL in the shipped file.
    #[must_use]
    pub fn trailer(&self) -> &[u8] {
        &self.trailer
    }
}
