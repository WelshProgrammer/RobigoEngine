//! XMIDI songs and Miles timbre banks.
//!
//! Settlers II plays its music through the Miles Sound System (AIL 3), which takes songs in
//! XMIDI, Miles' IFF-wrapped variant of MIDI, and FM instruments from a Global Timbre Library.
//! [`Xmidi::parse`] decodes a song into sequences of absolute-time events at 120 ticks per
//! second, [`Sequence::to_smf`] renders one as a Standard MIDI File, [`Sequence::from_smf`] reads
//! one back, and [`TimbreBank::parse`] reads the OPL2 instrument bank. Playback lives elsewhere; this module only parses.

mod chunk;
pub(crate) mod midi;
mod smf;
mod timbre;
mod xmidi;

pub use timbre::{Opl2Operator, Opl2Timbre, Timbre, TimbreBank, TimbreData};
pub use xmidi::{Branch, Event, MilesControl, Sequence, TICKS_PER_SECOND, TimbreRef, TimedEvent, Xmidi};

use crate::bytes::Truncated;

/// Why a song or timbre bank failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A read ran past the end of the input or of its chunk.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// A chunk had a different identifier from the one the format requires there.
    #[error("expected {} chunk at offset {offset}, found {}", fourcc(*expected), fourcc(*found))]
    UnexpectedChunk {
        /// Offset of the chunk header.
        offset: usize,
        /// The identifier the format requires.
        expected: [u8; 4],
        /// The identifier in the file.
        found: [u8; 4],
    },
    /// A `FORM` or `CAT ` chunk had the wrong form type.
    #[error("expected form type {} at offset {offset}, found {}", fourcc(*expected), fourcc(*found))]
    UnexpectedFormType {
        /// Offset of the form type field.
        offset: usize,
        /// The form type the format requires.
        expected: [u8; 4],
        /// The form type in the file.
        found: [u8; 4],
    },
    /// The `INFO` sequence count disagrees with the number of `FORM XMID` sequences present.
    #[error("XDIR declares {declared} sequences but the catalogue holds {found}")]
    SequenceCount {
        /// The count in the `XDIR` `INFO` chunk.
        declared: u16,
        /// The number of sequences found.
        found: usize,
    },
    /// A sequence has no `EVNT` chunk.
    #[error("sequence {sequence} has no EVNT chunk")]
    MissingEvents {
        /// Zero-based index of the sequence.
        sequence: usize,
    },
    /// An event started with a byte that is not a MIDI status byte.
    #[error("invalid status byte {byte:#04x} at offset {offset}")]
    BadStatus {
        /// Offset of the byte.
        offset: usize,
        /// The byte.
        byte: u8,
    },
    /// A MIDI data byte had its top bit set.
    #[error("data byte {byte:#04x} at offset {offset} has its top bit set")]
    BadDataByte {
        /// Offset of the byte.
        offset: usize,
        /// The byte.
        byte: u8,
    },
    /// A variable-length quantity ran past four bytes.
    #[error("variable-length quantity at offset {offset} is longer than four bytes")]
    BadVarint {
        /// Offset of the first byte.
        offset: usize,
    },
    /// Absolute event times overflowed 32 bits.
    #[error("tick counter overflowed at offset {offset}")]
    TickOverflow {
        /// Offset of the event that overflowed.
        offset: usize,
    },
    /// A Standard MIDI File of format 2, whose tracks are separate songs.
    #[error("SMF format {format} is not supported; only formats 0 and 1 are")]
    SmfFormat {
        /// The format in the header.
        format: u16,
    },
    /// A Standard MIDI File whose division counts SMPTE frames, or is zero.
    #[error("SMF division {division} does not count ticks per quarter note")]
    SmfDivision {
        /// The division in the header.
        division: u16,
    },
    /// The timbre directory ran to the end of the file without its bank byte of 255 that ends it.
    #[error("timbre directory has no terminator")]
    UnterminatedDirectory,
    /// A timbre record's length prefix is too small to cover itself.
    #[error("timbre record at offset {offset} declares length {len}")]
    BadTimbreLength {
        /// Offset of the record.
        offset: usize,
        /// The declared length, which includes the two-byte prefix.
        len: u16,
    },
    /// The timbre records together claim more bytes than the file holds, so some are aliased.
    #[error("timbre records claim more bytes than the file holds")]
    TimbreOverlap,
}

fn fourcc(id: [u8; 4]) -> String {
    id.iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect()
}
