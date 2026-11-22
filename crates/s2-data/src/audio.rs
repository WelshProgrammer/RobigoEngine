//! Red Book CD audio described by a cue sheet: the track table and raw PCM readers.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cue::{self, CueSheet, FRAMES_PER_SECOND, TrackMode};
use crate::error::{Error, IoContext, Result};
use crate::path;
use crate::sector::{Geometry, SpanReader};

/// The CD audio tracks listed by a cue sheet (`.cue`, or GOG's `.ins`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdAudio {
    sheet: PathBuf,
    tracks: Vec<AudioTrack>,
}

/// One audio track: where its PCM lives and how long it plays.
///
/// The span runs from the track's `INDEX 01` to the next track's `INDEX 00` when the sheet gives
/// one, leaving the pregap unplayed. Otherwise it runs to the next track's `INDEX 01` or the end
/// of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    number: u8,
    file: PathBuf,
    start_sector: u64,
    sector_count: u64,
}

impl CdAudio {
    /// Sample rate of every track, in hertz.
    pub const SAMPLE_RATE: u32 = 44_100;
    /// Interleaved channels per sample frame (left, then right).
    pub const CHANNELS: u16 = 2;
    /// Bits per sample; samples are signed little-endian.
    pub const BITS_PER_SAMPLE: u16 = 16;
    /// Bytes in one CD sector (1/75 of a second of audio).
    pub const SECTOR_LEN: u64 = 2352;

    /// Reads a cue sheet and builds the audio track table from it.
    ///
    /// Track files named in the sheet are looked up next to it, ignoring case. Tracks stored in
    /// anything other than `BINARY` files (WAVE, MP3 and so on) are left out of the table.
    ///
    /// # Errors
    ///
    /// Fails if the sheet cannot be read or parsed, if a track file it names is missing, or if a
    /// track starts beyond the end of its file.
    pub fn from_cue(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::from_sheet(path, &read_sheet(path)?)
    }

    pub(crate) fn from_sheet(sheet_path: &Path, sheet: &CueSheet) -> Result<Self> {
        let folder = path::folder_of(sheet_path);
        let fail = |reason: String| Error::Cue {
            path: sheet_path.to_owned(),
            line: 0,
            reason,
        };
        let mut tracks = Vec::new();
        for file in sheet.files.iter().filter(|f| f.binary) {
            if !file.tracks.iter().any(|t| t.mode == TrackMode::Audio) {
                continue;
            }
            let on_disk = locate(folder, &file.name)
                .ok_or_else(|| fail(format!("{} is not next to the cue sheet", file.name)))?;
            let file_sectors = std::fs::metadata(&on_disk).at(&on_disk)?.len() / Self::SECTOR_LEN;
            for (i, track) in file.tracks.iter().enumerate() {
                let end = file.tracks.get(i + 1).map_or(file_sectors, |next| {
                    u64::from(next.pregap.unwrap_or(next.start)).min(file_sectors)
                });
                let start = u64::from(track.start);
                if track.mode != TrackMode::Audio {
                    continue;
                }
                if start > end {
                    return Err(fail(format!(
                        "track {} starts past the end of {}, so the image is probably truncated",
                        track.number, file.name
                    )));
                }
                tracks.push(AudioTrack {
                    number: track.number,
                    file: on_disk.clone(),
                    start_sector: start,
                    sector_count: end - start,
                });
            }
        }
        Ok(Self {
            sheet: sheet_path.to_owned(),
            tracks,
        })
    }

    /// The cue sheet this table came from.
    #[must_use]
    pub fn cue_path(&self) -> &Path {
        &self.sheet
    }

    /// All audio tracks in disc order. Data tracks are not included.
    #[must_use]
    pub fn tracks(&self) -> &[AudioTrack] {
        &self.tracks
    }

    /// The audio track with the given CD track number, if there is one.
    #[must_use]
    pub fn track(&self, number: u8) -> Option<&AudioTrack> {
        self.tracks.iter().find(|t| t.number == number)
    }

    /// Opens a track for streaming its raw PCM (see the constants on [`CdAudio`] for the format).
    ///
    /// # Errors
    ///
    /// Fails with [`Error::NoSuchTrack`] if the number is not an audio track, or with
    /// [`Error::Io`] if its file cannot be opened.
    pub fn open_track(&self, number: u8) -> Result<TrackReader> {
        let track = self.track(number).ok_or(Error::NoSuchTrack(number))?;
        let file = File::open(&track.file).at(&track.file)?;
        Ok(TrackReader(SpanReader::new(
            file,
            Geometry::AUDIO,
            0,
            track.start_sector * Self::SECTOR_LEN,
            track.byte_len(),
        )))
    }
}

impl AudioTrack {
    /// The CD track number (the data track is 1, so audio usually starts at 2).
    #[must_use]
    pub fn number(&self) -> u8 {
        self.number
    }

    /// The image file holding this track's sectors.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The first sector of the track (its `INDEX 01`), counted from the start of [`Self::file`].
    #[must_use]
    pub fn start_sector(&self) -> u64 {
        self.start_sector
    }

    /// How many 2352-byte sectors the track plays for.
    #[must_use]
    pub fn sector_count(&self) -> u64 {
        self.sector_count
    }

    /// Length of the raw PCM in bytes.
    #[must_use]
    pub fn byte_len(&self) -> u64 {
        self.sector_count * CdAudio::SECTOR_LEN
    }

