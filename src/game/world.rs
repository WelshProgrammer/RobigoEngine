//! The World Campaign's world screen as the front end shows it: the backdrop with the world
//! picture laid over it, closed continents recoloured, and the mask the player picks with.

use s2_mods::{Backdrop, Sheets};
use s2_render::Frame;
use s2_ui::screens::world::{self, Continents, WorldScreen};

/// The world screen's picture and what picking on it needs.
pub(crate) struct WorldPicture {
    /// The backdrop with the world picture over it, whose palette the screen shows.
    pub(crate) backdrop: Backdrop,
    /// The same as a frame, ready to paste.
    pub(crate) frame: Frame,
    /// The world picture's size.
    pub(crate) size: (u32, u32),
    /// The mask, a byte per pixel of the world picture.
    mask: Vec<u8>,
}

impl WorldPicture {
    /// Whether the data `sheets` reads holds the World Campaign, which comes with the Gold
    /// Edition's mod.
    pub(crate) fn available(sheets: &Sheets<'_>) -> bool {
        sheets.has_backdrop(world::PICTURE) && sheets.has_backdrop(world::MASK)
    }

    /// Reads the pictures and lays them out with the continents `won` names, recolouring each
    /// continent `continents` does not open.
    pub(crate) fn load(sheets: &Sheets<'_>, continents: &Continents, won: &[bool]) -> Result<Self, String> {
        let mut backdrop = sheets
            .backdrop(world::BACKDROP)
            .map_err(|error| format!("{}: {error}", world::BACKDROP))?;
        let mut picture = sheets
            .backdrop(world::PICTURE)
            .map_err(|error| format!("{}: {error}", world::PICTURE))?;
        let mask = sheets
            .backdrop(world::MASK)
            .map_err(|error| format!("{}: {error}", world::MASK))?;
        let table = sheets
            .remap_table()
            .ok()
            .as_deref()
            .and_then(world::remap_table)
            .ok_or_else(|| format!("{}: not a colour table", s2_mods::REMAP))?;
        let closed = |number: u8| !continents.open(number, won);
        world::recolour(&mut picture.pixels, &mask.pixels, &table, &closed);
        let corner = WorldScreen::picture_corner(picture.width, s2_render::terrain::Point::new(0, 0));
        let screen_width = usize::try_from(backdrop.width).unwrap_or_default();
        let width = usize::try_from(picture.width).unwrap_or_default().max(1);
        for (row, line) in picture.pixels.chunks_exact(width).enumerate() {
            for (column, &index) in line.iter().enumerate() {
                let x = usize::try_from(corner.x).unwrap_or_default() + column;
                let y = usize::try_from(corner.y).unwrap_or_default() + row;
                if index != 0
                    && x < screen_width
                    && let Some(pixel) = backdrop.pixels.get_mut(y * screen_width + x)
                {
                    *pixel = index;
                }
            }
        }
        let frame = super::front::framed(&backdrop);
        Ok(Self {
            backdrop,
            frame,
            size: (picture.width, picture.height),
            mask: mask.pixels,
        })
    }

    /// The mask's pixel at (`x`, `y`) of the world picture, 0 off it.
    pub(crate) fn mask(&self, x: u32, y: u32) -> u8 {
        let at = usize::try_from(u64::from(y) * u64::from(self.size.0) + u64::from(x)).unwrap_or(usize::MAX);
        if x < self.size.0 {
            self.mask.get(at).copied().unwrap_or_default()
        } else {
            0
        }
    }
}
