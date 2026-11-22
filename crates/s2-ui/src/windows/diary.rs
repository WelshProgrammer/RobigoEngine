//! The diary: the letter a mission event opens, kind 16, and the small window the post office's
//! campaign button opens on the last task, kind 17.
//!
//! Both are one class in the original. The text sits in a [`TextPanel`] at the window's left, the
//! writer's portrait in the bottom-right corner and the buttons along the bottom.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::panel::{Gutter, TextPanel};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The text slot the title shows.
pub const TITLE: u16 = 180;

/// What a letter's text and the task under it are separated by (`0x171152`). The panel takes it
/// as a text of its own, which leaves three empty lines.
const BETWEEN: &[u8] = b"\r\n\r\n";

/// Where the panel sits: its left and top, and how much narrower and shorter it is than the
/// window (`0x66053`, `0x66130`).
const PANEL: (i32, i32, u32, u32) = (20, 30, 40, 50);

/// How far the portrait's right and bottom edges sit from the window's (`0x661e0`, `0x66202`).
const PORTRAIT_MARGIN: u32 = 30;

/// How far the buttons' top sits above the window's bottom edge, and how big one is.
const BUTTON: (u32, u32, u32) = (50, 100, 20);

/// The panel's flags (`0x6624f`): bit 0 takes four characters off a line and leaves the panel at
/// its top as text arrives, bit 1 leaves its background undrawn and bit 3 adds
/// [`LINE_GAP`] between lines.
const PANEL_FLAGS: u8 = 0b1011;

/// The pixels the panel adds under each line's font height (`0x59817`).
const LINE_GAP: u32 = 3;

/// The font the panel draws in.
const FONT: FontId = FontId::Medium;

/// The cell every glyph of a line sits in, which the wrap counts a line's characters in.
const CELL: u16 = 8;

/// Which of the two windows it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The letter an event opens, which pauses the game until it closes.
    Letter,
    /// The last task again, which the post office's campaign button opens.
    Note,
}

impl Shape {
    /// The window kind it opens as.
    #[must_use]
    pub const fn kind(self) -> WindowId {
        match self {
            Self::Letter => kind::DIARY,
            Self::Note => kind::NOTE,
        }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size(self) -> (u32, u32) {
        match self {
            Self::Letter => (500, 340),
            Self::Note => (400, 200),
        }
    }

    /// The portrait in its corner.
    #[must_use]
    pub const fn portrait(self) -> Picture {
        match self {
            Self::Letter => Picture::DIARY_BACKDROP,
            Self::Note => Picture::NOTE_BACKDROP,
        }
    }

    /// Whether the panel gives the portrait's width up, which the letter's does and the note's
    /// does not, so a long task runs under the portrait.
    const fn makes_room(self) -> bool {
        matches!(self, Self::Letter)
    }
}

/// One of the buttons along the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Closes the window, and for a letter takes the pause off (command `0x300c`).
    Continue,
    /// Ends the game; only the final event's letter carries it (command `0x300d`).
    EnterGate,
}

impl Press {
    /// Both, left first.
    pub const ALL: [Self; 2] = [Self::Continue, Self::EnterGate];

    /// The text slot it shows.
    #[must_use]
    pub const fn slot(self) -> u16 {
        match self {
            Self::Continue => 144,
            Self::EnterGate => 145,
        }
    }

    /// The style its face and text take.
    #[must_use]
    pub const fn style(self) -> u16 {
        match self {
            Self::Continue => 0,
            Self::EnterGate => 1,
        }
    }

    /// How far its left edge sits from the window's middle, with one button and with two.
    const fn left_of_middle(self, two: bool) -> i32 {
        match (self, two) {
            (Self::Continue, false) => -50,
            (Self::Continue, true) => -150,
            (Self::EnterGate, _) => 50,
        }
    }
}

/// The diary at either size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryWindow {
    /// Which of the two it is.
    pub shape: Shape,
    /// The lines it holds.
    pub panel: TextPanel,
    /// The portrait's width and height.
    portrait: (u32, u32),
    /// Whether it carries the button that ends the game.
    final_event: bool,
}

impl DiaryWindow {
    /// An event's letter: `page`, then `task` three lines under it when the task is not empty, the
    /// way `0x66436` fills it. `final_event` adds the button that ends the game. `portrait` is the
    /// size of [`Shape::portrait`]'s item, whose width the panel gives up.
    #[must_use]
    pub fn letter(page: &[u8], task: &[u8], final_event: bool, portrait: (u32, u32)) -> Self {
        let mut window = Self::new(Shape::Letter, final_event, portrait);
        window.take(page);
        if !task.is_empty() {
            window.take(BETWEEN);
            window.take(task);
        }
        window
    }

