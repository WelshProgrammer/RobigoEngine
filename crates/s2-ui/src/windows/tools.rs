//! The tool orders window: twelve rows, each weighing one tool the metalworks can make.

use s2_render::terrain::Point;
use s2_sim::{Command, MAX_WEIGHT, WareKind, World};
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

/// How far one press of a row's button moves its weight.
pub const STEP: u8 = 10;

/// Where the names of the tools start in the names table, which both buttons of a row report as
/// their line of help (`r + 16`, with bit 16 set to pick the names table).
const TOOL_NAMES: u16 = 16;

/// The help page the window's own button opens, which command 10941 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1005;

/// From this weight up, one press on the button that raises it goes straight to [`MAX_WEIGHT`]
/// (`0x5d6f7`).
const RAISE_FROM: u8 = 91;

/// Which way a row's button moves its weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Towards 0, which the button on the left does.
    Down,
    /// Towards [`MAX_WEIGHT`], which the button on the right does.
    Up,
}

impl Step {
    /// What one press makes of `weight`, as `0x5d67c` and `0x5d6f7` work it out.
    #[must_use]
    pub fn applied(self, weight: u8) -> u8 {
        match self {
            Self::Down if weight >= STEP => weight - STEP,
            Self::Down => 0,
            Self::Up if weight < RAISE_FROM => weight + STEP,
            Self::Up => MAX_WEIGHT,
        }
    }
}

/// What the tool orders window shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolsView {
    /// The weight of each tool, from tongs up.
    pub weights: [u8; WareKind::TOOLS],
}

impl ToolsView {
    /// The weights `player`'s metalworks give each tool.
    #[must_use]
    pub fn read(world: &World, player: Player) -> Self {
        Self {
            weights: *world.distribution(player).tools(),
        }
    }
}

/// The tool orders window.
///
/// `text_colour` stands in until the window's style record is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolsWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The palette index the weights draw in.
    pub text_colour: u8,
}

