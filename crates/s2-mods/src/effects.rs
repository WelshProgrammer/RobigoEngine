//! The sound effects: `SOUND.LST`'s samples as WAV files, and the effect table as TOML.

use std::sync::Arc;

use s2_audio::{Effect, Effects, LAST_EFFECT, Playback, Sample};
use s2_data::DataSet;
use s2_formats::lst::{Archive, Item, Sound};

use crate::audio::{ENDINGS, decode};
use crate::{BASE, Error, ModStack, Result, read_wav, write_wav};

/// Where the effects' samples go in a mod, one WAV an effect id.
pub const EFFECT_SAMPLES: &str = "sounds/effects";

/// Where the effect table goes in a mod.
pub const EFFECTS_TABLE: &str = "tables/effects.toml";

/// The original's archive of samples.
pub(crate) const SOUND_ARCHIVE: &str = "DATA/SOUNDDAT/SOUND.LST";

/// The rate a WAV header carries for an effect the table leaves silent. The game plays every
/// sample at its row's rate; the header only lets a sound editor play it back sensibly.
const UNSET_RATE: u32 = 11_025;

/// The path of effect `id`'s sample in a mod, as the base mod writes it.
#[must_use]
pub fn effect_path(id: u8) -> String {
    format!("{EFFECT_SAMPLES}/{id}.wav")
}

/// The paths effect `id`'s sample can take in a mod, one a file ending in [`ENDINGS`].
fn effect_paths(id: u8) -> Vec<String> {
    ENDINGS
        .iter()
        .map(|ending| format!("{EFFECT_SAMPLES}/{id}.{ending}"))
        .collect()
}

/// The effects as the game plays them: the table, and each sample by the archive item the
/// table's ids map to ([`Effects::item`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sounds {
    /// The effect table.
    pub table: Effects,
    /// The samples, by item, with `None` where no mod supplies one.
    pub samples: Vec<Option<Sample>>,
}

/// The effects the stack supplies.
///
/// # Errors
///
/// [`Error::Row`] for a table row the effect table does not take, [`Error::Wav`] for a sample
/// its row plays the original way that is not unsigned 8-bit mono PCM, [`Error::Audio`] for one
/// that does not decode, and the stack's own errors.
pub fn effects(stack: &ModStack) -> Result<Sounds> {
    let merged = stack.table(EFFECTS_TABLE)?;
    let row_error = |row: &str, reason: String| Error::Row {
        table: EFFECTS_TABLE.into(),
        row: row.into(),
        reason,
    };
    let mut rows = Vec::with_capacity(merged.len());
    for (key, value) in merged {
        let id: u8 = key
            .parse()
            .map_err(|_| row_error(&key, "the key is not an effect id".into()))?;
        let row: Effect = value
            .try_into()
            .map_err(|error: toml::de::Error| row_error(&key, error.message().into()))?;
        rows.push((id, row));
    }
    let table = Effects::from_rows(rows).map_err(|error| row_error("", error.to_string()))?;
    let mut samples = vec![None; usize::from(Effects::item(LAST_EFFECT)) + 1];
    for id in 1..=LAST_EFFECT {
        let Some((owner, file)) = stack.file_of(&effect_paths(id)) else {
            continue;
        };
        let bytes = std::fs::read(&file).map_err(Error::io(&file))?;
        let playback = table.get(id).playback.unwrap_or(if owner == BASE {
            Playback::Original
        } else {
            Playback::Smooth
        });
        let sample = match playback {
            Playback::Original => {
                let pcm = read_wav(&bytes).map_err(|reason| Error::Wav { path: file, reason })?;
                Sample::Original(Arc::from(pcm))
            }
            Playback::Smooth => {
                let decoded = decode(&file, bytes)?;
                Sample::Smooth {
                    rate: decoded.rate,
                    frames: decoded.frames,
                }
            }
        };
        samples[usize::from(Effects::item(id))] = Some(sample);
    }
    Ok(Sounds { table, samples })
}

/// The effect table as the base mod writes it, a row an id.
pub(crate) fn table_file(table: &Effects) -> Result<Vec<u8>> {
    let rows: toml::Table = table
        .rows()
        .map(|(id, row)| {
            let value = toml::Value::try_from(row).map_err(|source| Error::Write {
                path: EFFECTS_TABLE.into(),
                source,
            })?;
            Ok((id.to_string(), value))
        })
        .collect::<Result<_>>()?;
    toml::to_string(&rows)
        .map(String::into_bytes)
        .map_err(|source| Error::Write {
            path: EFFECTS_TABLE.into(),
            source,
        })
}

/// Every sample `SOUND.LST` holds for an effect id, as the base mod's WAV files.
pub(crate) fn sample_files(data: &DataSet, table: &Effects) -> Result<Vec<(String, Vec<u8>)>> {
    let bytes = data.read(SOUND_ARCHIVE)?;
    let archive = Archive::parse(&bytes).map_err(|source| Error::Archive {
        path: SOUND_ARCHIVE.into(),
        source,
    })?;
    let items = archive.into_items();
    Ok((1..=LAST_EFFECT)
        .filter_map(|id| match items.get(usize::from(Effects::item(id))) {
            Some(Item::Sound(Sound::Pcm(pcm))) => {
                let rate = match u32::from(table.get(id).rate) {
                    0 => UNSET_RATE,
                    rate => rate,
                };
                Some((effect_path(id), write_wav(rate, pcm)))
            }
            _ => None,
        })
        .collect())
}