    /// The small window showing `task`, with a portrait of `portrait`'s size.
    #[must_use]
    pub fn note(task: &[u8], portrait: (u32, u32)) -> Self {
        let mut window = Self::new(Shape::Note, false, portrait);
        window.take(task);
        window
    }

    const fn new(shape: Shape, final_event: bool, portrait: (u32, u32)) -> Self {
        Self {
            shape,
            panel: TextPanel::new(),
            portrait,
            final_event,
        }
    }

    /// Wraps `text` into the panel of a window at its own size.
    fn take(&mut self, text: &[u8]) {
        let (width, height) = self.shape.size();
        let panel = self.panel(Rect::new(0, 0, width, height));
        self.panel
            .push(text, TextPanel::columns(panel, Gutter::of(PANEL_FLAGS)));
    }

    /// Where the panel sits in a window of `area`.
    #[must_use]
    pub fn panel(&self, area: Rect) -> Rect {
        let (x, y, narrower, shorter) = PANEL;
        let room = if self.shape.makes_room() {
            self.portrait.0
        } else {
            0
        };
        Rect::new(
            area.x + x,
            area.y + y,
            area.width.saturating_sub(narrower).saturating_sub(room),
            area.height.saturating_sub(shorter),
        )
    }

    /// Where the portrait sits in a window of `area`.
    #[must_use]
    pub fn portrait(&self, area: Rect) -> Rect {
        let (width, height) = self.portrait;
        let offset = |whole: u32, part: u32| {
            i32::try_from(whole.saturating_sub(part + PORTRAIT_MARGIN)).unwrap_or_default()
        };
        Rect::new(
            area.x + offset(area.width, width),
            area.y + offset(area.height, height),
            width,
            height,
        )
    }

    /// Where `press` sits in a window of `area`, or nothing for a button it does not carry.
    #[must_use]
    pub fn button(&self, press: Press, area: Rect) -> Option<Rect> {
        if press == Press::EnterGate && !self.final_event {
            return None;
        }
        let (up, width, height) = BUTTON;
        let middle = i32::try_from(area.width / 2).unwrap_or_default();
        let top = i32::try_from(area.height.saturating_sub(up)).unwrap_or_default();
        Some(Rect::new(
            area.x + middle + press.left_of_middle(self.final_event),
            area.y + top,
            width,
            height,
        ))
    }

    /// How far one line sits below the last, for a font whose lines are `height` tall
    /// (`0x59958`).
    #[must_use]
    pub const fn line_step(height: u32) -> u32 {
        height + LINE_GAP
    }

    /// How many lines a panel of `area` shows for a font whose lines are `height` tall: one fewer
    /// than fit (`0x5996d`).
    #[must_use]
    pub fn rows(area: Rect, height: u32) -> usize {
        let fit = area
            .height
            .checked_div(Self::line_step(height))
            .unwrap_or_default();
        usize::try_from(fit.saturating_sub(1)).unwrap_or_default()
    }

