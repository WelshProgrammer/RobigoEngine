//! The one error type the binary's modules share.

use std::path::PathBuf;

use s2_sim::replay;
use s2_sim::save::SaveError;

/// Why Robigo stopped.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The data set could not be opened or read.
    #[error(transparent)]
    Data(#[from] s2_data::Error),
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// A file the game reads as a string table holds a page.
    #[error("{0} is not a string table")]
    NotATable(&'static str),
    /// The map did not decode.
    #[error("{0}: {1}")]
    Map(String, s2_formats::map::Error),
    /// The map lacks the terrain layers.
    #[error("{0}: {1}")]
    Terrain(String, s2_sim::MapError),
    /// The path names no map format the viewer knows.
    #[error("{0}: {1}")]
    Mission(String, s2_formats::rtx::Error),

    #[error("{0} is neither a WORLD_V1.0 map nor a WORLDnnn.DAT world")]
    UnknownMap(String),
    /// The map header names a landscape the game does not ship.
    #[error("the map's terrain byte {0} names no landscape; pick one with --landscape")]
    UnknownLandscape(u8),
    /// The landscape's graphics could not be built.
    #[error(transparent)]
    Assets(#[from] s2_assets::Error),
    /// The map or landscape comes with the Gold Edition, and the data or the settings leave the
    /// Gold Edition's mod out.
    #[error("{0} requires the '{name}' mod enabled (it comes with the Gold Edition)", name = s2_mods::GOLD_NAME)]
    NeedsGold(String),
    /// A save or replay was made with other mods than the ones loaded.
    #[error("{0}")]
    ModsDiffer(String),
    /// A file from the mods could not be read or decoded.
    #[error(transparent)]
    Mods(#[from] s2_mods::Error),
    /// The game was started with no data and has no base mod baked from any.
    #[error("no game data: copy Settlers 2 into the RETAIL folder")]
    NoData,
    /// The game has no mods to play from, for the reason given.
    #[error("no baked game data: {0}")]
    Unbaked(String),
    /// The GPU failed.
    #[error(transparent)]
    Render(#[from] s2_render::Error),
    /// SDL failed.
    #[error(transparent)]
    Platform(#[from] s2_platform::Error),
    /// A replay did not parse.
    #[error("{path}: {source}")]
    Log {
        /// The file.
        path: PathBuf,
        /// What was wrong.
        source: replay::ParseError,
    },
    /// A save could not be written or read.
    #[error("{path}: {source}")]
    Save {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: SaveError,
    },
    /// A save made with other sim tables is from another state format, so only replaying it
    /// would restore it, and that replays it by other rules.
    #[error(
        "{0}: the mods' rules have changed since this game was saved, and it is from another version of Robigo; it can't be loaded"
    )]
    SaveRulesChanged(PathBuf),
    /// A save holds a building whose type a mod has since given another size, or another
    /// strength when the building holds land.
    #[error(
        "{path}: a mod has changed the {what} of the {building} since this game was saved; it can't be loaded"
    )]
    SaveBeyondRepair {
        /// The save.
        path: PathBuf,
        /// What of the type changed.
        what: &'static str,
        /// The building type's name in the tables.
        building: &'static str,
    },
    /// A save's map is a file the save does not hold.
    #[error("{0}: the save does not hold its map file")]
    SaveWithoutMap(PathBuf),
    /// A replay was made with a different map, animation script or sim tables.
    #[error("{path}: the replay was made with a different {what}")]
    ReplayMismatch {
        /// The replay.
        path: PathBuf,
        /// `map`, `animation script` or `set of sim tables`.
        what: &'static str,
    },
    /// The screenshot could not be written.
    #[error("writing {path}: {source}")]
    Png {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
}

/// Result type for Robigo.
pub type Result<T> = std::result::Result<T, Error>;
