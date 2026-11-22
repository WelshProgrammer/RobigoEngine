//! Saved games: one XZ-compressed tar archive holding the game's state and the replay that led
//! to it.
//!
//! | Entry | Contents |
//! |---|---|
//! | `format` | `robigo-save` and [`FORMAT`], one line |
//! | `replay` | the [`Replay`] as text, ending at the saved step with a check of its state hash |
//! | `state` | the [`Game`] as [postcard](https://docs.rs/postcard) bytes |
//! | `map.wld` | the map file's bytes, when the replay's map is a file outside the game's data |
//! | `building-types` | the building types the game was played by, as postcard bytes |
//! | `desktop` | what the interface keeps beside the game, as the interface writes it, when it has any |
//!
//! Loading reads `state` when the archive's format is this build's and its hash matches the
//! replay's last check. Otherwise it plays the replay from the start of the map, under this
//! build's rules, to the saved step. The tables a game is configured with (the animation script,
//! building types and waypoints) are not part of the state: they come from the game the caller
//! starts from the replay. The save holds the building types beside the state, so that a state
//! loaded under other tables can be repaired ([`Game::repair`]).

use std::io::{Read, Write};

use lzma_rust2::{XzOptions, XzReader, XzWriter};

use crate::game::Game;
use crate::replay::{MapSource, ParseError, Playback, Replay};
use crate::world::{BuildingType, BuildingTypes};

/// The version of the `state` entry's layout. It changes whenever a type in the state changes
/// shape, so that an older save is replayed instead of misread.
pub const FORMAT: u32 = 1;

/// The first word of the `format` entry.
const MAGIC: &str = "robigo-save";

/// The XZ preset saves are compressed with.
const PRESET: u32 = 6;

/// Entry names inside the archive.
const FORMAT_ENTRY: &str = "format";
const REPLAY_ENTRY: &str = "replay";
const STATE_ENTRY: &str = "state";
const MAP_ENTRY: &str = "map.wld";
const DESKTOP_ENTRY: &str = "desktop";
const TYPES_ENTRY: &str = "building-types";

/// A saved game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Save {
    /// The state layout the save was written with.
    pub format: u32,
    /// How the game started and every command up to the saved step.
    pub replay: Replay,
    /// The game's state, as postcard bytes.
    pub state: Vec<u8>,
    /// The map file, for a replay whose map is a file.
    pub map: Option<Vec<u8>>,
    /// The rows of the building types the game was played by, as postcard bytes, or `None` in a
    /// save without them.
    pub building_types: Option<Vec<u8>>,
    /// What the interface keeps beside the game, which the save carries without reading. It
    /// survives a save of another format, which is replayed.
    pub desktop: Option<Vec<u8>>,
}

/// How [`Save::restore`] rebuilt the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restored {
    /// From the saved state.
    State,
    /// By playing the replay, because the save is from another format or its state did not
    /// read back to the hash it was saved with.
    Replayed,
}

