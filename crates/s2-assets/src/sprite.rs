//! Archive bitmaps as rows of runs, ready for a CPU blitter.
//!
//! Every drawable item becomes a [`Sprite`]: per row, the runs of pixels it draws and how to draw
//! each one (a palette index, a player-colour shade or a shadow). Transparent pixels cost nothing
//! at draw time, and clipping a run to a window is two comparisons.

use s2_data::DataSet;
use s2_formats::lst::{Archive, Item, Origin, Pixel, PlayerBitmap};

use crate::{Error, Result};

/// How a run's pixels are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ink {
    /// Each value is a palette index, written as it is.
    Colour,
    /// Each value is a player-colour shade, written as the player's base index plus the shade.
    Player,
    /// Each covered pixel is replaced through a remap table; the run has no values.
    Shadow,
}

/// A horizontal run of drawn pixels in one sprite row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Run {
    /// First column, from the sprite's left edge.
    pub x: u16,
    /// Pixels in the run.
    pub len: u16,
    /// How they are drawn.
    pub ink: Ink,
    start: u32,
}

/// One bitmap, shadow or player bitmap as rows of runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    origin: Origin,
    width: u16,
    height: u16,
    rows: Box<[u32]>,
    runs: Box<[Run]>,
    values: Box<[u8]>,
}

impl Sprite {
    /// The sprite of an archive item, or `None` for items that are not bitmaps.
    #[must_use]
    pub fn from_item(item: &Item) -> Option<Self> {
        match item {
            Item::Bitmap(b) => {
                let pixels = b
                    .image()
                    .pixels()
                    .iter()
                    .zip(b.mask().bits())
                    .map(|(&index, &drawn)| drawn.then_some((Ink::Colour, index)));
                Some(Self::build(b.origin(), b.width(), b.height(), pixels))
            }
            Item::PlayerBitmap(p) => Some(Self::from_player_bitmap(p)),
            Item::Shadow(s) => {
                let pixels = s.mask().bits().iter().map(|&set| set.then_some((Ink::Shadow, 0)));
                Some(Self::build(s.origin(), s.width(), s.height(), pixels))
            }
            Item::RawBitmap(r) => {
                let pixels = r.image().pixels().iter().map(|&index| Some((Ink::Colour, index)));
                Some(Self::build(r.origin(), r.width(), r.height(), pixels))
            }
            Item::Empty | Item::Sound(_) | Item::Font(_) | Item::Palette(_) => None,
        }
    }

    /// The sprite of a player-colour bitmap, from an archive or a BOB file.
    #[must_use]
    pub fn from_player_bitmap(bitmap: &PlayerBitmap) -> Self {
        let pixels = bitmap.pixels().iter().map(|pixel| match *pixel {
            Pixel::Transparent => None,
            Pixel::Colour(index) => Some((Ink::Colour, index)),
            Pixel::Player(shade) => Some((Ink::Player, shade.get())),
        });
        Self::build(bitmap.origin(), bitmap.width(), bitmap.height(), pixels)
    }

    /// Builds a sprite from its pixels, row by row: `None` for transparent, otherwise the ink and
    /// value. Shadow values are ignored.
    #[must_use]
    pub fn build(
        origin: Origin,
        width: u16,
        height: u16,
        pixels: impl IntoIterator<Item = Option<(Ink, u8)>>,
    ) -> Self {
        let mut rows = Vec::with_capacity(usize::from(height) + 1);
        let mut runs: Vec<Run> = Vec::new();
        let mut values = Vec::new();
        let mut pixels = pixels.into_iter();
        for _ in 0..height {
            rows.push(count(runs.len()));
            let mut open: Option<Run> = None;
            for x in 0..width {
                let pixel = pixels.next().flatten();
                match (pixel, open.as_mut()) {
                    (Some((ink, _)), Some(run)) if run.ink == ink => run.len += 1,
                    (pixel, _) => {
                        runs.extend(open.take());
                        open = pixel.map(|(ink, _)| Run {
                            x,
                            len: 1,
                            ink,
                            start: count(values.len()),
                        });
                    }
                }
                if let Some((ink, value)) = pixel
                    && ink != Ink::Shadow
                {
                    values.push(value);
                }
            }
            runs.extend(open);
        }
        rows.push(count(runs.len()));
        Self {
            origin,
            width,
            height,
            rows: rows.into(),
            runs: runs.into(),
            values: values.into(),
        }
    }

    /// The pixel that sits on the point the sprite is drawn at, from its top-left corner.
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The runs of row `y`, left to right; empty past the last row.
    #[must_use]
    pub fn row(&self, y: u16) -> &[Run] {
        let y = usize::from(y);
        match (self.rows.get(y), self.rows.get(y + 1)) {
            (Some(&from), Some(&to)) => self.runs.get(from as usize..to as usize).unwrap_or_default(),
            _ => &[],
        }
    }