    /// Draws the window, with the font's lines `font_height` tall, and gives back what a release
    /// on a button asks for: [`Outcome::Close`] for Continue and [`Outcome::EndGame`] for the
    /// other.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        font_height: u32,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let id = self.shape.kind();
        let window = *ui.window(id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let panel = self.panel(area);
        painter.push_clip(panel);
        let step = i32::try_from(Self::line_step(font_height)).unwrap_or_default();
        let colour = painter.first_lettering().colour;
        let shown: Vec<Vec<u8>> = self.panel.shown(Self::rows(panel, font_height)).to_vec();
        for (row, line) in shown.iter().enumerate() {
            let down = i32::try_from(row).unwrap_or_default() * step;
            let label = painter.run(line);
            painter.text_spaced(
                Point::new(panel.x, panel.y + down),
                FONT,
                colour,
                label,
                Pen::cells(CELL),
            );
        }
        painter.pop_clip();
        if let Some(item) = skin.picture(self.shape.portrait()) {
            let place = self.portrait(area);
            painter.push_clip(place);
            painter.item_centred(item, place);
            painter.pop_clip();
        }
        let mut asked = None;
        for press in Press::ALL {
            let Some(place) = self.button(press, area) else {
                continue;
            };
            let widget = WidgetId::new(id, press.slot());
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            let style = press.style();
            if let Some(face) = skin.face(style, look) {
                painter.frame_edges(face, skin.edge(style), place, look.edges());
            }
            crate::windows::label(painter, place, style, Label::Slot(press.slot()));
            if acted && asked.is_none() {
                asked = Some(match press {
                    Press::Continue => Outcome::Close(id),
                    Press::EnterGate => Outcome::EndGame,
                });
            }
        }
        painter.pop_clip();
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAST: (u32, u32) = (106, 194);

    fn texts(window: &DiaryWindow) -> Vec<String> {
        window
            .panel
            .lines()
            .iter()
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect()
    }

    #[test]
    fn the_two_shapes_take_their_own_size_kind_and_portrait() {
        assert_eq!(Shape::Letter.size(), (500, 340));
        assert_eq!(Shape::Note.size(), (400, 200));
        assert_eq!(Shape::Letter.kind(), kind::DIARY);
        assert_eq!(Shape::Note.kind(), kind::NOTE);
        assert_eq!(Shape::Letter.portrait(), Picture::DIARY_BACKDROP);
        assert_eq!(Shape::Note.portrait(), Picture::NOTE_BACKDROP);
        assert_eq!(Press::Continue.slot(), 144);
        assert_eq!(Press::EnterGate.slot(), 145);
    }

    #[test]
    fn the_letter_gives_its_panel_the_width_the_portrait_leaves() {
        let letter = DiaryWindow::letter(b"", b"", false, CAST);
        let area = Rect::new(70, 70, 500, 340);
        assert_eq!(letter.panel(area), Rect::new(90, 100, 354, 290));
        assert_eq!(letter.portrait(area), Rect::new(434, 186, 106, 194));
        let note = DiaryWindow::note(b"", CAST);
        let area = Rect::new(120, 140, 400, 200);
        assert_eq!(
            note.panel(area),
            Rect::new(140, 170, 360, 150),
            "the note's panel runs under the portrait"
        );
    }

    #[test]
    fn continue_sits_in_the_middle_until_the_final_letter_adds_a_second_button() {
        let area = Rect::new(70, 70, 500, 340);
        let letter = DiaryWindow::letter(b"", b"", false, CAST);
        assert_eq!(
            letter.button(Press::Continue, area),
            Some(Rect::new(270, 360, 100, 20))
        );
        assert_eq!(letter.button(Press::EnterGate, area), None);
        let last = DiaryWindow::letter(b"", b"", true, CAST);
        assert_eq!(
            last.button(Press::Continue, area),
            Some(Rect::new(170, 360, 100, 20))
        );
        assert_eq!(
            last.button(Press::EnterGate, area),
            Some(Rect::new(370, 360, 100, 20))
        );
        let note = DiaryWindow::note(b"", CAST);
        assert_eq!(
            note.button(Press::Continue, Rect::new(120, 140, 400, 200)),
            Some(Rect::new(270, 290, 100, 20))
        );
    }

    #[test]
    fn a_letter_wraps_its_page_and_puts_the_task_three_lines_under_it() {
        let letter = DiaryWindow::letter(
            b"Day four:@@@@@@All the survivors met in order to discuss the situation.",
            b"Build a woodcutter.",
            false,
            CAST,
        );
        assert_eq!(
            texts(&letter),
            [
                "Day four:",
                "",
                "",
                "All the survivors met in order to",
                "discuss the situation.",
                "",
                "",
                "",
                "Build a woodcutter.",
            ],
            "a 354 pixel panel holds 40 characters a line"
        );
        let bare = DiaryWindow::letter(b"Day four:", b"", false, CAST);
        assert_eq!(texts(&bare), ["Day four:"], "an empty task adds nothing");
    }

    #[test]
    fn the_panel_shows_one_line_fewer_than_fit() {
        assert_eq!(DiaryWindow::line_step(11), 14);
        assert_eq!(DiaryWindow::rows(Rect::new(0, 0, 354, 290), 11), 19);
        assert_eq!(DiaryWindow::rows(Rect::new(0, 0, 360, 150), 11), 9);
        assert_eq!(DiaryWindow::rows(Rect::new(0, 0, 360, 10), 11), 0);
    }

    #[test]
    fn continue_closes_the_window_and_the_other_button_ends_the_game() {
        let area = Rect::new(0, 0, 500, 340);
        let mut ui = Ui::new();
        ui.open(kind::DIARY, area);
        let mut window = DiaryWindow::letter(b"page", b"task", true, CAST);
        let skin = Skin::new();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let at = |rect: Rect| Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        let mut click = |ui: &mut Ui, place: Rect| {
            let mut pressed = at(place);
            pressed.pressed = true;
            window.run(ui, &mut painter, &skin, 11, pressed);
            let mut released = at(place);
            released.released = true;
            window.run(ui, &mut painter, &skin, 11, released)
        };
        let gate = DiaryWindow::letter(b"", b"", true, CAST)
            .button(Press::EnterGate, area)
            .expect("the final letter carries it");
        assert_eq!(click(&mut ui, gate), Some(Outcome::EndGame));
        let carry_on = DiaryWindow::letter(b"", b"", true, CAST)
            .button(Press::Continue, area)
            .expect("every letter carries it");
        assert_eq!(click(&mut ui, carry_on), Some(Outcome::Close(kind::DIARY)));
    }
}
