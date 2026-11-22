//! Robigo's own notices: a few lines of text over a button that closes them, in S2's faces and
//! letters, for what the player is to know about a change they made.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The most lines a notice holds.
pub const LINES: usize = 4;

/// How wide the window is.
const WIDTH: u32 = 290;

/// Where the first line sits and how far apart lines are.
const FIRST_LINE: i32 = 26;
const LINE: i32 = 12;

/// The button that closes the notice, as the prompt that asks before leaving draws its own.
const BUTTON: (u32, u32) = (70, 56);
const BUTTON_STYLE: u16 = 3;

/// The room around the button.
const GAP: i32 = 8;
const FOOT: u32 = 12;

/// The style the letters draw in.
const PLAIN: u16 = 0;

/// How many characters [`wrap`] puts on a line, which the plain letters fit in the window.
const CHARACTERS: usize = 36;

/// One line of a notice: a label, or text made as the game runs, such as an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A label, drawn as it is.
    Label(Label),
    /// Text, drawn as a run of `painter`'s when the notice draws.
    Text(String),
}

impl From<Label> for Line {
    fn from(label: Label) -> Self {
        Self::Label(label)
    }
}

impl Line {
    /// The line as a label `painter` can draw this frame.
    pub fn label(&self, painter: &mut Painter) -> Label {
        match self {
            Self::Label(label) => *label,
            Self::Text(text) => painter.run(text.as_bytes()),
        }
    }
}

/// `text` broken between words into the lines a notice holds, the last cut short with an
/// ellipsis when the text runs on past [`LINES`].
#[must_use]
pub fn wrap(text: &str) -> Vec<Line> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.len() + 1 + word.len() <= CHARACTERS => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    if lines.len() > LINES {
        lines.truncate(LINES);
        if let Some(last) = lines.last_mut() {
            last.push_str("...");
        }
    }
    lines.into_iter().map(Line::Text).collect()
}

/// A notice's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl Default for NoticeWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl NoticeWindow {
    /// The window at its own kind.
    #[must_use]
    pub const fn new() -> Self {
        Self { id: kind::NOTICE }
    }

    /// How big the window is.
    #[must_use]
    pub fn size() -> (u32, u32) {
        let below = Self::button(Point::new(0, 0));
        let bottom = u32::try_from(below.y).unwrap_or_default() + below.height + FOOT;
        (WIDTH, bottom)
    }

    /// Where the button sits in a window whose corner is `at`.
    #[must_use]
    pub fn button(at: Point) -> Rect {
        let lines = i32::try_from(LINES).unwrap_or_default();
        let x = i32::try_from((WIDTH - BUTTON.0) / 2).unwrap_or_default();
        Rect::new(
            at.x + x,
            at.y + FIRST_LINE + lines * LINE + GAP,
            BUTTON.0,
            BUTTON.1,
        )
    }

    /// Takes `mouse`, draws `lines` centred into `painter` and gives back the window's closing
    /// when the button is released on.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        lines: &[Label],
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        if let Some(style) = painter.lettering(PLAIN) {
            let mut y = at.y + FIRST_LINE;
            for &line in lines.iter().take(LINES) {
                painter.text_aligned(
                    Point::new(at.x, y),
                    style.font,
                    style.colour,
                    line,
                    Pen::TIGHT,
                    Align::CentredIn(WIDTH),
                );
                y += LINE;
            }
        }
        let rect = Self::button(at);
        let widget = WidgetId::new(self.id, 0);
        let acted = ui.press(widget, rect, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.face(BUTTON_STYLE, look) {
            painter.frame_edges(item, skin.edge(BUTTON_STYLE), rect, look.edges());
        }
        if let Some(item) = skin.picture(Picture::CONFIRM) {
            painter.item_centred(item, rect);
        }
        painter.pop_clip();
        acted.then_some(Outcome::Close(self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_wraps_between_words_and_stops_at_the_last_line() {
        let lines = wrap("Can't load this save, the mods it needs are not enabled: Settlers 2 - Gold");
        assert_eq!(
            lines,
            [
                Line::Text("Can't load this save, the mods it".into()),
                Line::Text("needs are not enabled: Settlers 2 -".into()),
                Line::Text("Gold".into()),
            ]
        );
        assert_eq!(wrap(&"word ".repeat(60)).len(), LINES);
    }

    #[test]
    fn the_button_closes_the_notice() {
        let (width, height) = NoticeWindow::size();
        let mut ui = Ui::new();
        ui.open(kind::NOTICE, Rect::new(0, 0, width, height));
        let mut window = NoticeWindow::new();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let skin = Skin::new();
        let button = NoticeWindow::button(Point::new(0, 0));
        assert!(button.y + i32::try_from(button.height).unwrap() < i32::try_from(height).unwrap());
        let on = Pointer::at(Point::new(button.x + 4, button.y + 4));
        let lines = [Label::Text("A notice")];
        window.run(&mut ui, &mut painter, &skin, &lines, on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin, &lines, on.releasing()),
            Some(Outcome::Close(kind::NOTICE))
        );
    }
}