    /// Playing time, at 75 sectors per second.
    #[must_use]
    pub fn duration(&self) -> Duration {
        let fps = u64::from(FRAMES_PER_SECOND);
        let nanos = self.sector_count % fps * 1_000_000_000 / fps;
        Duration::new(self.sector_count / fps, u32::try_from(nanos).unwrap_or(0))
    }
}

/// Streams one CD audio track as raw 44.1 kHz, 16-bit, little-endian stereo PCM.
#[derive(Debug)]
pub struct TrackReader(SpanReader);

impl TrackReader {
    /// Total length of the track's PCM in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.0.len()
    }

    /// Whether the track has no sectors.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Read for TrackReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for TrackReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.0.seek(to)
    }
}

pub(crate) fn read_sheet(path: &Path) -> Result<CueSheet> {
    let bytes = std::fs::read(path).at(path)?;
    cue::parse(&String::from_utf8_lossy(&bytes)).map_err(|(line, reason)| Error::Cue {
        path: path.to_owned(),
        line,
        reason,
    })
}

/// Finds a file named in a cue sheet, relative to the sheet's folder and ignoring case.
pub(crate) fn locate(folder: &Path, name: &str) -> Option<PathBuf> {
    let parts = path::split(name).ok()?;
    path::find_on_disk(folder, &parts).filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, audio_sectors};

    fn disc(dir: &TempDir, cue: &str) -> CdAudio {
        let mut bin = vec![0; 10 * 2352];
        for (track, sectors) in [(2, 20), (3, 30), (4, 15)] {
            bin.extend(audio_sectors(track, sectors));
        }
        dir.write("game.BIN", &bin);
        CdAudio::from_cue(dir.write("game.cue", cue.as_bytes())).unwrap()
    }

    const PLAIN: &str = "FILE \"Game.bin\" BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\n\
                         TRACK 02 AUDIO\n  INDEX 01 00:00:10\n TRACK 03 AUDIO\n  INDEX 01 00:00:30\n\
                         TRACK 04 AUDIO\n  INDEX 01 00:00:60\n";

    #[test]
    fn tracks_end_where_the_next_starts() {
        let dir = TempDir::new();
        let audio = disc(&dir, PLAIN);
        let table: Vec<_> = audio
            .tracks()
            .iter()
            .map(|t| (t.number(), t.start_sector(), t.sector_count()))
            .collect();
        assert_eq!(table, [(2, 10, 20), (3, 30, 30), (4, 60, 15)]);
        assert_eq!(audio.track(3).unwrap().duration(), Duration::from_millis(400));
        assert_eq!(audio.track(4).unwrap().duration(), Duration::from_millis(200));
        assert!(audio.track(1).is_none());
    }

    #[test]
    fn pregaps_shorten_the_previous_track() {
        let dir = TempDir::new();
        let cue = PLAIN.replace("TRACK 03 AUDIO\n", "TRACK 03 AUDIO\n  INDEX 00 00:00:25\n");
        let audio = disc(&dir, &cue);
        assert_eq!(audio.track(2).unwrap().sector_count(), 15);
        assert_eq!(audio.track(3).unwrap().start_sector(), 30);
    }

    #[test]
    fn streams_only_the_track() {
        let dir = TempDir::new();
        let audio = disc(&dir, PLAIN);
        for (number, sectors) in [(2u8, 20), (3, 30), (4, 15)] {
            let mut reader = audio.open_track(number).unwrap();
            assert_eq!(reader.len(), sectors * 2352);
            let mut pcm = Vec::new();
            reader.read_to_end(&mut pcm).unwrap();
            assert_eq!(pcm.len() as u64, sectors * 2352);
            assert!(pcm.iter().all(|&b| b == number));
        }
        assert!(matches!(audio.open_track(9), Err(Error::NoSuchTrack(9))));
    }

    #[test]
    fn handles_one_file_per_track() {
        let dir = TempDir::new();
        dir.write("t1.bin", &[0; 2352 * 4]);
        dir.write("T2.BIN", &audio_sectors(2, 7));
        dir.write("t3.bin", &audio_sectors(3, 5));
        let cue = "FILE t1.bin BINARY\nTRACK 1 MODE1/2352\nINDEX 1 00:00:00\n\
                   FILE t2.bin BINARY\nTRACK 2 AUDIO\nINDEX 0 00:00:00\nINDEX 1 00:00:02\n\
                   FILE t3.bin BINARY\nTRACK 3 AUDIO\nINDEX 1 00:00:00\n";
        let audio = CdAudio::from_cue(dir.write("multi.cue", cue.as_bytes())).unwrap();
        let table: Vec<_> = audio
            .tracks()
            .iter()
            .map(|t| (t.number(), t.start_sector(), t.sector_count()))
            .collect();
        assert_eq!(table, [(2, 2, 5), (3, 0, 5)]);
        let mut pcm = Vec::new();
        audio.open_track(2).unwrap().read_to_end(&mut pcm).unwrap();
        assert_eq!(pcm.len(), 5 * 2352);
    }

    #[test]
    fn explains_missing_and_short_files() {
        let dir = TempDir::new();
        let err = CdAudio::from_cue(dir.write("lost.cue", PLAIN.as_bytes())).unwrap_err();
        assert!(
            err.to_string().contains("Game.bin is not next to the cue sheet"),
            "{err}"
        );
        dir.write("game.bin", &vec![0; 2352 * 40]);
        let err = CdAudio::from_cue(dir.path().join("lost.cue")).unwrap_err();
        assert!(err.to_string().contains("truncated"), "{err}");
    }
}
