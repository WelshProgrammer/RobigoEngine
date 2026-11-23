//! Drawing sprites into an indexed surface, clipped to its edges as the original's blitters
//! clip to the view.

use s2_assets::{Ink, Remap, Sprite};
use s2_sim::Player;

use crate::terrain::{Point, Surface};

/// The palette index of player 0's colour ramp; each player's four entries follow on.
const PLAYER_RAMPS: u8 = 128;

/// The first index of `player`'s four-entry colour ramp, 128 + 4·player. A shade `s` of a
/// player-colour bitmap draws as this plus `s`.
#[must_use]
pub fn colour_base(player: Player) -> u8 {
    PLAYER_RAMPS + 4 * u8::try_from(player.index()).unwrap_or_default()
}

/// The colour ramp of the player numbered as `nation` is, 128 + 4·nation. The resting catapult
/// takes its colours this way (`0x1a7c6`); its shipped picture has no player-colour pixels, so the
/// choice only shows in a replaced picture.
#[must_use]
pub fn nation_colour_base(nation: s2_assets::Nation) -> u8 {
    PLAYER_RAMPS + 4 * u8::try_from(nation.index()).unwrap_or_default()
}

/// How a sprite's pixels are written.
#[derive(Debug, Clone, Copy)]
pub struct Paint<'a> {
    /// Added to player-colour shades: the player's colour ramp, as [`colour_base`] gives it.
    pub base: u8,
    /// Replaces each pixel a shadow covers; shadow runs draw nothing without one.
    pub shadow: Option<&'a Remap>,
}

impl Paint<'_> {
    /// Plain bitmaps: palette indices only.
    pub const PLAIN: Self = Self {
        base: 0,
        shadow: None,
    };
}

/// Draws `sprite` with its origin at `at`, where `at` is in frame coordinates and `surface`
/// holds the frame's rows from `top` down. Pixels outside the surface are cut.
pub fn blit(surface: &mut Surface<'_>, top: i32, sprite: &Sprite, at: Point, paint: Paint<'_>) {
    blit_rows(surface, top, sprite, at, paint, 0..sprite.height());
}

