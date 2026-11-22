//! `ROM_ANIM.DAT`: the Roman settler animation script.

use super::{Error, Quirk, expect_tag};
use crate::bytes::Reader;

/// Bytes per record.
const RECORD_LEN: usize = 6;

/// One decoded record, as stored.
///
/// The engine keeps a record index per animating settler, advances it one record per step,
/// and acts on [`Record::step`] until it reaches a frame to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Record {
    /// 0 for a `ROM_BOBS.LST` bitmap, anything else for a BOB figure. Only read by frames.
    pub kind: u8,
    /// Bitmap id, figure job and direction, or the command's argument.
    pub arg: u16,
    /// Added to the draw column. Only read by frames.
    pub dx: i8,
    /// Added to the draw row. Only read by frames.
    pub dy: i8,
    /// 0 to 4 for a command, anything else for a frame (always 255 in the shipped file).
    pub op: u8,
}

/// What a record tells the engine to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    /// Show a frame and wait for the next step.
    Frame(Sprite),
    /// Op 0: remember this record and continue at record `target`.
    Call {
        /// Record index to continue at.
        target: u16,
    },
    /// Op 1: ends the animation and frees its state.
    End,
    /// Op 2: continue after the record that made the last call.
    Return,
    /// Op 3: change the map object under the settler. The engine passes `value - 1` to the
    /// map code; what it changes there is not yet known.
    MapObject {
        /// The stored argument, 1 to 5 in the shipped file.
        value: u16,
    },
    /// Op 4: play sound effect `id` if the settler is in view.
    Sound {
        /// Sound effect number, 1 to 57 in the shipped file.
        id: u16,
    },
}

/// What a frame draws, offset by the record's `dx` and `dy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sprite {
    /// Item `id` of `ROM_BOBS.LST`. Ids 65535 and 65475 to 65482 also appear and match
    /// no item.
    Bitmap(u16),
    /// A BOB figure; the walking frame is the record index modulo 8.
    Figure {
        /// Row of the engine's per-nation figure table, which names the BOB file, item and
        /// body type.
        job: u8,
        /// Direction index, 0 to 5 (see [`super::Direction`]).
        direction: u8,
    },
}

impl Record {
    /// Decodes one stored record.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; RECORD_LEN]) -> Self {
        let [kind, a0, a1, dx, dy, op] = bytes;
        Self {
            kind,
            arg: u16::from_le_bytes([a0, a1]),
            dx: dx.cast_signed(),
            dy: dy.cast_signed(),
            op,
        }
    }

    /// The action the engine takes on reaching this record.
    #[must_use]
    pub const fn step(&self) -> Step {
        match self.op {
            0 => Step::Call { target: self.arg },
            1 => Step::End,
            2 => Step::Return,
            3 => Step::MapObject { value: self.arg },
            4 => Step::Sound { id: self.arg },
            _ => Step::Frame(self.sprite()),
        }
    }

    /// What this record would draw if shown as a frame.
    #[must_use]
    pub const fn sprite(&self) -> Sprite {
        if self.kind == 0 {
            Sprite::Bitmap(self.arg)
        } else {
            let [job, direction] = self.arg.to_le_bytes();
            Sprite::Figure { job, direction }
        }
    }

    /// Whether every byte is 255, as in the unused records at the end of the shipped file.
    #[must_use]
    pub const fn is_blank(&self) -> bool {
        self.kind == u8::MAX && self.arg == u16::MAX && self.dx == -1 && self.dy == -1 && self.op == u8::MAX
    }
}

/// A decoded `ROM_ANIM.DAT`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RomAnim {
    records: Vec<Record>,
    quirks: Vec<Quirk>,
}

impl RomAnim {
    /// Class tag that opens the file.
    pub const TAG: u16 = 21_111;

    /// Decodes the file: the class tag, a `u16` record count and the records. Bytes after
    /// the records are ignored and reported as a quirk.
    ///
    /// # Errors
    ///
    /// [`Error::Tag`] or [`Error::Truncated`].
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        expect_tag(&mut r, Self::TAG)?;
        let count = usize::from(r.u16()?);
        let (raw, _) = r.bytes(count * RECORD_LEN)?.as_chunks::<RECORD_LEN>();
        let mut quirks = Vec::new();
        if !r.is_empty() {
            quirks.push(Quirk::TrailingBytes {
                offset: r.position(),
                len: r.remaining(),
            });
        }
        Ok(Self {
            records: raw.iter().map(|&bytes| Record::from_bytes(bytes)).collect(),
            quirks,
        })
    }

    /// What was unusual about the file; empty for the shipped one.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Every record in file order.
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The record at `index`, or `None` past the end.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Record> {
        self.records.get(index)
    }

    /// Number of records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the file holds no records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
