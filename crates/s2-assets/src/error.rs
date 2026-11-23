/// Why assets could not be built.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read from the data set.
    #[error(transparent)]
    Data(#[from] s2_data::Error),
    /// A palette or texture sheet did not decode.
    #[error("{path}: {source}")]
    Iff {
        /// The file.
        path: &'static str,
        /// What went wrong.
        source: s2_formats::iff::Error,
    },
    /// A shading table did not decode.
    #[error("{path}: {source}")]
    Shading {
        /// The file.
        path: &'static str,
        /// What went wrong.
        source: s2_formats::gouraud::Error,
    },
    /// An object archive did not decode.
    #[error("{path}: {source}")]
    Archive {
        /// The file.
        path: &'static str,
        /// What went wrong.
        source: s2_formats::lst::Error,
    },
    /// A BOB file or the settler animation script did not decode.
    #[error("{path}: {source}")]
    Bob {
        /// The file.
        path: &'static str,
        /// What went wrong.
        source: s2_formats::bob::Error,
    },
    /// A texture sheet is not 256 by 256 pixels.
    #[error("{path} is {width}x{height}, expected 256x256")]
    SheetSize {
        /// The file.
        path: &'static str,
        /// Width found.
        width: u16,
        /// Height found.
        height: u16,
    },
}

/// Result type for asset building.
pub type Result<T> = std::result::Result<T, Error>;