/// Draws source rows `rows` of `sprite` where [`blit`] puts them, and nothing else
/// (`0x863a4`): a building site's picture grows up from its bottom row this way.
pub fn blit_rows(
    surface: &mut Surface<'_>,
    top: i32,
    sprite: &Sprite,
    at: Point,
    paint: Paint<'_>,
    rows: std::ops::Range<u16>,
) {
    let origin = sprite.origin();
    let left = at.x - i32::from(origin.x);
    let first = at.y - i32::from(origin.y) - top;
    let height = i32::try_from(surface.height()).unwrap_or(i32::MAX);
    let width = i32::try_from(surface.width()).unwrap_or(i32::MAX);
    let end = i32::from(rows.end.min(sprite.height()));
    let start = i32::from(rows.start);
    if start >= end
        || first + start >= height
        || first + end <= 0
        || left >= width
        || left + i32::from(sprite.width()) <= 0
    {
        return;
    }
    let skip = (-first).max(start);
    for sy in skip..end.min(height - first) {
        let Some((line, mut trace, pen)) = surface.line(first + sy) else {
            continue;
        };
        for run in sprite.row(u16::try_from(sy).unwrap_or_default()) {
            let x0 = left + i32::from(run.x);
            let a = x0.max(0);
            let b = (x0 + i32::from(run.len)).min(width);
            if a >= b {
                continue;
            }
            let index = |v: i32| usize::try_from(v).unwrap_or_default();
            let (a, b, skip) = (index(a), index(b), index(a - x0));
            let target = &mut line[a..b];
            match run.ink {
                Ink::Colour => target.copy_from_slice(&sprite.values(run)[skip..skip + (b - a)]),
                Ink::Player => {
                    for (pixel, &shade) in target.iter_mut().zip(&sprite.values(run)[skip..]) {
                        *pixel = paint.base.wrapping_add(shade);
                    }
                }
                Ink::Shadow => {
                    let Some(table) = paint.shadow else { continue };
                    for pixel in target.iter_mut() {
                        *pixel = table.get(*pixel);
                    }
                }
            }
            if let Some(trace) = trace.as_deref_mut() {
                for mark in &mut trace[a..b] {
                    *mark |= pen;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use s2_assets::Origin;

    use super::*;
    use crate::Layers;

    fn sprite() -> Sprite {
        let c = |v| Some((Ink::Colour, v));
        let p = |v| Some((Ink::Player, v));
        let s = Some((Ink::Shadow, 0));
        Sprite::build(Origin { x: 1, y: 1 }, 3, 2, [c(9), p(2), c(7), s, None, s])
    }

    fn draw(at: Point, top: i32, height: u32) -> (Vec<u8>, Vec<u16>) {
        let mut pixels: Vec<u8> = (0..4 * height).map(|i| u8::try_from(i).unwrap()).collect();
        let mut trace = vec![0; pixels.len()];
        let mut surface = Surface::of_parts(&mut pixels, Some(&mut trace), 4, height);
        surface.set_pen(Layers::OBJECTS);
        let table = Remap::nearest(&s2_assets::Palette::default(), s2_assets::Tint::SHADOW);
        let paint = Paint {
            base: PLAYER_RAMPS + 4,
            shadow: Some(&table),
        };
        blit(&mut surface, top, &sprite(), at, paint);
        (pixels, trace)
    }

    #[test]
    fn draws_at_the_origin_with_player_colours_and_shadows() {
        let (pixels, trace) = draw(Point::new(1, 1), 0, 3);
        assert_eq!(&pixels[..4], [9, PLAYER_RAMPS + 4 + 2, 7, 3]);
        assert_eq!(
            &pixels[4..8],
            [0, 5, 0, 7],
            "shadows remap: all-black palette maps to 0"
        );
        assert_eq!(&trace[..8], [16, 16, 16, 0, 16, 0, 16, 0]);
    }

    #[test]
    fn a_row_range_draws_only_those_rows_in_place() {
        let mut pixels: Vec<u8> = (0..12).collect();
        let mut surface = Surface::of_parts(&mut pixels, None, 4, 3);
        surface.set_pen(Layers::OBJECTS);
        blit_rows(&mut surface, 0, &sprite(), Point::new(1, 1), Paint::PLAIN, 1..2);
        assert_eq!(&pixels[..4], [0, 1, 2, 3], "row 0 is left out");
        assert_eq!(
            &pixels[4..8],
            [4, 5, 6, 7],
            "row 1 is shadow, which plain paint skips"
        );
        let mut pixels: Vec<u8> = (0..12).collect();
        let mut surface = Surface::of_parts(&mut pixels, None, 4, 3);
        surface.set_pen(Layers::OBJECTS);
        blit_rows(&mut surface, 0, &sprite(), Point::new(1, 1), Paint::PLAIN, 0..1);
        assert_eq!(&pixels[..8], [9, 2, 7, 3, 4, 5, 6, 7], "row 0 alone");
    }

    #[test]
    fn clips_at_every_edge() {
        let (pixels, _) = draw(Point::new(0, 0), 0, 2);
        assert_eq!(
            &pixels[..4],
            [0, 0, 2, 3],
            "only the second row shows, cut on the left"
        );
        let (pixels, _) = draw(Point::new(4, 1), 0, 2);
        assert_eq!(&pixels[..4], [0, 1, 2, 9], "only the first column shows");
        let (pixels, trace) = draw(Point::new(1, 1), 1, 2);
        assert_eq!(&pixels[..4], [0, 1, 0, 3], "the surface starts at frame row 1");
        assert_eq!(trace[4..].iter().filter(|&&m| m != 0).count(), 0);
    }
}
