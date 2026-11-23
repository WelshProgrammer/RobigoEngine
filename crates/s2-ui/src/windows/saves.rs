//! The save and load screens.
//!
//! Both are the same window, told apart by a [`Mode`]. The slots come from whatever lists the
//! player's saves; the window only shows them and says which one was pressed.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{self, Painter};
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{self, Outcome, kind};

/// The window style the screen builds every control and label in, which `0x4bf82`, `0x4c035`
/// and `0x4c107` all pass as 0.
const STYLE: u16 = 0;

/// How many slots the list holds.
pub const SLOTS: usize = 11;

/// How many of them the save screen offers, which leaves the last for loading only.
pub const SAVEABLE: usize = SLOTS - 1;

/// How many automatic save periods the save screen offers.
pub const PERIODS: usize = 5;

/// The text slot an empty row shows.
const EMPTY_SLOT: u16 = 316;

/// The text slot the button that closes the window shows.
const CANCEL_SLOT: u16 = 217;

/// The text slot the automatic save label shows.
const AUTOMATIC_SLOT: u16 = 555;

/// The text slot the first automatic save period shows.
const PERIOD_SLOT: u16 = 556;

/// Where the first row sits, how big one is and how far apart they are.
const ROW: (i32, i32, u32, u32, i32) = (16, 22, 218, 20, 22);

/// Where the button that closes the window sits, and how far down the load screen moves it.
const CANCEL: (i32, i32, u32, u32, i32) = (20, 255, 210, 20, 20);

/// Where the automatic save label sits.
const AUTOMATIC: (i32, i32, u32, u32) = (25, 280, 200, 20);

/// Where the first automatic save period sits and how far apart they are.
const PERIOD: (i32, i32, u32, u32, i32) = (25, 300, 40, 20, 40);

/// Which of the two screens a window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The screen `F1` opens, which loads.
    Load,
    /// The screen `F2` opens, which saves.
    Save,
}

impl Mode {
    /// The window kind it opens as.
    #[must_use]
    pub const fn kind(self) -> WindowId {
        match self {
            Self::Load => kind::LOAD_GAME,
            Self::Save => kind::SAVE_GAME,
        }
    }

    /// The text slot its title shows.
    #[must_use]
    pub const fn title(self) -> u16 {
        match self {
            Self::Load => 195,
            Self::Save => 194,
        }
    }

    /// How many rows it draws.
    #[must_use]
    pub const fn rows(self) -> usize {
        match self {
            Self::Load => SLOTS,
            Self::Save => SAVEABLE,
        }
    }
}

/// One slot of the list the window draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot<'a> {
    /// The number printed before the name.
    pub number: i32,
    /// The name the save was given, in cp437.
    pub name: &'a [u8],
}

/// What a release on one of the two screens asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Load the game in this slot.
    Load(usize),
    /// Save the game into this slot.
    Save(usize),
    /// Save by itself every so often, 0 being never.
    Automatic(usize),
    /// Close the window without doing anything.
    Cancel,
}

/// The save or load screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavesWindow {
    /// Which of the two it is.
    pub mode: Mode,
    /// Which automatic save period is set (`0x1f791d`).
    pub automatic: usize,
}

