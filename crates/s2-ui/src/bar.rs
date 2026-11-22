//! The bar a settings screen shows a weight in (`0x51014`).
//!
//! The original builds one with `0x50f7c` and fills it with `0x50ff0`. The trough goes through
//! the call every widget's background goes through, in style 0 and in the other of the two
//! states, so it takes the style's second item and the shaded pair of edges. The share runs from
//! the left in a flat colour that steps with the weight.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Edges, Painter};
use crate::skin::{Look, Skin};
use crate::text::{FontId, Label, Pen};

/// The style the trough draws in.
const STYLE: u16 = 0;

/// How far inside the rectangle the share sits on every side.
const INSET: i32 = 3;

/// The font the count prints in: font index 2, `font09` (`0x51026` passes it to `0x50868`).
const COUNT_FONT: FontId = FontId::Small;

/// The pen colour the count prints in: the one a new drawing context starts with, which the bar
/// never changes. It fills the inside of each `font09` glyph, whose outline is a fixed colour.
const COUNT_COLOUR: u8 = 1;

/// The weight a full bar stands for.
pub const FULL: u32 = 100;

/// The share's colour below [`AMBER_FROM`] (`0x510fd`).
const RED: u8 = 137;

/// Its colour from there to [`GREEN_FROM`] (`0x51106`).
const AMBER: u8 = 5;

/// Its colour from there up (`0x5110a`).
const GREEN: u8 = 45;

/// The weight the share turns amber at.
const AMBER_FROM: u32 = 15;

/// The weight it turns green at.
const GREEN_FROM: u32 = 50;

/// The colour a share of `weight` draws in.
#[must_use]
pub const fn colour(weight: u32) -> u8 {
    if weight < AMBER_FROM {
        RED
    } else if weight < GREEN_FROM {
        AMBER
    } else {
        GREEN
    }
}

/// Draws a bar of `weight` in `rect`, printing the weight over the share when `counted`.
///
/// The original prints it only for a bar built with a zero second byte, which is the one a
/// building's own window carries; every settings screen passes 1 and shows the share alone. The
/// number sits across the middle of the bar, half the inside's height below its top
/// (`0x51121`, `0x50444`), centred as a drawing context centres a line (`0x509cc`).
pub fn draw(painter: &mut Painter, skin: &Skin, rect: Rect, weight: u32, counted: bool) {
    if let Some(face) = skin.face(STYLE, Look::On) {
        painter.frame_edges(face, skin.edge(STYLE), rect, Edges::Shaded);
    }
    let across = rect.width.saturating_sub(inset(2));
    let tall = rect.height.saturating_sub(inset(2));
    let share = across * weight.min(FULL) / FULL;
    let filled = Rect::new(rect.x + INSET, rect.y + INSET, share, tall);
    painter.fill(filled, colour(weight));
    if counted {
        let number = i32::try_from(weight).unwrap_or_default();
        let down = i32::try_from(tall / 2).unwrap_or_default();
        painter.text_aligned(
            Point::new(rect.x, rect.y + down),
            COUNT_FONT,
            COUNT_COLOUR,
            Label::PerCent(number),
            Pen::TIGHT,
            Align::Across(rect.width),
        );
    }
}

/// How much `steps` insets take off a side.
const fn inset(steps: u32) -> u32 {
    steps * (INSET as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_share_turns_colour_at_the_two_weights() {
        assert_eq!(colour(0), RED);
        assert_eq!(colour(AMBER_FROM - 1), RED);
        assert_eq!(colour(AMBER_FROM), AMBER);
        assert_eq!(colour(GREEN_FROM - 1), AMBER);
        assert_eq!(colour(GREEN_FROM), GREEN);
        assert_eq!(colour(FULL), GREEN);
    }

    #[test]
    fn the_share_fills_the_inside_at_a_full_weight() {
        let mut painter = Painter::new(Rect::new(0, 0, 200, 100));
        let skin = Skin::new();
        draw(&mut painter, &skin, Rect::new(10, 10, 80, 16), FULL, false);
        let filled = painter
            .ops()
            .iter()
            .find_map(|op| match op.kind {
                crate::paint::OpKind::Fill { rect, index } => Some((rect, index)),
                _ => None,
            })
            .expect("the share");
        assert_eq!(filled, (Rect::new(13, 13, 74, 10), GREEN));
    }
}
