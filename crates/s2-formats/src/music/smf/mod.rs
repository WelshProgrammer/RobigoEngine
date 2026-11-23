//! Standard MIDI Files: writing a decoded sequence as one, and reading one back.

mod read;
mod write;

/// Ticks per quarter note; with the default 500,000 µs quarter this gives 120 ticks per second.
const DIVISION: u16 = 60;

/// The tempo a file starts at until a tempo event says otherwise, in µs per quarter note.
const TEMPO_US_PER_QUARTER: u32 = 500_000;

/// The largest delta a four-byte variable-length quantity holds.
const MAX_DELTA: u32 = 0x0FFF_FFFF;

/// The length of the `MThd` body the writer gives and the reader needs at least.
const HEADER_LEN: u32 = 6;

/// The marker the writer puts before a Miles for-loop.
const LOOP_START: &[u8] = b"loopStart";

/// The marker the writer puts before a Miles next-loop that jumps.
const LOOP_END: &[u8] = b"loopEnd";

/// The lowest next-loop value that jumps back; Miles ignores lower ones.
const LOOP_JUMP: u8 = 64;
