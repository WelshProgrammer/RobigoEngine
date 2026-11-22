//! Reading a mod's own samples: WAV, Ogg Vorbis or FLAC at any rate, 8 or 16 bits, mono or
//! stereo, widened to 16-bit stereo for the smooth playback path.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use s2_audio::CHANNELS;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as Decode;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use crate::{Error, Result};

/// The tag naming the first frame of the stretch of a file that repeats.
pub const LOOP_START: &str = "LOOPSTART";

/// The tag naming how many frames repeat.
pub const LOOP_LENGTH: &str = "LOOPLENGTH";

/// The endings a sample file can have, in the order a mod's folder is searched.
pub const ENDINGS: [&str; 3] = ["wav", "ogg", "flac"];

/// A sample's frames, left then right, and the rate they were recorded at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// Frames a second.
    pub rate: u32,
    /// The frames.
    pub frames: Arc<[[i16; CHANNELS]]>,
}

/// The audio in `bytes`, the file at `path`, as 16-bit stereo: a mono file's one channel goes to
/// both sides, and a file with more than two keeps its first two.
///
/// # Errors
///
/// [`Error::Audio`] for a file the decoders cannot read or that holds no audio.
pub fn decode(path: &Path, bytes: Vec<u8>) -> Result<Decoded> {
    let mut stream = AudioStream::from_source(path, Box::new(Cursor::new(bytes)))?;
    let mut frames = Vec::new();
    while stream.read(&mut frames)? {}
    if frames.is_empty() {
        return Err(Error::Audio {
            path: path.to_owned(),
            reason: "no audio".into(),
        });
    }
    Ok(Decoded {
        rate: stream.rate,
        frames: frames.into(),
    })
}

/// A file's audio read a packet at a time, as 16-bit stereo like [`decode`]'s.
pub struct AudioStream {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    rate: u32,
    repeat: Option<(u64, u64)>,
    scratch: Vec<i16>,
}

impl std::fmt::Debug for AudioStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioStream")
            .field("path", &self.path)
            .field("rate", &self.rate)
            .finish_non_exhaustive()
    }
}

impl AudioStream {
    /// Opens the file at `path` for reading as it plays.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file does not open and [`Error::Audio`] when the decoders cannot
    /// read it.
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(Error::io(path))?;
        Self::from_source(path, Box::new(file))
    }

    fn from_source(path: &Path, source: Box<dyn MediaSource>) -> Result<Self> {
        let fail = |reason: String| Error::Audio {
            path: path.to_owned(),
            reason,
        };
        let source = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(ending) = path.extension().and_then(|ending| ending.to_str()) {
            hint.with_extension(ending);
        }
        let mut format = symphonia::default::get_probe()
            .probe(
                &hint,
                source,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|error| fail(error.to_string()))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| fail("no audio track".into()))?;
        let id = track.id;
        let parameters = track
            .codec_params
            .as_ref()
            .and_then(|parameters| parameters.audio())
            .ok_or_else(|| fail("no audio parameters".into()))?;
        let rate = parameters
            .sample_rate
            .ok_or_else(|| fail("no sample rate".into()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(parameters, &AudioDecoderOptions::default())
            .map_err(|error| fail(error.to_string()))?;
        let repeat = repeat(&mut *format);
        Ok(Self {
            path: path.to_owned(),
            format,
            decoder,
            track: id,
            rate,
            repeat,
            scratch: Vec::new(),
        })
    }

    /// The frames that repeat, as the first and how many, when the file's tags name them with
    /// [`LOOP_START`] and [`LOOP_LENGTH`].
    #[must_use]
    pub const fn repeat(&self) -> Option<(u64, u64)> {
        self.repeat
    }

    /// Frames a second.
    #[must_use]
    pub const fn rate(&self) -> u32 {
        self.rate
    }

    /// Adds the next packet's frames to `frames`; `false` once the file has no more.
    ///
    /// # Errors
    ///
    /// [`Error::Audio`] for a packet that cannot be read.
    pub fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> Result<bool> {
        let fail = |reason: String| Error::Audio {
            path: self.path.clone(),
            reason,
        };
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(false),
                Err(error) => return Err(fail(error.to_string())),
            };
            if packet.track_id != self.track {
                continue;
            }
            let buffer = match self.decoder.decode(&packet) {
                Ok(buffer) => buffer,
                Err(Decode::DecodeError(_)) => continue,
                Err(error) => return Err(fail(error.to_string())),
            };
            let channels = buffer.spec().channels().count().max(1);
            buffer.copy_to_vec_interleaved(&mut self.scratch);
            frames.extend(self.scratch.chunks_exact(channels).map(|frame| {
                let left = frame[0];
                [left, frame.get(1).copied().unwrap_or(left)]
            }));
            return Ok(true);
        }
    }
}

/// The repeat a file's tags name, from its whole-file tags or its track's.
fn repeat(format: &mut dyn FormatReader) -> Option<(u64, u64)> {
    let mut metadata = format.metadata();
    let revision = metadata.skip_to_latest()?;
    let tags = revision.media.tags.iter().chain(
        revision
            .per_track
            .iter()
            .flat_map(|track| track.metadata.tags.iter()),
    );
    let (mut start, mut length) = (None, None);
    for tag in tags {
        let value = tag.raw.value.to_string().trim().parse::<u64>().ok();
        if tag.raw.key.eq_ignore_ascii_case(LOOP_START) {
            start = value;
        } else if tag.raw.key.eq_ignore_ascii_case(LOOP_LENGTH) {
            length = value;
        }
    }
    Some((start?, length.filter(|&length| length > 0)?))
}
