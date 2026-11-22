//! Type 1 sound items.

use super::Error;
use crate::bytes::{Reader, to_usize};

/// A type 1 item: a `u32` length and that many bytes, holding either a sound effect or an
/// XMIDI song.
#[derive(Clone, PartialEq, Eq)]
pub enum Sound {
    /// Headerless unsigned 8-bit mono PCM. The sample rate is not stored in the item.
    Pcm(Vec<u8>),
    /// A whole XMIDI file (`FORM`…`XDIR`), left for the music parser.
    Xmidi(Vec<u8>),
}

impl Sound {
    pub(super) fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let len = to_usize(r.u32()?);
        let bytes = r.bytes(len)?;
        let is_xmidi = bytes.get(..4) == Some(b"FORM") && bytes.get(8..12) == Some(b"XDIR");
        Ok(if is_xmidi {
            Self::Xmidi(bytes.to_vec())
        } else {
            Self::Pcm(bytes.to_vec())
        })
    }

    /// The stored bytes: samples for [`Sound::Pcm`], the file for [`Sound::Xmidi`].
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Pcm(bytes) | Self::Xmidi(bytes) => bytes,
        }
    }
}

impl std::fmt::Debug for Sound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, len) = match self {
            Self::Pcm(bytes) => ("Pcm", bytes.len()),
            Self::Xmidi(bytes) => ("Xmidi", bytes.len()),
        };
        f.debug_struct(kind).field("len", &len).finish()
    }
}
