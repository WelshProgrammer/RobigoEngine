//! The CD's audio tracks, ripped to FLAC from the player's disc image.

use std::fmt::Write as _;
use std::io::Read;

use flacenc::error::SourceError;
use flacenc::source::{Fill, Source};
use s2_data::{CdAudio, DataSet, TrackReader};

use crate::audio::ENDINGS;
use crate::flac::{self, FRAME_LEN, SAMPLE_LEN};
use crate::{Error, Result};

/// Where the CD's tracks go in a mod.
pub const CD: &str = "cd";

/// The bake record's entry for the disc image, which holds the track table in place of a hash
/// of every sample.
pub(crate) const CD_SOURCE: &str = "CD audio";

/// The path of audio track `track`'s FLAC file in a mod, by its number on the disc.
#[must_use]
pub fn cd_path(track: u8) -> String {
    format!("{CD}/{track}.flac")
}

/// The paths audio track `track` can take in a mod, one a file ending in [`ENDINGS`].
#[must_use]
pub fn cd_paths(track: u8) -> Vec<String> {
    ENDINGS
        .iter()
        .map(|ending| format!("{CD}/{track}.{ending}"))
        .collect()
}

/// The audio track numbers of `data`'s disc image, none without one.
pub(crate) fn cd_tracks(data: &DataSet) -> Vec<u8> {
    data.cd_audio()
        .map(|audio| audio.tracks().iter().map(s2_data::AudioTrack::number).collect())
        .unwrap_or_default()
}

/// The disc image's track table as the bake records it: each track's number, first sector and
/// length in sectors, then the length of the file that holds it. Hashing this in place of the
/// samples keeps the check for a stale bake from reading the whole image.
pub(crate) fn cd_table(audio: &CdAudio) -> String {
    let mut table = String::new();
    for track in audio.tracks() {
        let file = std::fs::metadata(track.file()).map_or(0, |meta| meta.len());
        let _ = writeln!(
            table,
            "{} {} {} {file}",
            track.number(),
            track.start_sector(),
            track.sector_count()
        );
    }
    table
}

/// Audio track `track` of `audio` as a FLAC file holding every sample.
pub(crate) fn rip(audio: &CdAudio, track: u8) -> Result<Vec<u8>> {
    let source = Track {
        reader: audio.open_track(track)?,
        bytes: Vec::new(),
    };
    flac::encode(source, &[]).map_err(|reason| Error::Rip { track, reason })
}

/// A track read straight from the image into the encoder, a block at a time.
struct Track {
    reader: TrackReader,
    bytes: Vec<u8>,
}

impl Source for Track {
    fn channels(&self) -> usize {
        usize::from(CdAudio::CHANNELS)
    }

    fn bits_per_sample(&self) -> usize {
        usize::from(CdAudio::BITS_PER_SAMPLE)
    }

    fn sample_rate(&self) -> usize {
        usize::try_from(CdAudio::SAMPLE_RATE).unwrap_or(usize::MAX)
    }

    fn read_samples<F: Fill>(
        &mut self,
        block_size: usize,
        dest: &mut F,
    ) -> std::result::Result<usize, SourceError> {
        self.bytes.resize(block_size * FRAME_LEN, 0);
        let mut filled = 0;
        while filled < self.bytes.len() {
            match self.reader.read(&mut self.bytes[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(SourceError::from_io_error(error)),
            }
        }
        let frames = filled / FRAME_LEN;
        dest.fill_le_bytes(&self.bytes[..frames * FRAME_LEN], SAMPLE_LEN)?;
        Ok(frames)
    }

    fn len_hint(&self) -> Option<usize> {
        usize::try_from(self.reader.len()).ok().map(|len| len / FRAME_LEN)
    }
}
