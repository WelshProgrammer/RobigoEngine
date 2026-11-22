//! The songs, rendered for the game: each played through S2's FM driver into an emulated OPL3,
//! or through a sound font as General MIDI, and written as a FLAC file whose tags name the
//! stretch that repeats (`docs/design/mods.md`). The game plays the files; nothing reads MIDI
//! while it runs. The base mod keeps the songs' XMIDI files too, for rendering them again through
//! another sound font.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use s2_audio::{GeneralMidi, Instrument, Opl3, Score, Song, render_song};
use s2_data::DataSet;
use s2_formats::music::{Error as MusicError, Sequence, TimbreBank, Xmidi};

use crate::audio::{ENDINGS, LOOP_LENGTH, LOOP_START};
use crate::flac::{self, Pcm};
use crate::{BASE, Error, Result};

/// Which rendering of the songs plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rendition {
    /// S2's FM driver on an OPL3, as S2 plays them.
    #[default]
    Opl3,
    /// General MIDI through a sound font.
    GeneralMidi,
}

impl Rendition {
    /// The folder its songs go in, in a mod.
    #[must_use]
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Opl3 => "music/opl",
            Self::GeneralMidi => "music/midi",
        }
    }
}

/// The sound font a mod ships for the player to render the General MIDI songs with.
pub const SOUND_FONT: &str = "music/soundfont.sf2";

/// The record of what the General MIDI songs were rendered with, beside them.
const MIDI_RECORD: &str = "render.toml";

/// The General MIDI renderer's version: songs rendered by another are rendered again.
const MIDI_RENDERER: u32 = 1;

/// What the General MIDI songs were rendered with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct MidiRecord {
    renderer: u32,
    sound_font: String,
}

/// The songs the MIDI player offers, numbered from 1 as `S2.EXE` numbers them.
pub const SONGS: u8 = 14;

/// The FM instruments the songs play with: the bank the OPL3 driver loads.
const BANK: &str = "DRIVERS/SETT_SND.OPL";

/// The path of song `number` of `rendition` in a mod, as the base mod writes it.
#[must_use]
pub fn song_path(rendition: Rendition, number: u8) -> String {
    format!("{}/{number}.flac", rendition.folder())
}

/// The paths song `number` of `rendition` can take in a mod, one a file ending in [`ENDINGS`].
#[must_use]
pub fn song_paths(rendition: Rendition, number: u8) -> Vec<String> {
    ENDINGS
        .iter()
        .map(|ending| format!("{}/{number}.{ending}", rendition.folder()))
        .collect()
}

/// The path of song `number` in the original data.
fn original_song(number: u8) -> String {
    format!("DATA/SOUNDDAT/SNG/SNG_{number:04}.DAT")
}

/// The path of song `number`'s XMIDI file in the base mod, which the General MIDI songs are
/// rendered from.
fn score_path(number: u8) -> String {
    format!("music/score/{number}.xmi")
}

/// The songs' XMIDI files as the base mod keeps them, each with its path. The General MIDI songs
/// are rendered from these, so a new sound font needs no game data.
pub(crate) fn score_files(data: &DataSet) -> Result<Vec<(String, Vec<u8>)>> {
    (1..=SONGS)
        .filter(|&number| data.exists(&original_song(number)))
        .map(|number| Ok((score_path(number), data.read(&original_song(number))?)))
        .collect()
}

/// The original files the songs are rendered from, of those `data` has.
pub(crate) fn music_files(data: &DataSet) -> Vec<String> {
    (1..=SONGS)
        .map(original_song)
        .chain([BANK.to_owned()])
        .filter(|path| data.exists(path))
        .collect()
}

/// One song to render: its number, its score and the instruments it plays with.
pub(crate) struct SongJob {
    pub(crate) number: u8,
    score: Score,
    bank: Arc<TimbreBank>,
}

/// The songs of `data` to render, none when it has no FM bank.
pub(crate) fn song_jobs(data: &DataSet) -> Result<Vec<SongJob>> {
    if !data.exists(BANK) {
        return Ok(Vec::new());
    }
    let bank = Arc::new(TimbreBank::parse(&data.read(BANK)?).map_err(music_error(BANK))?);
    let mut jobs = Vec::new();
    for number in 1..=SONGS {
        let path = original_song(number);
        if data.exists(&path) {
            let song = first_sequence(&path, &data.read(&path)?)?;
            jobs.push(SongJob {
                number,
                score: Score::new(&song),
                bank: Arc::clone(&bank),
            });
        }
    }
    Ok(jobs)
}

