//! The military settings window: seven rows, each a bar between the pair of buttons that step it.

use s2_render::terrain::Point;
use s2_sim::{Command, MILITARY_SETTINGS, MOST_MILITARY, World};
use s2_systems::player::Player;

use crate::bar;
use crate::geometry::Rect;
use crate::paint::Painter;
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{self, Outcome};

/// How far each row's buttons move its setting, from the byte at `+0x1c` of the row records at
/// `0x1fcc40`.
pub const STEPS: [u8; MILITARY_SETTINGS] = [10, 25, 25, 25, 10, 10, 10];

/// The help page the window's left button opens, which command 10936 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1002;

/// What the military settings window shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MilitaryView {
    /// Each setting, in the order the rows run.
    pub settings: [u8; MILITARY_SETTINGS],
}

impl MilitaryView {
    /// `player`'s settings.
    #[must_use]
    pub fn read(world: &World, player: Player) -> Self {
        Self {
            settings: world.military_settings(player),
        }
    }
}

/// The text slots each row's two buttons report, the one that raises first, in row order
/// (`0x1fcc40` words `+0x04` and `+0x06`).
const ROW_HELP: [(u16, u16); 7] = [
    (501, 500),
    (511, 510),
    (513, 512),
    (515, 514),
    (516, 516),
    (517, 517),
    (518, 518),
];

/// Which control of a row a widget slot stands for. Each row holds three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Down,
    Up,
}

/// The military settings window.
///
/// `text_colour` stands in until the window's style record is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MilitaryWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The palette index the values draw in.
    pub text_colour: u8,
}

impl MilitaryWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 168;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 298;
    /// Where the first row's buttons sit.
    const ROW_TOP: i32 = 25;
    /// How far a row sits below the one above.
    const ROW_PITCH: i32 = 32;
    /// A stepping button.
    const BUTTON: (u32, u32) = (26, 26);
    /// Where the button that steps a setting down sits.
    const DOWN_X: i32 = 17;
    /// Where the button that steps a setting up sits.
    const UP_X: i32 = 124;
    /// Where a row's bar sits, and how far it drops below the buttons.
    const BAR: (i32, i32, u32, u32) = (45, 4, 76, 16);
    /// The two buttons below the rows.
    const FOOT: (u32, u32) = (30, 32);
    /// How far above the bottom edge the two buttons sit.
    const FOOT_UP: i32 = 48;
    /// How far in from the left the help button sits, and from the right the other one.
    const FOOT_IN: i32 = 18;

    /// The window at `id`, drawing its values in `text_colour`.
    #[must_use]
    pub const fn new(id: WindowId, text_colour: u8) -> Self {
        Self { id, text_colour }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where row `row`'s stepping buttons sit in a window whose top-left corner is `at`, the one
    /// that steps down first.
    #[must_use]
    pub fn buttons(row: usize, at: Point) -> (Rect, Rect) {
        let y = at.y + Self::ROW_TOP + Self::ROW_PITCH * i32::try_from(row).unwrap_or_default();
        (
            Rect::new(at.x + Self::DOWN_X, y, Self::BUTTON.0, Self::BUTTON.1),
            Rect::new(at.x + Self::UP_X, y, Self::BUTTON.0, Self::BUTTON.1),
        )
    }

    /// Where row `row`'s bar sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn bar(row: usize, at: Point) -> Rect {
        let y = at.y + Self::ROW_TOP + Self::ROW_PITCH * i32::try_from(row).unwrap_or_default();
        Rect::new(at.x + Self::BAR.0, y + Self::BAR.1, Self::BAR.2, Self::BAR.3)
    }

    /// Where the button that opens the help page sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        Rect::new(
            area.x + Self::FOOT_IN,
            area.bottom() - Self::FOOT_UP,
            Self::FOOT.0,
            Self::FOOT.1,
        )
    }

    /// Where the button that puts the defaults back sits in a window of `area`.
    #[must_use]
    pub fn defaults(area: Rect) -> Rect {
        Rect::new(
            area.right() - Self::FOOT_UP,
            area.bottom() - Self::FOOT_UP,
            Self::FOOT.0,
            Self::FOOT.1,
        )
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release on one of its
    /// controls does. Nothing happens when the window is closed, and a rolled-up window draws its
    /// frame alone.
    pub fn run(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &MilitaryView,
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
        let mut outcome = None;
        for (row, &value) in view.settings.iter().enumerate() {
            let (down, up) = Self::buttons(row, at);
            for (part, place) in [(Part::Down, down), (Part::Up, up)] {
                let enabled = Self::moved(row, value, part) != value;
                let widget = Self::widget(self.id, row, part);
                if let Some(line) = Self::line(row, part) {
                    ui.note_help(place, mouse, Label::Slot(line));
                }
                let acted = ui.press(widget, place, mouse, enabled);
                let look = ui.button(widget).look(enabled, false);
                if let Some(item) = skin.item(Control::Button, look) {
                    painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
                }
                let number = u16::try_from(row).unwrap_or_default();
                if let Some(item) = skin.picture(Picture::military(number, part == Part::Up)) {
                    painter.item_centred(item, place);
                }
                if acted && outcome.is_none() {
                    outcome = Some(Outcome::Command(Command::SetMilitary {
                        setting: u8::try_from(row).unwrap_or_default(),
                        value: Self::moved(row, value, part),
                    }));
                }
            }
            bar::draw(painter, skin, Self::bar(row, at), u32::from(value), false);
        }
        let help = Self::help(area);
        let widget = WidgetId::new(self.id, Self::HELP_SLOT);
        ui.note_help(help, mouse, Label::Slot(windows::HELP_BUTTON));
        if ui.press(widget, help, mouse, true) && outcome.is_none() {
            outcome = Some(Outcome::Help(HELP_PAGE));
        }
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), help, look.edges());
        }
        if let Some(item) = skin.picture(Picture::HELP) {
            painter.item_centred(item, help);
        }
        let back = Self::defaults(area);
        let widget = WidgetId::new(self.id, Self::DEFAULTS_SLOT);
        ui.note_help(back, mouse, Label::Slot(windows::DEFAULTS_BUTTON));
        if ui.press(widget, back, mouse, true) && outcome.is_none() {
            outcome = Some(Outcome::Command(Command::ResetMilitary));
        }
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), back, look.edges());
        }
        if let Some(item) = skin.picture(Picture::DEFAULTS) {
            painter.item_centred(item, back);
        }
        painter.pop_clip();
        outcome
    }

    /// The text slot one part of row `row` reports as its line of help, taken from the
    /// original's own row records at `0x1fcc40`.
    fn line(row: usize, part: Part) -> Option<u16> {
        let (up, down) = *ROW_HELP.get(row)?;
        Some(match part {
            Part::Up => up,
            Part::Down => down,
        })
    }

    /// The widget slot the help button takes, past the rows.
    const HELP_SLOT: u16 = 240;
    /// The widget slot the button that puts the defaults back takes.
    const DEFAULTS_SLOT: u16 = 241;

    /// The widget slot for one part of one row.
    fn widget(id: WindowId, row: usize, part: Part) -> WidgetId {
        let row = u16::try_from(row).unwrap_or_default();
        let part = match part {
            Part::Down => 0,
            Part::Up => 1,
        };
        WidgetId::new(id, row * 2 + part)
    }

    /// What one press of a row's button makes of `value`, as `0x5c96c` works it out: a step up
    /// stops at [`MOST_MILITARY`], a step down at 0.
    fn moved(row: usize, value: u8, part: Part) -> u8 {
        let step = STEPS.get(row).copied().unwrap_or_default();
        match part {
            Part::Up => value.saturating_add(step).min(MOST_MILITARY),
            Part::Down => value.saturating_sub(step),
        }
    }
}

