//! Writing FLAC files of 16-bit stereo audio, with Vorbis comments.

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, MetadataBlockData};
use flacenc::error::{SourceError, Verify};
use flacenc::source::{Fill, Source};

/// Bytes in one stereo frame of 16-bit audio.
pub(crate) const FRAME_LEN: usize = 4;

/// Bytes in one 16-bit sample.
pub(crate) const SAMPLE_LEN: usize = 2;

/// The metadata block type of Vorbis comments.
const VORBIS_COMMENT: u8 = 4;

/// What the Vorbis comments name as the encoder.
const VENDOR: &str = "robigo";

/// `source` encoded as a FLAC file, with `comments` as its Vorbis comments, or what the encoder
/// said went wrong.
pub(crate) fn encode<S: Source>(source: S, comments: &[(&str, String)]) -> Result<Vec<u8>, String> {
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, error)| error.to_string())?;
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|error| error.to_string())?;
    // The FLAC format leaves a stream's last block out of its minimum block size, and a decoder
    // reads a minimum below the maximum as variable blocking. The encoder counts the short last
    // block, so the minimum goes back to the fixed size.
    let block = stream.stream_info().max_block_size();
    stream
        .stream_info_mut()
        .set_block_sizes(block, block)
        .map_err(|error| error.to_string())?;
    if !comments.is_empty() {
        let block = MetadataBlockData::new_unknown(VORBIS_COMMENT, &vorbis_comments(comments))
            .map_err(|error| error.to_string())?;
        stream.add_metadata_block(block);
    }
    let mut sink = ByteSink::new();
    stream.write(&mut sink).map_err(|error| error.to_string())?;
    Ok(sink.into_inner())
}

/// A Vorbis comment block's body: the vendor, then each comment as `NAME=value`, every string
/// after its length as a little-endian `u32`.
fn vorbis_comments(comments: &[(&str, String)]) -> Vec<u8> {
    let mut body = Vec::new();
    let string = |body: &mut Vec<u8>, text: &str| {
        body.extend_from_slice(&u32::try_from(text.len()).unwrap_or(u32::MAX).to_le_bytes());
        body.extend_from_slice(text.as_bytes());
    };
    string(&mut body, VENDOR);
    body.extend_from_slice(&u32::try_from(comments.len()).unwrap_or(u32::MAX).to_le_bytes());
    for (name, value) in comments {
        string(&mut body, &format!("{name}={value}"));
    }
    body
}

/// Stereo frames in memory, handed to the encoder a block at a time.
pub(crate) struct Pcm<'a> {
    frames: &'a [[i16; 2]],
    rate: u32,
    next: usize,
    bytes: Vec<u8>,
}

impl<'a> Pcm<'a> {
    /// `frames` at `rate` frames a second.
    pub(crate) fn new(frames: &'a [[i16; 2]], rate: u32) -> Self {
        Self {
            frames,
            rate,
            next: 0,
            bytes: Vec::new(),
        }
    }
}

impl Source for Pcm<'_> {
    fn channels(&self) -> usize {
        2
    }

    fn bits_per_sample(&self) -> usize {
        16
    }

    fn sample_rate(&self) -> usize {
        usize::try_from(self.rate).unwrap_or(usize::MAX)
    }

    fn read_samples<F: Fill>(&mut self, block_size: usize, dest: &mut F) -> Result<usize, SourceError> {
        let end = (self.next + block_size).min(self.frames.len());
        self.bytes.clear();
        for frame in &self.frames[self.next..end] {
            for sample in frame {
                self.bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        dest.fill_le_bytes(&self.bytes, SAMPLE_LEN)?;
        let read = end - self.next;
        self.next = end;
        Ok(read)
    }

    fn len_hint(&self) -> Option<usize> {
        Some(self.frames.len())
    }
}
