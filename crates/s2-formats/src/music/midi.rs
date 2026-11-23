//! MIDI status and meta bytes shared by the XMIDI reader and the SMF writer.

/// Set on every status byte and on every varint byte that has another after it.
pub const STATUS_BIT: u8 = 0x80;
/// The seven payload bits of a data or varint byte.
pub const DATA_MASK: u8 = 0x7f;
/// The message kind in the high nibble of a channel status byte.
pub const KIND_MASK: u8 = 0xf0;
/// Channels a channel status byte can address in its low nibble.
pub const CHANNELS: u8 = 16;

/// Note off. The channel status kinds below sit in the high nibble; the channel fills the low one.
pub const NOTE_OFF: u8 = 0x80;
/// Note on; XMIDI follows the key and velocity with a duration varint.
pub const NOTE_ON: u8 = 0x90;
/// Polyphonic key pressure.
pub const POLY_PRESSURE: u8 = 0xa0;
/// Control change.
pub const CONTROLLER: u8 = 0xb0;
/// Program change.
pub const PROGRAM_CHANGE: u8 = 0xc0;
/// Channel pressure.
pub const CHANNEL_PRESSURE: u8 = 0xd0;
/// Pitch wheel.
pub const PITCH_BEND: u8 = 0xe0;

/// System exclusive message.
pub const SYSEX: u8 = 0xf0;
/// System exclusive continuation or escape.
pub const SYSEX_ESCAPE: u8 = 0xf7;
/// Meta event; a type byte and a length-prefixed payload follow.
pub const META: u8 = 0xff;

/// Text meta event.
pub const META_TEXT: u8 = 0x01;
/// Marker meta event.
pub const META_MARKER: u8 = 0x06;
/// End-of-track meta event.
pub const META_END_OF_TRACK: u8 = 0x2f;
/// Tempo meta event: three bytes of microseconds per quarter note.
pub const META_TEMPO: u8 = 0x51;