#[cfg(test)]
mod tests {
    use s2_sim::{DEFENCE, OCCUPATION, RANK, RECRUITING};

    use super::*;
    use crate::paint::ItemId;
    use crate::widget::Press;
    use crate::windows::kind;

    const WINDOW: WindowId = kind::MILITARY;
    const TEXT: u8 = 1;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Button, Look::Pressed, ItemId(3));
        skin.set(Control::Button, Look::Disabled, ItemId(4));
        skin.set(Control::ScrollTrack, Look::Idle, ItemId(5));
        skin
    }

    fn open(settings: [u8; MILITARY_SETTINGS]) -> (MilitaryWindow, MilitaryView, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = MilitaryWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        (
            MilitaryWindow::new(WINDOW, TEXT),
            MilitaryView { settings },
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(
        window: MilitaryWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: MilitaryView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), &view, on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), &view, on.releasing())
    }

    #[test]
    fn a_row_sits_where_the_original_puts_it() {
        let at = Point::new(0, 0);
        let (down, up) = MilitaryWindow::buttons(0, at);
        assert_eq!(down, Rect::new(17, 25, 26, 26));
        assert_eq!(up, Rect::new(124, 25, 26, 26));
        assert_eq!(MilitaryWindow::bar(0, at), Rect::new(45, 29, 76, 16));
        let (down, up) = MilitaryWindow::buttons(6, at);
        assert_eq!(down, Rect::new(17, 25 + 6 * 32, 26, 26));
        assert_eq!(up.right(), 150);
        assert!(
            up.right() <= i32::try_from(MilitaryWindow::WIDTH).expect("fits"),
            "the row fits the window"
        );
        let area = Rect::new(0, 0, MilitaryWindow::WIDTH, MilitaryWindow::HEIGHT);
        assert_eq!(MilitaryWindow::help(area), Rect::new(18, 250, 30, 32));
        assert_eq!(MilitaryWindow::defaults(area), Rect::new(120, 250, 30, 32));
        assert!(
            MilitaryWindow::buttons(6, at).0.bottom() <= MilitaryWindow::help(area).y,
            "the last row clears the buttons below it"
        );
        let moved = MilitaryWindow::buttons(1, Point::new(10, 20));
        assert_eq!(moved.0, Rect::new(27, 77, 26, 26), "a row moves with the window");
    }

    #[test]
    fn a_step_carries_the_value_it_lands_on() {
        let (window, view, mut ui, mut painter) = open([100, 50, 100, 50, 10, 30, 100]);
        let (down, up) = MilitaryWindow::buttons(RANK, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view, up),
            Some(Outcome::Command(Command::SetMilitary {
                setting: 1,
                value: 75
            })),
            "the rank row steps by 25"
        );
        assert_eq!(
            click(window, &mut ui, &mut painter, view, down),
            Some(Outcome::Command(Command::SetMilitary {
                setting: 1,
                value: 25
            }))
        );
        let (down, _) = MilitaryWindow::buttons(OCCUPATION, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view, down),
            Some(Outcome::Command(Command::SetMilitary { setting: 4, value: 0 })),
            "an occupation row steps by 10"
        );
    }

    #[test]
    fn a_step_that_would_leave_the_range_is_refused() {
        let (window, view, mut ui, mut painter) = open([100, 50, 100, 50, 0, 30, 100]);
        let (_, up) = MilitaryWindow::buttons(RECRUITING, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view, up),
            None,
            "a setting already at the top does not step up"
        );
        let (down, _) = MilitaryWindow::buttons(OCCUPATION, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view, down),
            None,
            "a setting already at 0 does not step down"
        );
        let (down, _) = MilitaryWindow::buttons(DEFENCE, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view, down),
            Some(Outcome::Command(Command::SetMilitary {
                setting: 2,
                value: 75
            }))
        );
    }

    #[test]
    fn the_two_buttons_below_the_rows_do_their_own_jobs() {
        let (window, view, mut ui, mut painter) = open([100, 50, 100, 50, 10, 30, 100]);
        let area = Rect::new(0, 0, MilitaryWindow::WIDTH, MilitaryWindow::HEIGHT);
        assert_eq!(
            click(window, &mut ui, &mut painter, view, MilitaryWindow::help(area)),
            Some(Outcome::Help(HELP_PAGE))
        );
        assert_eq!(
            click(
                window,
                &mut ui,
                &mut painter,
                view,
                MilitaryWindow::defaults(area)
            ),
            Some(Outcome::Command(Command::ResetMilitary))
        );
    }

    #[test]
    fn sliding_off_a_button_before_letting_go_does_nothing() {
        let (window, view, mut ui, mut painter) = open([100, 50, 100, 50, 10, 30, 100]);
        let (_, up) = MilitaryWindow::buttons(RANK, Point::new(0, 0));
        let on = Pointer::at(Point::new(up.x + 1, up.y + 1));
        let away = Pointer::at(Point::new(up.x + 1, up.bottom() + 4));
        window.run(&mut ui, &mut painter, &skin(), &view, on.pressing());
        window.run(&mut ui, &mut painter, &skin(), &view, away);
        assert_eq!(ui.button(WidgetId::new(WINDOW, 3)).press(), Some(Press::Outside));
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &view, away.releasing()),
            None
        );
    }

    #[test]
    fn neither_a_closed_window_nor_a_rolled_one_draws() {
        let view = MilitaryView {
            settings: [100, 50, 100, 50, 10, 30, 100],
        };
        let window = MilitaryWindow::new(WINDOW, TEXT);
        let mut ui = Ui::new();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let resting = Pointer::at(Point::new(0, 0));
        assert_eq!(window.run(&mut ui, &mut painter, &skin(), &view, resting), None);
        let found = painter.ops();
        assert!(found.is_empty(), "{found:?}");

        let (width, height) = MilitaryWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        ui.roll(WINDOW);
        window.run(&mut ui, &mut painter, &skin(), &view, resting);
        assert!(
            painter.ops().is_empty(),
            "a rolled window draws nothing of its own"
        );
    }

    #[test]
    fn every_row_draws_its_value_and_its_pair_of_buttons() {
        let (window, view, mut ui, mut painter) = open([100, 50, 100, 50, 10, 30, 100]);
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view,
            Pointer::at(Point::new(-1, -1)),
        );
        let numbers = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, crate::paint::OpKind::Text { .. }))
            .count();
        assert_eq!(numbers, 0, "a settings screen's bar shows no number");
        let shares = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, crate::paint::OpKind::Fill { .. }))
            .count();
        assert_eq!(shares, MILITARY_SETTINGS, "one share per row");
    }
}
