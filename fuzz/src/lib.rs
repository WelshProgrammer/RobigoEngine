//! Shared helpers for the `s2-formats` fuzz targets.
//!
//! Each target hands its input to one format family's parsers. When a parse succeeds, it walks
//! everything the result exposes (accessors, render paths, error and quirk formatting), so a
//! panic or runaway allocation downstream of the parser shows up too.

use std::fmt::{Debug, Display};
use std::hint::black_box;

use s2_formats::Palette;
use s2_formats::lst::{Item, PlayerBitmap, PlayerColour};
use s2_formats::map::{LayerKind, Map};

/// Splits an input made of several files into length-prefixed parts.
///
/// Each part is a little-endian `u32` byte count followed by that many bytes, clamped to what
/// is left; iteration stops when fewer than four bytes remain. The seed tool writes the same
/// framing for the two-file and seven-file formats.
#[derive(Debug, Clone)]
pub struct Parts<'a>(&'a [u8]);

impl<'a> Parts<'a> {
    /// Starts splitting `data` from its first byte.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self(data)
    }
}

impl<'a> Iterator for Parts<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let (len, rest) = self.0.split_first_chunk::<4>()?;
        let len = usize::try_from(u32::from_le_bytes(*len)).map_or(rest.len(), |len| len.min(rest.len()));
        let (part, rest) = rest.split_at(len);
        self.0 = rest;
        Some(part)
    }
}

/// Formats a failed parse's error message, or hands a successful parse's value to `walk`.
pub fn check<T, E: Display>(result: Result<T, E>, walk: impl FnOnce(T)) {
    match result {
        Ok(value) => walk(value),
        Err(error) => {
            black_box(error.to_string());
        }
    }
}

/// Formats quirks or any other value with `Debug`, which every public type implements.
pub fn debug(value: &impl Debug) {
    black_box(format!("{value:?}"));
}

/// Walks a decoded map: header text, every layer at its far corner, areas and animals.
pub fn map(map: &Map) {
    debug(&map.quirks());
    if let Some(header) = map.header() {
        debug(&(
            header.title(),
            header.author(),
            header.terrain(),
            header.headquarters(),
        ));
    }
    let (x, y) = (map.width().saturating_sub(1), map.height().saturating_sub(1));
    for kind in LayerKind::ALL {
        debug(&map.get(kind, x, y));
    }
    debug(&(map.areas(), map.animals(), map.trailer().len()));
}

/// Renders a player-coloured bitmap both ways the crate offers.
pub fn player(bitmap: &PlayerBitmap) {
    debug(&bitmap.quirks());
    black_box(bitmap.to_indexed(PlayerColour::DEFAULT));
    black_box(bitmap.to_rgba(&Palette::default(), PlayerColour::DEFAULT));
}

/// Renders or measures one LST item, whatever its type.
pub fn item(item: &Item) {
    debug(&item.quirks());
    match item {
        Item::Empty => {}
        Item::Sound(sound) => {
            black_box(sound.bytes().len());
        }
        Item::Bitmap(bitmap) => {
            black_box(bitmap.to_rgba(&Palette::default()));
        }
        Item::Font(font) => font.glyphs().for_each(|(_, glyph)| player(glyph)),
        Item::PlayerBitmap(bitmap) => player(bitmap),
        Item::Palette(palette) => {
            black_box(palette.colour(u8::MAX));
        }
        Item::Shadow(shadow) => {
            black_box(shadow.mask().count());
        }
        Item::RawBitmap(bitmap) => {
            black_box(bitmap.to_rgba(&Palette::default()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Parts;

    #[test]
    fn parts_clamp_to_the_input() {
        let data = [2, 0, 0, 0, 7, 8, 9, 255, 255, 255, 255, 1, 2, 0, 0];
        let parts: Vec<_> = Parts::new(&data).collect();
        assert_eq!(parts, [&[7, 8][..], &[255, 1, 2, 0, 0][..]]);
    }
}
