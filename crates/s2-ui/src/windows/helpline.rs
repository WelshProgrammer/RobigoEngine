//! The line of help the pointer's own key asks for.
//!
//! The `i` key opens it over whatever the pointer is on. The window is one label the caller fills
//! with the line for that thing, so the text arrives as a run rather than a slot.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::store::Ui;
use crate::text::Pen;
use crate::widget::WindowId;
use crate::windows::kind;

/// The style the label takes (`0x583ae`).
pub const STYLE: u16 = 5;

/// The flags the label takes, bits 2, 3 and 4 (`0x583a2`).
pub const FLAGS: u16 = (1 << 2) | (1 << 3) | (1 << 4);

/// Where the label sits: its left and top, how much narrower it is than the window, and how tall.
const LABEL: (i32, i32, u32, u32) = (16, 21, 32, 17);

/// The line of help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpLineWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl HelpLineWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 280;
    /// How tall it is.
    pub const HEIGHT: u32 = 50;

    /// The window at its own kind.
    #[must_use]
    pub const fn new() -> Self {
        Self { id: kind::HELP_LINE }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the label sits inside a window of `area`.
    #[must_use]
    pub fn label(area: Rect) -> Rect {
        let (x, y, narrower, height) = LABEL;
        Rect::new(
            area.x + x,
            area.y + y,
            area.width.saturating_sub(narrower),
            height,
        )
    }

    /// Draws the window with `line` in it, which is already in cp437.
    pub fn run(&mut self, ui: &mut Ui, painter: &mut Painter, line: &[u8]) {
        let Some(window) = ui.window(self.id).copied() else {
            return;
        };
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return;
        }
        write(painter, Self::label(area), STYLE, line);
        painter.pop_clip();
    }
}

impl Default for HelpLineWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Draws `line` in the strip a window carries for it, clipped to the strip.
///
/// The original writes the line into every registered label at once (`0x4444c`), so each strip of
/// an open window shows whatever the pointer is over, whichever window that control belongs to.
pub fn strip(painter: &mut Painter, rect: Rect, line: &[u8]) {
    painter.push_clip(rect);
    write(painter, rect, crate::windows::HELP_STYLE, line);
    painter.pop_clip();
}

/// Prints `line` in the middle of `rect` in the font and colour `which` names, which is what a
/// label widget does with the one line it holds (`0x4deae`).
///
/// The strip keeps the texture under it and darkens it. Measured on the node menu's `back_01`
/// client in S2, the original sends 113 and 114 to 112, 115 and 116 to 113 and 117 to 145,
/// which the shadow table matches for the middle of that run. Its 112 goes to 159 where the shadow
/// table gives 146, so the darkest part of a strip is one index out.
fn write(painter: &mut Painter, rect: Rect, which: u16, line: &[u8]) {
    let Some(style) = painter.lettering(which) else {
        return;
    };
    painter.darken(rect);
    let label = painter.run(line);
    painter.text_aligned(
        Point::new(rect.x, rect.y),
        style.font,
        style.colour,
        label,
        Pen::TIGHT,
        Align::CentredInBox(rect.width, rect.height),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style;
    use crate::text::Label;

    #[test]
    fn the_label_is_narrower_than_the_window_and_holds_the_line_given() {
        let (width, height) = HelpLineWindow::size();
        assert_eq!((width, height), (280, 50));
        let area = Rect::new(0, 0, width, height);
        assert_eq!(HelpLineWindow::label(area), Rect::new(16, 21, 248, 17));
        let mut ui = Ui::new();
        ui.open(kind::HELP_LINE, area);
        let mut window = HelpLineWindow::new();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.run(&mut ui, &mut painter, b"a woodcutter");
        assert_eq!(painter.runs().get(0), b"a woodcutter");
        assert!(
            painter.ops().iter().any(|op| matches!(
                op.kind,
                crate::paint::OpKind::Text {
                    label: Label::Run(0),
                    ..
                }
            )),
            "the line is drawn as a run"
        );
    }

    #[test]
    fn a_strip_darkens_what_is_under_it_rather_than_filling_it() {
        let rect = Rect::new(12, 166, 166, 12);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        strip(&mut painter, rect, b"a woodcutter");
        assert!(
            painter
                .ops()
                .iter()
                .any(|op| op.kind == crate::paint::OpKind::Darken { rect }),
            "the strip goes through the shadow table"
        );
        assert!(
            !painter
                .ops()
                .iter()
                .any(|op| matches!(op.kind, crate::paint::OpKind::Fill { .. })),
            "nothing is filled with one index"
        );
    }

    #[test]
    fn the_window_and_a_strip_print_in_the_style_each_one_takes() {
        let font = |painter: &Painter| {
            painter.ops().iter().find_map(|op| match op.kind {
                crate::paint::OpKind::Text { font, colour, .. } => Some((font, colour)),
                _ => None,
            })
        };
        let area = Rect::new(0, 0, HelpLineWindow::WIDTH, HelpLineWindow::HEIGHT);
        let mut ui = Ui::new();
        ui.open(kind::HELP_LINE, area);
        let mut window = HelpLineWindow::new();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.run(&mut ui, &mut painter, b"a woodcutter");
        let own = style::ORIGINAL[STYLE as usize].lettering();
        assert_eq!(font(&painter), Some((own.font, own.colour)));

        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        strip(&mut painter, Rect::new(12, 166, 166, 12), b"a woodcutter");
        let shown = style::ORIGINAL[crate::windows::HELP_STYLE as usize].lettering();
        assert_eq!(font(&painter), Some((shown.font, shown.colour)));
    }

    #[test]
    fn a_rolled_up_window_draws_no_line() {
        let (width, height) = HelpLineWindow::size();
        let area = Rect::new(0, 0, width, height);
        let mut ui = Ui::new();
        ui.open(kind::HELP_LINE, area);
        ui.roll(kind::HELP_LINE);
        let mut window = HelpLineWindow::new();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.run(&mut ui, &mut painter, b"a woodcutter");
        assert!(painter.runs().is_empty());
        let found = painter.ops();
        assert!(found.is_empty(), "{found:?}");
    }
}