/// Why a save could not be written or read.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// Reading or writing the archive failed.
    #[error("archive: {0}")]
    Io(#[from] std::io::Error),
    /// The game's state could not be serialised.
    #[error("state: {0}")]
    State(#[from] postcard::Error),
    /// The archive is not a save.
    #[error("not a Robigo save")]
    NotASave,
    /// The archive lacks an entry.
    #[error("the save has no {0} entry")]
    Missing(&'static str),
    /// The replay entry could not be read.
    #[error("replay: {0}")]
    Replay(#[from] ParseError),
}

impl Save {
    /// Saves `game`, started and played as `replay` says up to its current step. `map` is the
    /// map file's bytes, kept when the replay's map is a file. The replay's end becomes the
    /// game's step and its last check the game's hash.
    ///
    /// # Errors
    ///
    /// [`SaveError::State`] when the state cannot be serialised.
    pub fn new(game: &Game, replay: &Replay, map: Option<&[u8]>) -> Result<Self, SaveError> {
        let step = game.clock().step();
        let mut replay = replay.clone();
        replay.end = step;
        replay.commands.retain(|stamped| stamped.step <= step);
        replay.checks.retain(|&(at, _)| at < step);
        replay.checks.push((step, game.state_hash()));
        let map = match replay.map {
            MapSource::File(_) => map.map(<[u8]>::to_vec),
            MapSource::Data(_) => None,
        };
        Ok(Self {
            format: FORMAT,
            replay,
            state: postcard::to_allocvec(game)?,
            map,
            building_types: Some(postcard::to_allocvec(game.world().building_types().types())?),
            desktop: None,
        })
    }

    /// The save as a compressed archive.
    ///
    /// # Errors
    ///
    /// [`SaveError::Io`] when compression fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveError> {
        let mut archive = tar::Builder::new(Vec::new());
        let format = format!("{MAGIC} {}\n", self.format);
        let replay = self.replay.to_text();
        let mut entries = vec![
            (FORMAT_ENTRY, format.as_bytes()),
            (REPLAY_ENTRY, replay.as_bytes()),
            (STATE_ENTRY, self.state.as_slice()),
        ];
        if let Some(map) = &self.map {
            entries.push((MAP_ENTRY, map));
        }
        if let Some(types) = &self.building_types {
            entries.push((TYPES_ENTRY, types));
        }
        if let Some(desktop) = &self.desktop {
            entries.push((DESKTOP_ENTRY, desktop));
        }
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            archive.append_data(&mut header, name, bytes)?;
        }
        let tar = archive.into_inner()?;
        let mut xz = XzWriter::new(Vec::new(), XzOptions::with_preset(PRESET))?;
        xz.write_all(&tar)?;
        Ok(xz.finish()?)
    }

    /// Reads a save written by [`Self::to_bytes`], of any format.
    ///
    /// # Errors
    ///
    /// [`SaveError`] when the bytes are not a save or an entry is missing or unreadable.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let mut tar = Vec::new();
        XzReader::new(bytes, false).read_to_end(&mut tar)?;
        let mut archive = tar::Archive::new(tar.as_slice());
        let (mut format, mut replay, mut state, mut map, mut desktop) = (None, None, None, None, None);
        let mut building_types = None;
        for entry in archive.entries()? {
            let mut entry = entry?;
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
            match entry.path_bytes().as_ref() {
                b"format" => format = Some(contents),
                b"replay" => replay = Some(contents),
                b"state" => state = Some(contents),
                b"map.wld" => map = Some(contents),
                b"building-types" => building_types = Some(contents),
                b"desktop" => desktop = Some(contents),
                _ => {}
            }
        }
        let format = format.ok_or(SaveError::Missing(FORMAT_ENTRY))?;
        let format = std::str::from_utf8(&format)
            .ok()
            .and_then(|line| line.trim().strip_prefix(MAGIC))
            .and_then(|version| version.trim().parse().ok())
            .ok_or(SaveError::NotASave)?;
        let replay = replay.ok_or(SaveError::Missing(REPLAY_ENTRY))?;
        let replay = Replay::parse(&String::from_utf8_lossy(&replay))?;
        Ok(Self {
            format,
            replay,
            state: state.ok_or(SaveError::Missing(STATE_ENTRY))?,
            map,
            building_types,
            desktop,
        })
    }

    /// The saved game, from `start`: a game freshly started as the replay says, with its
    /// tables set. The saved state is used when it is this build's format and reads back to
    /// the hash it was saved with; the game keeps `start`'s tables. Otherwise the replay's
    /// commands are played on `start` to the saved step.
    #[must_use]
    pub fn restore(&self, mut start: Game) -> (Game, Restored) {
        if let Some(game) = self.state(&start) {
            return (game, Restored::State);
        }
        let mut playback = Playback::new(&self.replay);
        while start.clock().step() < self.replay.end {
            playback.catch_up(&mut start);
            start.step();
        }
        playback.catch_up(&mut start);
        (start, Restored::Replayed)
    }

    /// The saved state with `start`'s tables, when it can be trusted: the save is of this
    /// build's format and its state reads back to the hash it was saved with. Unlike
    /// [`Save::restore`], never replays.
    #[must_use]
    pub fn state(&self, start: &Game) -> Option<Game> {
        if self.format != FORMAT {
            return None;
        }
        let mut game: Game = postcard::from_bytes(&self.state).ok()?;
        game.adopt_tables(start);
        let &(step, hash) = self.replay.checks.last()?;
        (step == game.clock().step() && hash == game.state_hash()).then_some(game)
    }

    /// The building types the game was played by, when the save is of this build's format and
    /// holds them.
    #[must_use]
    pub fn building_types(&self) -> Option<BuildingTypes> {
        if self.format != FORMAT {
            return None;
        }
        let rows: Vec<BuildingType> = postcard::from_bytes(self.building_types.as_deref()?).ok()?;
        BuildingTypes::new(rows).ok()
    }
}

#[cfg(test)]
mod tests;
