//! Parsers for The Settlers II data file formats.
//!
//! Every parser takes bytes the caller has already read (usually through `s2-data`) and returns
//! plain Rust types. Parsers never panic on malformed input and never allocate more than the
//! input can justify, so they are safe to fuzz and to point at damaged files.
//!
//! No game data ships with this crate; players supply their own copy.

#![forbid(unsafe_code)]

pub mod bob;
pub mod bytes;
pub mod gouraud;
pub mod idx;
pub mod iff;
pub mod image;
pub mod lst;
pub mod map;
pub mod music;
pub mod palette;
pub mod rtx;
pub mod save;
pub mod smacker;
pub mod text;

pub use bytes::Truncated;
pub use image::IndexedImage;
pub use palette::Palette;
