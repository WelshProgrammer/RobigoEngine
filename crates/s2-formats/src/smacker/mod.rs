//! Smacker video (`SMK2` and `SMK4`), as used for the intro film.
//!
//! [`Video::parse`] reads the header, frame tables and Huffman trees; [`Video::decoder`] then
//! yields frames in order as an [`IndexedImage`], the current [`Palette`] and each audio track's
//! samples. The decoder reuses its buffers, so once they have grown to size it allocates only
//! the small Huffman trees each audio chunk carries.
//!
//! The format is described in `docs/formats/smacker.md`.

mod audio;
mod bits;
mod header;
mod huffman;
mod palette;
#[cfg(test)]
mod tests;
mod video;

pub use audio::{MAX_CHUNK_BYTES, Pcm};
pub use header::{AudioCodec, AudioTrack, HEADER_SIZE, Header, MAX_DIMENSION, MAX_TRACKS, Scaling, Version};

use crate::{
    bytes::{Reader, Truncated, slice, to_usize},
    image::IndexedImage,
    palette::Palette,
};

/// Why a Smacker file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A table or chunk runs past the end of the input.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The file does not start with `SMK2` or `SMK4`.
    #[error("not a Smacker file (signature {0:02x?})")]
    Signature([u8; 4]),
    /// Width or height is zero or above [`MAX_DIMENSION`].
    #[error("unsupported picture size {width}x{height}")]
    Dimensions {
        /// Width from the header.
        width: u32,
        /// Height from the header.
        height: u32,
    },
    /// A frame's data runs past the end of the input.
    #[error("frame {0} runs past the end of the file")]
    FrameBounds(u32),
    /// A bitstream ended in the middle of a code.
    #[error("bitstream ended early")]
    OutOfBits,
    /// A Huffman tree has more nodes than its value range allows.
    #[error("Huffman tree too large")]
    TreeTooLarge,
    /// A palette chunk is malformed.
    #[error("malformed palette chunk")]
    Palette,
    /// An audio chunk is malformed.
    #[error("malformed audio chunk: {0}")]
    Audio(&'static str),
    /// A track uses Bink audio, which this decoder does not handle.
    #[error("Bink audio is not supported")]
    BinkAudio,
}

/// Where one frame's data sits and what it contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameInfo {
    /// Offset of the frame's data from the start of the file.
    pub offset: usize,
    /// Length of the frame's data in bytes.
    pub size: usize,
    /// Whether the frame redraws the whole picture.
    pub keyframe: bool,
    /// Whether the frame starts with a palette chunk.
    pub palette: bool,
    /// Bit `n` is set when the frame carries a chunk for audio track `n`.
    pub audio: u8,
}

/// A parsed Smacker file borrowing its bytes.
#[derive(Debug, Clone)]
pub struct Video<'a> {
    data: &'a [u8],
    header: Header,
    frames: Vec<FrameInfo>,
    trees: video::Trees,
}

impl<'a> Video<'a> {
    /// Parses the header, frame tables and Huffman trees.
    ///
    /// # Errors
    ///
    /// [`Error::Signature`] or [`Error::Dimensions`] for an unusable header,
    /// [`Error::Truncated`] or [`Error::FrameBounds`] when tables or frames run past the end,
    /// and [`Error::OutOfBits`] or [`Error::TreeTooLarge`] for malformed trees.
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(data);
        let header = Header::parse(&mut reader)?;
        let count = to_usize(header.frame_count)
            .checked_add(usize::from(header.ring_frame))
            .ok_or(Truncated {
                offset: HEADER_SIZE,
                needed: usize::MAX,
            })?;
        let table_bytes = count.checked_mul(5).ok_or(Truncated {
            offset: HEADER_SIZE,
            needed: usize::MAX,
        })?;
        slice(data, HEADER_SIZE, table_bytes)?;
        let sizes = reader.bytes(count * 4)?;
        let types = reader.bytes(count)?;
        let trees = video::Trees::read(reader.bytes(to_usize(header.trees_size))?)?;
        let mut offset = reader.position();
        let mut frames = Vec::with_capacity(count);
        for (index, (size, &kind)) in (0u32..).zip(sizes.as_chunks::<4>().0.iter().zip(types)) {
            // A size's low two bits are flags (bit 0 keyframe, bit 1 unknown). A type byte's bit 0
            // flags a palette chunk and bits 1 to 7 the audio tracks (`docs/formats/smacker.md`).
            let word = u32::from_le_bytes([size[0], size[1], size[2], size[3]]);
            let size = to_usize(word & !3);
            let end = offset
                .checked_add(size)
                .filter(|&end| end <= data.len())
                .ok_or(Error::FrameBounds(index))?;
            frames.push(FrameInfo {
                offset,
                size,
                keyframe: word & 1 != 0,
                palette: kind & 1 != 0,
                audio: kind >> 1,
            });
            offset = end;
        }
        Ok(Self {
            data,
            header,
            frames,
            trees,
        })
    }

    /// The parsed header.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Every frame's location and contents, including the ring frame if present.
    #[must_use]
    pub fn frames(&self) -> &[FrameInfo] {
        &self.frames
    }

    /// A decoder positioned at the first frame.
    #[must_use]
    pub fn decoder(&self) -> Decoder<'_> {
        Decoder::new(self)
    }
}

