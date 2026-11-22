//! Type 3 fonts, as stored in `FONT14.FNT` and `RESOURCE.DAT`.

use super::bitmap::Budget;
use super::{Bitmap, Error, ItemType, PlayerBitmap, Quirk};
use crate::bytes::Reader;

/// The character code of the first glyph slot.
pub const FIRST_GLYPH: u8 = 32;

/// Glyph slots in every font, covering codes 32 to 255.
pub const GLYPH_COUNT: usize = 224;

/// A bitmap font: one optional player-colour glyph per character code from 32 to 255, so
/// text can be recoloured through [`PlayerColour`](super::PlayerColour). Glyphs stored as
/// plain type 2 bitmaps become player bitmaps with fixed colours only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    dx: u8,
    dy: u8,
    glyphs: Vec<Option<PlayerBitmap>>,
    quirks: Vec<Quirk>,
}

impl Font {
    /// Decodes a stand-alone font file such as `FONT14.FNT`: a `u16` type id of 3 followed by
    /// the font body. Bytes after the font are ignored and reported as a quirk.
    ///
    /// # Errors
    ///
    /// [`Error::NotAFont`] when the type id is not 3, or any error from decoding the glyphs.
    pub fn parse_file(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let id = r.u16()?;
        if id != ItemType::Font.id() {
            return Err(Error::NotAFont(id));
        }
        let mut font = Self::read(&mut r, &mut Budget::for_input(data.len()))?;
        if !r.is_empty() {
            font.quirks.push(Quirk::TrailingBytes {
                offset: r.position(),
                len: r.remaining(),
            });
        }
        Ok(font)
    }

    /// Reads a type 3 body: `u8` dx, `u8` dy, then [`GLYPH_COUNT`] slots, each a `u16` type id
    /// (0 for no glyph, 2 for a bitmap, 4 for a player bitmap) and the bitmap body.
    pub(super) fn read(r: &mut Reader<'_>, budget: &mut Budget) -> Result<Self, Error> {
        let (dx, dy) = (r.u8()?, r.u8()?);
        let mut quirks = Vec::new();
        let mut glyphs = Vec::with_capacity(GLYPH_COUNT);
        for code in FIRST_GLYPH..=u8::MAX {
            let glyph = match r.u16()? {
                0 => None,
                2 => Some(PlayerBitmap::from(Bitmap::read(r, budget)?)),
                4 => Some(PlayerBitmap::read(r, budget)?),
                id => return Err(Error::GlyphType(id)),
            };
            if let Some(glyph) = &glyph {
                quirks.extend(glyph.quirks().iter().map(|quirk| Quirk::Glyph {
                    code,
                    quirk: Box::new(quirk.clone()),
                }));
            }
            glyphs.push(glyph);
        }
        Ok(Self {
            dx,
            dy,
            glyphs,
            quirks,
        })
    }

    /// The first header byte: 8, 10 and 12 in the shipped fonts, against a [`Font::dy`] of 9, 11
    /// and 14. Its use is not yet confirmed against S2.EXE.
    #[must_use]
    pub fn dx(&self) -> u8 {
        self.dx
    }

    /// The second header byte: the height of every glyph in the shipped fonts (14, 11 and 9),
    /// so probably the line height.
    #[must_use]
    pub fn dy(&self) -> u8 {
        self.dy
    }

    /// The glyph for character `code`, if the font has one.
    #[must_use]
    pub fn glyph(&self, code: u8) -> Option<&PlayerBitmap> {
        let slot = code.checked_sub(FIRST_GLYPH)?;
        self.glyphs.get(usize::from(slot))?.as_ref()
    }

    /// What was unusual about the glyphs or the file; empty for every shipped font.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// Every glyph present, with its character code.
    pub fn glyphs(&self) -> impl Iterator<Item = (u8, &PlayerBitmap)> {
        (FIRST_GLYPH..=u8::MAX)
            .zip(&self.glyphs)
            .filter_map(|(code, glyph)| Some((code, glyph.as_ref()?)))
    }
}