impl SavesWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 250;
    /// How tall the load screen is.
    pub const HEIGHT: u32 = 320;
    /// What the save screen's automatic save row adds to that.
    pub const AUTOMATIC_HEIGHT: u32 = 20;

    /// The window for `mode`, with no automatic save set.
    #[must_use]
    pub const fn new(mode: Mode) -> Self {
        Self { mode, automatic: 0 }
    }

    /// How big the window is.
    #[must_use]
    pub const fn size(mode: Mode) -> (u32, u32) {
        let height = match mode {
            Mode::Load => Self::HEIGHT,
            Mode::Save => Self::HEIGHT + Self::AUTOMATIC_HEIGHT,
        };
        (Self::WIDTH, height)
    }

    /// Where row `row` sits.
    #[must_use]
    pub fn row(row: usize, at: Point) -> Rect {
        let (x, y, width, height, step) = ROW;
        let down = i32::try_from(row).unwrap_or_default() * step;
        Rect::new(at.x + x, at.y + y + down, width, height)
    }

    /// Where the button that closes the window sits.
    #[must_use]
    pub fn cancel(mode: Mode, at: Point) -> Rect {
        let (x, y, width, height, down) = CANCEL;
        let y = y + if matches!(mode, Mode::Load) { down } else { 0 };
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where the automatic save label sits.
    #[must_use]
    pub fn automatic_label(at: Point) -> Rect {
        let (x, y, width, height) = AUTOMATIC;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where automatic save period `period` sits.
    #[must_use]
    pub fn period(period: usize, at: Point) -> Rect {
        let (x, y, width, height, step) = PERIOD;
        let across = i32::try_from(period).unwrap_or_default() * step;
        Rect::new(at.x + x + across, at.y + y, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release asks for.
    /// `slots` holds the saves in slot order, `None` for an empty slot.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        slots: &[Option<Slot<'_>>],
        mouse: Pointer,
    ) -> Option<Outcome> {
        let id = self.mode.kind();
        let window = *ui.window(id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        let mut asked = None;
        for place in 0..self.mode.rows() {
            let rect = Self::row(place, at);
            let held = slots.get(place).copied().flatten();
            let label = match held {
                Some(slot) => painter.run(&printed(slot)),
                None => Label::Slot(EMPTY_SLOT),
            };
            if held.is_none() && matches!(self.mode, Mode::Load) {
                windows::label(painter, rect, STYLE, label);
                continue;
            }
            let widget = WidgetId::new(id, ROW_WIDGET + u16::try_from(place).unwrap_or_default());
            let acted = ui.press(widget, rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), rect, look.edges());
            }
            windows::label(painter, rect, STYLE, label);
            if acted && asked.is_none() {
                asked = Some(match self.mode {
                    Mode::Load => Ask::Load(place),
                    Mode::Save => Ask::Save(place),
                });
            }
        }
        if matches!(self.mode, Mode::Save) {
            let rect = Self::automatic_label(at);
            windows::label(painter, rect, STYLE, Label::Slot(AUTOMATIC_SLOT));
            for period in 0..PERIODS {
                let rect = Self::period(period, at);
                let widget = WidgetId::new(id, PERIOD_WIDGET + u16::try_from(period).unwrap_or_default());
                let acted = ui.press(widget, rect, mouse, true);
                // The period takes the mark before it draws, so the frame the click lands on
                // already shows it.
                if acted && asked.is_none() {
                    self.automatic = period;
                    asked = Some(Ask::Automatic(period));
                }
                let look = ui.button(widget).look(true, false);
                if let Some(item) = skin.item(Control::Button, look) {
                    painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), rect, look.edges());
                }
                // The period that is set is marked rather than held, and a marked control
                // lightens its own background (`0x51898`).
                if period == self.automatic {
                    painter.lighten(rect.shrink(paint::EDGE));
                }
                let slot = PERIOD_SLOT + u16::try_from(period).unwrap_or_default();
                windows::label(painter, rect, STYLE, Label::Slot(slot));
            }
        }
        let rect = Self::cancel(self.mode, at);
        let widget = WidgetId::new(id, CANCEL_WIDGET);
        let acted = ui.press(widget, rect, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), rect, look.edges());
        }
        windows::label(painter, rect, STYLE, Label::Slot(CANCEL_SLOT));
        if acted && asked.is_none() {
            asked = Some(Ask::Cancel);
        }
        painter.pop_clip();
        asked.map(Outcome::Saves)
    }
}

