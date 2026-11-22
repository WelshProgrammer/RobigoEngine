//! Audio chunk decoding: raw PCM and Smacker's Huffman-coded deltas.

use super::{AudioCodec, AudioTrack, Error, bits::BitReader, huffman::ByteTree};
use crate::bytes::{Reader, to_usize};

/// The largest decoded chunk accepted, whatever the header claims.
pub const MAX_CHUNK_BYTES: usize = 4 << 20;

/// Decoded samples for one track of one frame, interleaved left then right when stereo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pcm<'a> {
    /// Unsigned 8-bit samples; silence is 128.
    U8(&'a [u8]),
    /// Signed 16-bit samples; silence is 0.
    I16(&'a [i16]),
}

impl Pcm<'_> {
    /// Number of samples across all channels.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::U8(s) => s.len(),
            Self::I16(s) => s.len(),
        }
    }

    /// Whether there are no samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A reusable output buffer for one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Buffer {
    U8(Vec<u8>),
    I16(Vec<i16>),
}

impl Buffer {
    pub(super) fn for_track(track: &AudioTrack) -> Self {
        if track.sixteen_bit {
            Self::I16(Vec::new())
        } else {
            Self::U8(Vec::new())
        }
    }

    pub(super) fn clear(&mut self) {
        match self {
            Self::U8(v) => v.clear(),
            Self::I16(v) => v.clear(),
        }
    }

    pub(super) fn pcm(&self) -> Pcm<'_> {
        match self {
            Self::U8(v) => Pcm::U8(v),
            Self::I16(v) => Pcm::I16(v),
        }
    }
}

/// Decodes one chunk body (after its length field) into `out`, which starts empty.
pub(super) fn decode(track: &AudioTrack, body: &[u8], out: &mut Buffer) -> Result<(), Error> {
    match track.codec {
        AudioCodec::Raw => raw(track, body, out),
        AudioCodec::Dpcm => dpcm(track, body, out),
        AudioCodec::Bink => Err(Error::BinkAudio),
    }
}

fn raw(track: &AudioTrack, body: &[u8], out: &mut Buffer) -> Result<(), Error> {
    if !body.len().is_multiple_of(track.frame_bytes()) {
        return Err(Error::Audio("raw chunk is not a whole number of samples"));
    }
    match out {
        Buffer::U8(v) => v.extend_from_slice(body),
        Buffer::I16(v) => v.extend(
            body.as_chunks::<2>()
                .0
                .iter()
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
        ),
    }
    Ok(())
}

fn dpcm(track: &AudioTrack, body: &[u8], out: &mut Buffer) -> Result<(), Error> {
    let mut reader = Reader::new(body);
    let unpacked = to_usize(reader.u32()?);
    if unpacked > to_usize(track.max_chunk_bytes) || unpacked > MAX_CHUNK_BYTES {
        return Err(Error::Audio("chunk is larger than the header allows"));
    }
    let mut bits = BitReader::new(body.get(4..).unwrap_or_default());
    if !bits.bit()? {
        return Ok(());
    }
    let stereo = bits.bit()?;
    let sixteen_bit = bits.bit()?;
    if stereo != (track.channels == 2) || sixteen_bit != track.sixteen_bit {
        return Err(Error::Audio("chunk format differs from the header"));
    }
    let channels = usize::from(track.channels);
    let frames = unpacked / track.frame_bytes();
    let mut trees: [ByteTree; 4] = Default::default();
    let tree_count = channels * if sixteen_bit { 2 } else { 1 };
    for tree in trees.iter_mut().take(tree_count) {
        *tree = ByteTree::read(&mut bits)?;
    }
    if frames == 0 {
        return Ok(());
    }
    match out {
        Buffer::U8(v) => {
            v.reserve(frames * channels);
            let mut last = [0u8; 2];
            // The first samples are stored right channel first.
            for slot in last[..channels].iter_mut().rev() {
                *slot = bits.u8()?;
            }
            v.extend_from_slice(&last[..channels]);
            for _ in 1..frames {
                for (slot, tree) in last[..channels].iter_mut().zip(&trees) {
                    *slot = slot.wrapping_add(tree.decode(&mut bits)?);
                    v.push(*slot);
                }
            }
        }
        Buffer::I16(v) => {
            v.reserve(frames * channels);
            let mut last = [0i16; 2];
            // The first samples are stored right channel first, high byte then low.
            for slot in last[..channels].iter_mut().rev() {
                let high = bits.u8()?;
                let low = bits.u8()?;
                *slot = i16::from_be_bytes([high, low]);
            }
            v.extend_from_slice(&last[..channels]);
            for _ in 1..frames {
                for (slot, pair) in last[..channels].iter_mut().zip(trees.as_chunks::<2>().0) {
                    let low = pair[0].decode(&mut bits)?;
                    let high = pair[1].decode(&mut bits)?;
                    *slot = slot.wrapping_add(i16::from_le_bytes([low, high]));
                    v.push(*slot);
                }
            }
        }
    }
    Ok(())
}