impl ToolsWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 166;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 432;
    /// A row's button.
    pub const BUTTON: (u32, u32) = (26, 26);
    /// Where a row's left button sits, and its right one.
    const BUTTON_X: (i32, i32) = (15, 126);
    /// Where the first row sits.
    const ROW_TOP: i32 = 25;
    /// How far each row sits below the one above.
    const ROW_PITCH: i32 = 28;
    /// A row's bar: x, how far it drops below the buttons, and its size.
    const BAR: (i32, i32, u32, u32) = (43, 4, 80, 16);
    /// The two buttons at the bottom.
    const FOOT: (u32, u32) = (30, 32);
    /// How far in from the left the help button sits, and from the right the other one.
    const FOOT_IN: i32 = 18;
    /// How far up from the bottom edge both sit.
    const FOOT_UP: i32 = 48;

    /// The window at `id`, drawing its weights in `text_colour`.
    #[must_use]
    pub const fn new(id: WindowId, text_colour: u8) -> Self {
        Self { id, text_colour }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// The tool row `row` weighs, from tongs up.
    #[must_use]
    pub fn tool(row: usize) -> Option<WareKind> {
        (row < WareKind::TOOLS)
            .then(|| WareKind(WareKind::FIRST_TOOL.0 + u8::try_from(row).unwrap_or_default()))
    }

    /// Where row `row`'s buttons sit in a window whose top-left corner is `at`, the one that
    /// lowers its weight first.
    #[must_use]
    pub fn buttons(row: usize, at: Point) -> (Rect, Rect) {
        let y = at.y + Self::ROW_TOP + Self::ROW_PITCH * i32::try_from(row).unwrap_or(0);
        (
            Rect::new(at.x + Self::BUTTON_X.0, y, Self::BUTTON.0, Self::BUTTON.1),
            Rect::new(at.x + Self::BUTTON_X.1, y, Self::BUTTON.0, Self::BUTTON.1),
        )
    }

    /// Where row `row`'s bar sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn bar(row: usize, at: Point) -> Rect {
        let y = at.y + Self::ROW_TOP + Self::ROW_PITCH * i32::try_from(row).unwrap_or(0);
        let (x, drop, width, height) = Self::BAR;
        Rect::new(at.x + x, y + drop, width, height)
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

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &ToolsView,
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
        for (row, &weight) in view.weights.iter().enumerate() {
            let (down, up) = Self::buttons(row, at);
            for (step, place) in [(Step::Down, down), (Step::Up, up)] {
                let landing = step.applied(weight);
                let enabled = landing != weight;
                let widget = Self::widget(self.id, row, step);
                let named = u16::try_from(row).unwrap_or_default() + TOOL_NAMES;
                ui.note_help(place, mouse, Label::Named(named));
                let acted = ui.press(widget, place, mouse, enabled);
                let look = ui.button(widget).look(enabled, false);
                if let Some(item) = skin.item(Control::Button, look) {
                    painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
                }
                if let Some(item) = skin.picture(Picture::tool(named - TOOL_NAMES, step == Step::Up)) {
                    painter.item_centred(item, place);
                }
                if acted
                    && outcome.is_none()
                    && let Some(tool) = Self::tool(row)
                {
                    outcome = Some(Outcome::Command(Command::SetToolWeight {
                        tool,
                        weight: landing,
                    }));
                }
            }
            bar::draw(painter, skin, Self::bar(row, at), u32::from(weight), false);
        }
        for (index, (slot, place, page)) in [
            (Self::HELP_SLOT, Self::help(area), Some(HELP_PAGE)),
            (Self::DEFAULTS_SLOT, Self::defaults(area), None),
        ]
        .into_iter()
        .enumerate()
        {
            let widget = WidgetId::new(self.id, slot);
            let line = [windows::HELP_BUTTON, windows::DEFAULTS_BUTTON][index.min(1)];
            ui.note_help(place, mouse, Label::Slot(line));
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if let Some(item) = skin.picture([Picture::HELP, Picture::DEFAULTS][index.min(1)]) {
                painter.item_centred(item, place);
            }
            if acted && outcome.is_none() {
                outcome = Some(match page {
                    Some(page) => Outcome::Help(page),
                    None => Outcome::Command(Command::ResetToolWeights),
                });
            }
        }
        painter.pop_clip();
        outcome
    }

    /// The widget slot the help button takes, past the rows.
    const HELP_SLOT: u16 = 64;
    /// The widget slot the button that puts the defaults back takes.
    const DEFAULTS_SLOT: u16 = 65;

    /// The widget slot for one button of one row.
    fn widget(id: WindowId, row: usize, step: Step) -> WidgetId {
        let row = u16::try_from(row).unwrap_or_default();
        let step = match step {
            Step::Down => 0,
            Step::Up => 1,
        };
        WidgetId::new(id, row * 2 + step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{ItemId, OpKind};
    use crate::windows::kind;

    const WINDOW: WindowId = kind::TOOLS;
    const TEXT: u8 = 1;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Button, Look::Disabled, ItemId(3));
        skin.set(Control::ScrollTrack, Look::Idle, ItemId(4));
        skin
    }

    fn open(weights: [u8; WareKind::TOOLS]) -> (ToolsWindow, ToolsView, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = ToolsWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        (
            ToolsWindow::new(WINDOW, TEXT),
            ToolsView { weights },
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(
        window: ToolsWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: ToolsView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), &view, on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), &view, on.releasing())
    }

    const DEFAULTS: [u8; WareKind::TOOLS] = [10, 40, 20, 50, 70, 10, 30, 10, 20, 10, 20, 10];

    #[test]
    fn a_row_sits_where_the_original_puts_it() {
        let at = Point::new(0, 0);
        let (down, up) = ToolsWindow::buttons(0, at);
        assert_eq!(down, Rect::new(15, 25, 26, 26));
        assert_eq!(up, Rect::new(126, 25, 26, 26));
        assert_eq!(ToolsWindow::bar(0, at), Rect::new(43, 29, 80, 16));
        let (_, up) = ToolsWindow::buttons(WareKind::TOOLS - 1, at);
        assert_eq!(up.y, 25 + 11 * 28);
        assert!(
            up.right() <= i32::try_from(ToolsWindow::WIDTH).expect("fits"),
            "a row fits the window"
        );
        let area = Rect::new(0, 0, ToolsWindow::WIDTH, ToolsWindow::HEIGHT);
        assert_eq!(ToolsWindow::help(area), Rect::new(18, 384, 30, 32));
        assert_eq!(ToolsWindow::defaults(area), Rect::new(118, 384, 30, 32));
        assert!(
            up.bottom() <= ToolsWindow::help(area).y,
            "the last row clears the buttons below it"
        );
    }

    #[test]
    fn the_rows_run_from_tongs_up() {
        assert_eq!(ToolsWindow::tool(0), Some(WareKind::TONGS));
        assert_eq!(ToolsWindow::tool(1), Some(WareKind::AXE));
        assert_eq!(ToolsWindow::tool(WareKind::TOOLS - 1), Some(WareKind::BOW));
        assert_eq!(ToolsWindow::tool(WareKind::TOOLS), None);
    }

    #[test]
    fn a_step_lands_inside_the_range() {
        assert_eq!(Step::Up.applied(0), 10);
        assert_eq!(Step::Up.applied(95), MAX_WEIGHT);
        assert_eq!(Step::Up.applied(MAX_WEIGHT), MAX_WEIGHT);
        assert_eq!(Step::Down.applied(10), 0);
        assert_eq!(Step::Down.applied(9), 0);
        assert_eq!(Step::Down.applied(0), 0);
    }

    #[test]
    fn a_button_carries_the_weight_its_tool_lands_on() {
        let (window, view, mut ui, mut painter) = open(DEFAULTS);
        let (down, up) = ToolsWindow::buttons(1, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view, up),
            Some(Outcome::Command(Command::SetToolWeight {
                tool: WareKind::AXE,
                weight: 50
            }))
        );
        assert_eq!(
            click(window, &mut ui, &mut painter, view, down),
            Some(Outcome::Command(Command::SetToolWeight {
                tool: WareKind::AXE,
                weight: 30
            }))
        );
    }

    #[test]
    fn a_button_with_nowhere_to_go_swallows_the_click() {
        let mut weights = DEFAULTS;
        weights[0] = 0;
        weights[1] = MAX_WEIGHT;
        let (window, view, mut ui, mut painter) = open(weights);
        let (down, _) = ToolsWindow::buttons(0, Point::new(0, 0));
        assert_eq!(click(window, &mut ui, &mut painter, view, down), None);
        let (_, up) = ToolsWindow::buttons(1, Point::new(0, 0));
        assert_eq!(click(window, &mut ui, &mut painter, view, up), None);
    }

    #[test]
    fn the_two_buttons_at_the_bottom_do_their_own_jobs() {
        let (window, view, mut ui, mut painter) = open(DEFAULTS);
        let area = Rect::new(0, 0, ToolsWindow::WIDTH, ToolsWindow::HEIGHT);
        assert_eq!(
            click(window, &mut ui, &mut painter, view, ToolsWindow::help(area)),
            Some(Outcome::Help(HELP_PAGE))
        );
        assert_eq!(
            click(window, &mut ui, &mut painter, view, ToolsWindow::defaults(area)),
            Some(Outcome::Command(Command::ResetToolWeights))
        );
    }

    #[test]
    fn every_row_draws_its_weight_and_its_bar() {
        let (window, view, mut ui, mut painter) = open(DEFAULTS);
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view,
            Pointer::at(Point::new(-1, -1)),
        );
        let shares = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Fill { .. }))
            .count();
        assert_eq!(shares, WareKind::TOOLS, "one share per row");
        let numbers = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Text { .. }))
            .count();
        assert_eq!(numbers, 0, "a settings screen's bar shows no number");
    }
}
