//! Error type shared by the whole crate.

use std::io;
use std::path::PathBuf;

/// Everything that can go wrong while finding or reading the game data.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Nothing at the given location looks like a Settlers II installation or CD.
    #[error(
        "no S2.EXE found under {0}; point at the Settlers II install folder, the GOG folder, \
         or the CD image (.cue, .bin or .iso)"
    )]
    GameNotFound(PathBuf),

    /// The requested file or directory is not present in any layer.
    #[error("{0} was not found in the Settlers II data")]
    NotFound(String),

    /// A file was requested but the path names a directory.
    #[error("{0} is a directory, not a file")]
    NotAFile(String),

    /// A directory listing was requested but the path names a file.
    #[error("{0} is a file, not a directory")]
    NotADirectory(String),

    /// The data path cannot be used, for example because it climbs out with `..`.
    #[error("invalid data path {path:?}: {reason}")]
    InvalidPath {
        /// The path as given by the caller.
        path: String,
        /// Why it was rejected.
        reason: &'static str,
    },

    /// The path does not have an extension this crate knows how to open.
    #[error("cannot open {0}: expected a folder, or a .cue, .ins, .bin, .gog or .iso file")]
    UnsupportedSource(PathBuf),

    /// A cue sheet (`.cue` or GOG `.ins`) could not be understood.
    #[error("cue sheet {path}, line {line}: {reason}")]
    Cue {
        /// The cue sheet on disk.
        path: PathBuf,
        /// One-based line number, or 0 when the problem is not tied to a line.
        line: usize,
        /// What was wrong with it.
        reason: String,
    },

    /// A CD image is truncated, damaged or in a format this crate does not read.
    #[error("{path} is not a usable CD image: {reason}")]
    Image {
        /// The image file on disk.
        path: PathBuf,
        /// What was wrong with it.
        reason: String,
    },

    /// `S2.EXE` was found but does not carry a recognisable Settlers II banner.
    #[error("S2.EXE is not a Settlers II executable this crate recognises")]
    UnknownEdition,

    /// The requested CD audio track does not exist in the cue sheet.
    #[error("there is no CD audio track {0}")]
    NoSuchTrack(u8),

    /// An operating system error while touching a file on disk.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
}

/// Shorthand for results carrying this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: path.into(),
            source,
        })
    }
}