/// The song of `job` rendered and written as a FLAC file at the OPL3's own rate.
pub(crate) fn render(job: &SongJob) -> Result<Vec<u8>> {
    let mut opl = Opl3::new((*job.bank).clone());
    encode(job.number, &mut opl, job.score.clone())
}

/// Song `number` played through `instrument` and written as a FLAC file tagged with its repeat.
fn encode(number: u8, instrument: &mut dyn Instrument, score: Score) -> Result<Vec<u8>> {
    let rate = instrument.rate();
    let Song { frames, repeat, .. } = render_song(instrument, score);
    let tags: Vec<(&str, String)> = repeat
        .map(|(start, length)| vec![(LOOP_START, start.to_string()), (LOOP_LENGTH, length.to_string())])
        .unwrap_or_default();
    flac::encode(Pcm::new(&frames, rate), &tags).map_err(|reason| Error::Song { song: number, reason })
}

/// Whether the base mod in `root` holds General MIDI songs rendered by this renderer with the
/// sound font `identity` names ([`crate::Bank::identity`]).
#[must_use]
pub fn midi_current(root: &Path, identity: &str) -> bool {
    let record = root
        .join(BASE)
        .join(Rendition::GeneralMidi.folder())
        .join(MIDI_RECORD);
    std::fs::read_to_string(record)
        .ok()
        .and_then(|text| toml::from_str::<MidiRecord>(&text).ok())
        .is_some_and(|record| record.renderer == MIDI_RENDERER && record.sound_font == identity)
}

/// Renders the songs the base mod in `root` keeps as XMIDI as General MIDI through the sound
/// font at `font`, into that base mod, replacing any rendered before, and records `identity` as what they were
/// rendered with. The songs render side by side, into a folder that takes the old one's place
/// once every song is written.
///
/// # Errors
///
/// [`Error::SoundFont`] for a sound font that does not load, [`Error::Music`] for a song that
/// does not decode, [`Error::Song`] for one that does not encode, and [`Error::Io`] for a file
/// that cannot be read or written.
pub fn render_midi(root: &Path, font: &Path, identity: &str) -> Result<()> {
    let failed = |reason: String| Error::SoundFont {
        name: font.display().to_string(),
        reason,
    };
    let mut file = std::fs::File::open(font).map_err(Error::io(font))?;
    let font_data =
        Arc::new(rustysynth::SoundFont::new(&mut file).map_err(|error| failed(error.to_string()))?);
    let folder = root.join(BASE).join(Rendition::GeneralMidi.folder());
    let rendering = folder.with_extension("rendering");
    let _ = std::fs::remove_dir_all(&rendering);
    std::fs::create_dir_all(&rendering).map_err(Error::io(&rendering))?;
    let scores: Vec<(u8, PathBuf)> = (1..=SONGS)
        .map(|number| (number, root.join(BASE).join(score_path(number))))
        .filter(|(_, path)| path.is_file())
        .collect();
    scores.par_iter().try_for_each(|(number, score)| {
        let song = first_sequence(
            &score_path(*number),
            &std::fs::read(score).map_err(Error::io(score))?,
        )?;
        let mut synth = GeneralMidi::new(&font_data).map_err(failed)?;
        let bytes = encode(*number, &mut synth, Score::new(&song))?;
        let path = rendering.join(format!("{number}.flac"));
        std::fs::write(&path, bytes).map_err(Error::io(&path))
    })?;
    let record = MidiRecord {
        renderer: MIDI_RENDERER,
        sound_font: identity.to_owned(),
    };
    let text = toml::to_string(&record).map_err(|source| Error::Write {
        path: MIDI_RECORD.into(),
        source,
    })?;
    let path = rendering.join(MIDI_RECORD);
    std::fs::write(&path, text).map_err(Error::io(&path))?;
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::rename(&rendering, &folder).map_err(Error::io(&folder))?;
    Ok(())
}

/// The first sequence of the XMIDI file `bytes` from data path `path`, the one `S2.EXE` plays.
fn first_sequence(path: &str, bytes: &[u8]) -> Result<Sequence> {
    Xmidi::parse(bytes)
        .map_err(music_error(path))?
        .sequences
        .into_iter()
        .next()
        .ok_or_else(|| music_error(path)(MusicError::MissingEvents { sequence: 0 }))
}

fn music_error(path: &str) -> impl Fn(MusicError) -> Error + '_ {
    move |source| Error::Music {
        path: path.to_owned(),
        source,
    }
}
