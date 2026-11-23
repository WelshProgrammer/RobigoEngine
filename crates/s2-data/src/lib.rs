//! Finds and reads the player's original Settlers II data.
//!
//! The game files can come from an installed folder, the retail CD as a `.cue`/`.bin` pair or
//! an `.iso`, or the GOG Gold Edition. Sources stack as [`Layer`]s inside a [`DataSet`], so the
//! v1.51 patch folder can sit on top of the CD image. [`DataSet::discover`] builds the usual
//! stacks from a single path. [`DataSet::edition`] says which build is present and
//! [`DataSet::cd_audio`] exposes the Red Book music tracks.
//!
//! No game data ships with this crate; players supply their own copy.

mod audio;
mod cue;
mod dataset;
mod edition;
mod error;
mod iso9660;
mod layer;
mod path;
mod sector;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;

pub use audio::{AudioTrack, CdAudio, TrackReader};
pub use dataset::DataSet;
pub use edition::{Edition, Release};
pub use error::{Error, Result};
pub use layer::{DataReader, DirEntry, EntryKind, Layer, LayerKind, Metadata};
