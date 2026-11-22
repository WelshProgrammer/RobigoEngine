//! The fixed 104-byte header.

use std::time::Duration;

use super::Error;
use crate::bytes::Reader;

/// Bytes in the fixed header.
pub const HEADER_SIZE: usize = 104;

/// Audio tracks a file can carry.
pub const MAX_TRACKS: usize = 7;

/// The largest width or height accepted, which caps a decoder's frame buffer at 4 MiB.
pub const MAX_DIMENSION: u16 = 2048;

/// Which revision of the format a file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    /// `SMK2`: the original block coder.
    Smk2,
    /// `SMK4`: adds the double and half variants of full blocks.
    Smk4,
}

/// How rows of the coded picture map onto the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scaling {
    /// One coded row per display row.
    None,
    /// Each coded row is shown on every other display row; the rest stay black.
    Interlaced,
    /// Each coded row is shown twice.
    Doubled,
}

/// How a track's audio chunks are coded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCodec {
    /// Plain PCM: unsigned 8-bit or signed little-endian 16-bit, interleaved.
    Raw,
    /// Smacker's Huffman-coded sample deltas.
    Dpcm,
    /// Bink audio (RDFT or DCT), which this decoder does not handle.
    Bink,
}

/// One audio track's format, from the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioTrack {
    /// Samples per second per channel.
    pub rate: u32,
    /// 1 for mono, 2 for stereo.
    pub channels: u8,
    /// Whether samples are 16-bit signed (otherwise 8-bit unsigned).
    pub sixteen_bit: bool,
    /// How chunks are coded.
    pub codec: AudioCodec,
    /// The header's bound on the decoded size of one chunk, in bytes.
    pub max_chunk_bytes: u32,
}

impl AudioTrack {
    /// Bytes per decoded sample frame (all channels).
    #[must_use]
    pub fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * if self.sixteen_bit { 2 } else { 1 }
    }
}

/// The parsed header.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Header {
    /// Format revision.
    pub version: Version,
    /// Picture width in pixels.
    pub width: u16,
    /// Coded picture height in pixels, before [`Scaling`].
    pub height: u16,
    /// Frames to play, not counting the ring frame.
    pub frame_count: u32,
    /// Time each frame stays on screen.
    pub frame_duration: Duration,
    /// Whether one extra frame follows the last, leading back to the first for looped play.
    pub ring_frame: bool,
    /// How coded rows map to display rows.
    pub scaling: Scaling,
    /// Audio tracks by index; `None` where a track is absent.
    pub tracks: [Option<AudioTrack>; MAX_TRACKS],
    /// Bytes of packed Huffman trees after the frame tables.
    pub trees_size: u32,
}

impl Header {
    pub(super) fn parse(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let signature = reader.array::<4>()?;
        let version = match &signature {
            b"SMK2" => Version::Smk2,
            b"SMK4" => Version::Smk4,
            _ => return Err(Error::Signature(signature)),
        };
        let width = reader.u32()?;
        let height = reader.u32()?;
        let (Some(w), Some(h)) = (dimension(width), dimension(height)) else {
            return Err(Error::Dimensions { width, height });
        };
        let frame_count = reader.u32()?;
        let frame_duration = frame_duration(reader.i32()?);
        let flags = reader.u32()?;
        let mut sizes = [0; MAX_TRACKS];
        for size in &mut sizes {
            *size = reader.u32()?;
        }
        let trees_size = reader.u32()?;
        // The four tree allocation hints only sized the original player's buffers.
        reader.skip(16)?;
        let mut tracks = [None; MAX_TRACKS];
        for (track, &max_chunk_bytes) in tracks.iter_mut().zip(&sizes) {
            *track = audio_track(reader.u32()?, max_chunk_bytes);
        }
        reader.skip(4)?;
        // Flags: bit 0 ring frame, bit 1 interlaced, bit 2 doubled (`docs/formats/smacker.md`).
        let scaling = match flags & 6 {
            0 => Scaling::None,
            2 => Scaling::Interlaced,
            _ => Scaling::Doubled,
        };
        Ok(Self {
            version,
            width: w,
            height: h,
            frame_count,
            frame_duration,
            ring_frame: flags & 1 != 0,
            scaling,
            tracks,
            trees_size,
        })
    }

    /// Frames per second as an exact fraction `(numerator, denominator)` in lowest terms.
    #[must_use]
    pub fn frame_rate(&self) -> (u64, u64) {
        let micros = u64::try_from(self.frame_duration.as_micros()).unwrap_or(u64::MAX);
        let divisor = gcd(1_000_000, micros);
        (1_000_000 / divisor, micros / divisor)
    }

    /// Display height in pixels after [`Scaling`].
    #[must_use]
    pub fn display_height(&self) -> u32 {
        match self.scaling {
            Scaling::None => u32::from(self.height),
            Scaling::Interlaced | Scaling::Doubled => u32::from(self.height) * 2,
        }
    }
}

fn dimension(value: u32) -> Option<u16> {
    u16::try_from(value)
        .ok()
        .filter(|&v| (1..=MAX_DIMENSION).contains(&v))
}

/// Positive values count milliseconds, negative values count units of 10 µs, zero means 10 fps.
fn frame_duration(rate: i32) -> Duration {
    let magnitude = u64::from(rate.unsigned_abs());
    match rate {
        0 => Duration::from_millis(100),
        1.. => Duration::from_millis(magnitude),
        _ => Duration::from_micros(magnitude * 10),
    }
}

/// The sample rate in the low 24 bits of a track's format word.
const AUDIO_RATE_MASK: u32 = 0x00ff_ffff;

/// Decodes a track's format word: bits 0 to 23 the rate, 27 Bink, 28 stereo, 29 16-bit, 30
/// present, 31 DPCM (`docs/formats/smacker.md`).
fn audio_track(word: u32, max_chunk_bytes: u32) -> Option<AudioTrack> {
    let rate = word & AUDIO_RATE_MASK;
    if word & (1 << 30) == 0 && rate == 0 {
        return None;
    }
    let codec = if word & (1 << 27) != 0 {
        AudioCodec::Bink
    } else if word & (1 << 31) != 0 {
        AudioCodec::Dpcm
    } else {
        AudioCodec::Raw
    };
    Some(AudioTrack {
        rate,
        channels: if word & (1 << 28) != 0 { 2 } else { 1 },
        sixteen_bit: word & (1 << 29) != 0,
        codec,
        max_chunk_bytes,
    })
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_durations() {
        assert_eq!(frame_duration(0), Duration::from_millis(100));
        assert_eq!(frame_duration(40), Duration::from_millis(40));
        assert_eq!(frame_duration(-6600), Duration::from_millis(66));
        assert_eq!(frame_duration(i32::MIN), Duration::from_micros(21_474_836_480));
    }

    #[test]
    fn audio_flags() {
        let track = audio_track(0xd000_5622, 100).unwrap();
        assert_eq!(track.rate, 22_050);
        assert_eq!(track.channels, 2);
        assert!(!track.sixteen_bit);
        assert_eq!(track.codec, AudioCodec::Dpcm);
        assert_eq!(audio_track(0, 0), None);
        let bink = audio_track(0x6800_ac44, 0).unwrap();
        assert_eq!(bink.codec, AudioCodec::Bink);
        assert!(bink.sixteen_bit);
        assert_eq!(bink.channels, 1);
    }
}
