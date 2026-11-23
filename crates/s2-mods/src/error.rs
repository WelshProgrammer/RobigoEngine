use std::path::PathBuf;

/// What went wrong baking or loading mods.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The player's data could not be read.
    #[error(transparent)]
    Data(#[from] s2_data::Error),
    /// An original archive did not decode.
    #[error("{path}: {source}")]
    Archive {
        /// The data path.
        path: String,
        /// Why.
        source: s2_formats::lst::Error,
    },
    /// An original picture did not decode.
    #[error("{path}: {source}")]
    Picture {
        /// The data path.
        path: String,
        /// Why.
        source: s2_formats::iff::Error,
    },
    /// A CD audio track did not encode.
    #[error("CD track {track}: {reason}")]
    Rip {
        /// The track's number on the disc.
        track: u8,
        /// What the encoder said.
        reason: String,
    },
    /// A sound font could not be fetched or did not match its digest.
    #[error("sound font {name}: {reason}")]
    SoundFont {
        /// The sound font's name.
        name: String,
        /// What went wrong.
        reason: String,
    },
    /// A song did not encode once rendered.
    #[error("song {song}: {reason}")]
    Song {
        /// The song's number.
        song: u8,
        /// What the encoder said.
        reason: String,
    },
    /// A song or an FM bank did not decode.
    #[error("{path}: {source}")]
    Music {
        /// The data path or the mod file.
        path: String,
        /// Why.
        source: s2_formats::music::Error,
    },
    /// The data is a release the bake does not take: only v1.51 and the Gold Edition bake.
    #[error("this data is {0}; patch it to v1.51 before baking")]
    Unpatched(String),
    /// No loaded mod holds the file, which the original data may still have: a Gold Edition file
    /// with the `gold` mod left out, for one.
    #[error("no loaded mod holds {0}")]
    NotInMods(String),
    /// An original palette, terrain sheet or shading table did not decode.
    #[error(transparent)]
    Assets(#[from] s2_assets::Error),
    /// An original BOB file did not decode.
    #[error("{path}: {source}")]
    Bob {
        /// The data path.
        path: String,
        /// Why.
        source: s2_formats::bob::Error,
    },
    /// An original named pair did not decode.
    #[error("{path}: {source}")]
    Index {
        /// The pair's data path without the extension.
        path: String,
        /// Why.
        source: s2_formats::idx::Error,
    },
    /// An original named pair holds no item of the name asked for.
    #[error("{path} has no {name}")]
    NoItem {
        /// The pair's data path without the extension.
        path: String,
        /// The name.
        name: String,
    },
    /// A file in a mod could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// A TOML file in a mod did not parse.
    #[error("{}: {source}", path.display())]
    Toml {
        /// The file.
        path: PathBuf,
        /// Why.
        source: Box<toml::de::Error>,
    },
    /// A table could not be written as TOML.
    #[error("{path}: {source}")]
    Write {
        /// The table.
        path: String,
        /// Why.
        source: toml::ser::Error,
    },
    /// A row a mod gave a table is not one the table takes, or with no row, the table as a whole.
    #[error("{table}{}: {reason}", if row.is_empty() { String::new() } else { format!(", row {row}") })]
    Row {
        /// The table's path in the mod.
        table: String,
        /// The row's key.
        row: String,
        /// Why.
        reason: String,
    },
    /// A sample is not one the game plays.
    #[error("{}: {reason}", path.display())]
    Wav {
        /// The file.
        path: PathBuf,
        /// Why.
        reason: &'static str,
    },
    /// A sample's audio did not decode.
    #[error("{}: {reason}", path.display())]
    Audio {
        /// The file.
        path: PathBuf,
        /// What the decoder said.
        reason: String,
    },
    /// A mod the stack was asked for has no folder or no manifest.
    #[error("mod {0} is not installed")]
    Missing(String),
    /// A mod comes before a mod it depends on, or depends on one not loaded.
    #[error("mod {id} needs {needs} loaded before it")]
    Depends {
        /// The mod.
        id: String,
        /// What it needs.
        needs: String,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| Self::Io { path, source }
    }
}

/// A result whose error is [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