    /// The values of `run`, one per pixel: palette indices or shades. Empty for shadows.
    #[must_use]
    pub fn values(&self, run: &Run) -> &[u8] {
        if run.ink == Ink::Shadow {
            return &[];
        }
        let start = run.start as usize;
        self.values
            .get(start..start + usize::from(run.len))
            .unwrap_or_default()
    }

    /// The sprite with each palette index `index` drawn as `table[index]`. Player shades and
    /// shadows stay as they are.
    #[must_use]
    pub fn recoloured(&self, table: &[u8; 256]) -> Self {
        let mut sprite = self.clone();
        for run in self.runs.iter().filter(|run| run.ink == Ink::Colour) {
            let start = run.start as usize;
            if let Some(values) = sprite.values.get_mut(start..start + usize::from(run.len)) {
                for value in values {
                    *value = table[usize::from(*value)];
                }
            }
        }
        sprite
    }

    /// The palette indices the sprite draws, one per pixel drawn as a colour.
    pub fn colours(&self) -> impl Iterator<Item = u8> + '_ {
        self.runs
            .iter()
            .filter(|run| run.ink == Ink::Colour)
            .flat_map(|run| self.values(run).iter().copied())
    }

    /// Pixels drawn, over all rows.
    #[must_use]
    pub fn drawn(&self) -> usize {
        self.runs.iter().map(|r| usize::from(r.len)).sum()
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The ship sheet: ships at rest and under sail, part-built ships, cargo and pennants. `S2.EXE`
/// loads it only for campaign games.
pub const SHIP_SPRITES: &str = "DATA/BOOT_Z.LST";

/// Every drawable item of an archive as a [`Sprite`], by slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpriteSheet {
    sprites: Vec<Option<Sprite>>,
}

impl SpriteSheet {
    /// The sprites of `archive`'s items, keeping their slot numbers.
    #[must_use]
    pub fn from_archive(archive: &Archive) -> Self {
        Self {
            sprites: archive.iter().map(Sprite::from_item).collect(),
        }
    }

    /// A sheet holding `slots` in order, for sprites made without an archive.
    #[must_use]
    pub fn from_slots(slots: impl IntoIterator<Item = Option<Sprite>>) -> Self {
        Self {
            sprites: slots.into_iter().collect(),
        }
    }

    /// Reads and decodes the archive at `path` in `data`.
    ///
    /// # Errors
    ///
    /// [`Error::Data`] if the file cannot be read, [`Error::Archive`] if it does not decode.
    pub fn load(data: &DataSet, path: &'static str) -> Result<Self> {
        let bytes = data.read(path)?;
        let archive = Archive::parse(&bytes).map_err(|source| Error::Archive { path, source })?;
        Ok(Self::from_archive(&archive))
    }

    /// The sprite in slot `slot`, or `None` for an empty or undrawable slot.
    #[must_use]
    pub fn get(&self, slot: usize) -> Option<&Sprite> {
        self.sprites.get(slot).and_then(Option::as_ref)
    }

    /// Slots in the archive, drawable or not.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    /// Whether the archive had no slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Every sprite with its slot, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &Sprite)> {
        self.sprites
            .iter()
            .enumerate()
            .filter_map(|(slot, sprite)| sprite.as_ref().map(|s| (slot, s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> Origin {
        Origin { x: 1, y: 2 }
    }

    #[test]
    fn rows_become_runs_split_by_ink() {
        let c = |v| Some((Ink::Colour, v));
        let p = |v| Some((Ink::Player, v));
        let s = Some((Ink::Shadow, 0));
        let sprite = Sprite::build(origin(), 4, 2, [c(5), c(6), p(2), None, None, s, s, c(0)]);
        let first = sprite.row(0);
        assert_eq!(first.len(), 2);
        assert_eq!((first[0].x, first[0].len, first[0].ink), (0, 2, Ink::Colour));
        assert_eq!(sprite.values(&first[0]), [5, 6]);
        assert_eq!((first[1].x, first[1].len, first[1].ink), (2, 1, Ink::Player));
        assert_eq!(sprite.values(&first[1]), [2]);
        let second = sprite.row(1);
        assert_eq!((second[0].x, second[0].len, second[0].ink), (1, 2, Ink::Shadow));
        assert_eq!(sprite.values(&second[0]), []);
        assert_eq!((second[1].x, second[1].ink), (3, Ink::Colour));
        assert_eq!(sprite.values(&second[1]), [0], "index 0 stays drawable");
        assert_eq!(sprite.row(2), []);
        assert_eq!(sprite.drawn(), 6);
        assert_eq!(sprite.origin(), origin());
    }

    #[test]
    fn short_pixel_lists_leave_the_rest_transparent() {
        let sprite = Sprite::build(origin(), 3, 2, [Some((Ink::Colour, 1))]);
        assert_eq!(sprite.drawn(), 1);
        assert_eq!(sprite.row(1), []);
    }
}