/// The line a filled row shows: the slot's number and name, as `#%ld: %s` prints them
/// (`0x170c74`).
#[must_use]
fn printed(slot: Slot<'_>) -> Vec<u8> {
    let mut line = Vec::with_capacity(slot.name.len() + 8);
    line.push(b'#');
    line.extend_from_slice(slot.number.to_string().as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(slot.name);
    line
}

/// The widget slot the first row takes.
const ROW_WIDGET: u16 = 0;
/// The widget slot the first automatic save period takes.
const PERIOD_WIDGET: u16 = 32;
/// The widget slot the button that closes the window takes.
const CANCEL_WIDGET: u16 = 48;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::text::{NoLabels, Runs, WithRuns};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin
    }

    fn click(place: Rect) -> Pointer {
        Pointer::at(Point::new(place.x + 1, place.y + 1))
    }

    fn open(mode: Mode) -> (Ui, SavesWindow, Painter) {
        let (width, height) = SavesWindow::size(mode);
        let mut ui = Ui::new();
        ui.open(mode.kind(), Rect::new(0, 0, width, height));
        (
            ui,
            SavesWindow::new(mode),
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    #[test]
    fn the_save_screen_is_taller_and_offers_one_slot_fewer() {
        assert_eq!(SavesWindow::size(Mode::Load), (250, 320));
        assert_eq!(SavesWindow::size(Mode::Save), (250, 340));
        assert_eq!(Mode::Load.rows(), 11);
        assert_eq!(Mode::Save.rows(), 10);
        let at = Point::new(0, 0);
        assert_eq!(SavesWindow::row(0, at), Rect::new(16, 22, 218, 20));
        assert_eq!(SavesWindow::row(10, at), Rect::new(16, 242, 218, 20));
        assert_eq!(SavesWindow::cancel(Mode::Save, at), Rect::new(20, 255, 210, 20));
        assert_eq!(SavesWindow::cancel(Mode::Load, at), Rect::new(20, 275, 210, 20));
        assert_eq!(SavesWindow::period(0, at), Rect::new(25, 300, 40, 20));
        assert_eq!(SavesWindow::period(4, at), Rect::new(185, 300, 40, 20));
    }

    #[test]
    fn a_filled_row_asks_to_load_its_own_slot() {
        let (mut ui, mut window, mut painter) = open(Mode::Load);
        let slots = [
            None,
            Some(Slot {
                number: 7,
                name: b"kingdom",
            }),
        ];
        let on = click(SavesWindow::row(1, Point::new(0, 0)));
        window.run(&mut ui, &mut painter, &skin(), &slots, on.pressing());
        painter.reset();
        let outcome = window.run(&mut ui, &mut painter, &skin(), &slots, on.releasing());
        assert_eq!(outcome, Some(Outcome::Saves(Ask::Load(1))));
        let runs = painter.runs();
        assert_eq!(runs.get(0), b"#7: kingdom");
    }

    #[test]
    fn an_empty_row_takes_no_press_on_the_load_screen_and_one_on_the_save_screen() {
        let slots = [None, None];
        let (mut ui, mut window, mut painter) = open(Mode::Load);
        let on = click(SavesWindow::row(0, Point::new(0, 0)));
        window.run(&mut ui, &mut painter, &skin(), &slots, on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &slots, on.releasing()),
            None,
            "an empty slot cannot be loaded"
        );
        let (mut ui, mut window, mut painter) = open(Mode::Save);
        window.run(&mut ui, &mut painter, &skin(), &slots, on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &slots, on.releasing()),
            Some(Outcome::Saves(Ask::Save(0))),
            "an empty slot can be saved into"
        );
    }

    #[test]
    fn a_period_button_sets_the_automatic_save_and_marks_itself() {
        let (mut ui, mut window, mut painter) = open(Mode::Save);
        let on = click(SavesWindow::period(2, Point::new(0, 0)));
        window.run(&mut ui, &mut painter, &skin(), &[], on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &[], on.releasing()),
            Some(Outcome::Saves(Ask::Automatic(2)))
        );
        assert_eq!(window.automatic, 2);
    }

    #[test]
    fn the_cancel_button_asks_for_nothing_else() {
        let (mut ui, mut window, mut painter) = open(Mode::Load);
        let on = click(SavesWindow::cancel(Mode::Load, Point::new(0, 0)));
        window.run(&mut ui, &mut painter, &skin(), &[], on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &[], on.releasing()),
            Some(Outcome::Saves(Ask::Cancel))
        );
    }

    #[test]
    fn a_run_draws_the_text_a_window_printed() {
        let mut runs = Runs::new();
        let label = runs.add(b"#1: hello");
        assert_eq!(label, Label::Run(0));
        let table = WithRuns {
            labels: &NoLabels,
            runs: &runs,
        };
        assert_eq!(label.with_text(&table, <[u8]>::to_vec), b"#1: hello");
        assert_eq!(Label::Run(9).with_text(&table, <[u8]>::to_vec), b"");
    }
}