/// One decoded frame, borrowed from the [`Decoder`] until the next call.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'d> {
    /// Zero-based frame number.
    pub index: u32,
    /// Whether the frame redraws the whole picture.
    pub keyframe: bool,
    /// Whether this frame changed the palette.
    pub palette_changed: bool,
    /// The picture, `width` by coded `height` (see [`Scaling`] for display).
    pub image: &'d IndexedImage,
    /// The palette in effect for this frame.
    pub palette: &'d Palette,
    audio: [Option<Pcm<'d>>; MAX_TRACKS],
}

impl<'d> Frame<'d> {
    /// The samples this frame carries for `track`, or `None` when it has no chunk for it.
    ///
    /// Samples follow the track's [`AudioTrack`] format: interleaved left then right when
    /// stereo, unsigned 8-bit or signed 16-bit.
    #[must_use]
    pub fn audio(&self, track: usize) -> Option<Pcm<'d>> {
        self.audio.get(track).copied().flatten()
    }
}

/// Decodes frames in order, keeping the picture and palette between them.
#[derive(Debug, Clone)]
pub struct Decoder<'v> {
    video: &'v Video<'v>,
    next: usize,
    image: IndexedImage,
    palette: Palette,
    audio: [Option<audio::Buffer>; MAX_TRACKS],
    pictures: bool,
}

impl<'v> Decoder<'v> {
    fn new(video: &'v Video<'v>) -> Self {
        let header = &video.header;
        Self {
            video,
            next: 0,
            image: IndexedImage::filled(header.width, header.height, 0),
            palette: Palette::default(),
            audio: header.tracks.map(|t| t.as_ref().map(audio::Buffer::for_track)),
            pictures: true,
        }
    }

    /// The same decoder skipping the pictures, for reading the sound alone: each frame's image
    /// stays black and its palette still follows the file.
    #[must_use]
    pub fn sound_only(self) -> Self {
        Self {
            pictures: false,
            ..self
        }
    }

    /// The number of the frame the next call to [`Self::decode_next`] decodes.
    #[must_use]
    pub fn position(&self) -> usize {
        self.next
    }

    /// Returns to the first frame with a black picture and palette.
    pub fn rewind(&mut self) {
        self.next = 0;
        self.image.pixels_mut().fill(0);
        self.palette = Palette::default();
    }

    /// Decodes the next frame, or returns `None` after the last one. The ring frame is never
    /// played.
    ///
    /// After an error the decoder moves on to the following frame, whose picture may show
    /// damage until the next keyframe.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`], [`Error::Palette`], [`Error::Audio`], [`Error::BinkAudio`] or
    /// [`Error::OutOfBits`] when the frame's data is malformed.
    pub fn decode_next(&mut self) -> Result<Option<Frame<'_>>, Error> {
        let video = self.video;
        if self.next >= to_usize(video.header.frame_count) {
            return Ok(None);
        }
        let Some(info) = video.frames.get(self.next).copied() else {
            return Ok(None);
        };
        let index = u32::try_from(self.next).unwrap_or(u32::MAX);
        self.next += 1;
        let mut reader = Reader::new(slice(video.data, info.offset, info.size)?);
        if info.palette {
            let len = usize::from(reader.u8()?) * 4;
            let body = reader.bytes(len.checked_sub(1).ok_or(Error::Palette)?)?;
            palette::apply(&mut self.palette, body)?;
        }
        let mut present = [false; MAX_TRACKS];
        for (track, (buffer, present)) in self.audio.iter_mut().zip(&mut present).enumerate() {
            if let Some(buffer) = buffer.as_mut() {
                buffer.clear();
            }
            if info.audio >> track & 1 == 0 {
                continue;
            }
            let len = to_usize(reader.u32()?);
            let body = reader.bytes(len.checked_sub(4).ok_or(Error::Audio("bad length"))?)?;
            if let (Some(buffer), Some(format)) = (buffer, &video.header.tracks[track]) {
                audio::decode(format, body, buffer)?;
                *present = true;
            }
        }
        if self.pictures {
            let rest = reader.bytes(reader.remaining())?;
            video::decode(&video.trees, video.header.version, rest, &mut self.image)?;
        }
        let mut audio = [None; MAX_TRACKS];
        for ((out, buffer), &present) in audio.iter_mut().zip(&self.audio).zip(&present) {
            if present {
                *out = buffer.as_ref().map(audio::Buffer::pcm);
            }
        }
        Ok(Some(Frame {
            index,
            keyframe: info.keyframe,
            palette_changed: info.palette,
            image: &self.image,
            palette: &self.palette,
            audio,
        }))
    }
}
