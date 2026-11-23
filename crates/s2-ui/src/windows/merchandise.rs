//! The graph of what each ware group brought in over time.
//!
//! The window keeps which groups and which span it draws, as the original keeps them in the view
//! block so a reopened window comes up on the same choice. The graph itself is drawn by
//! [`Plot`](crate::windows::graph::Plot), which the statistics screen shares.

use s2_render::terrain::Point;
use s2_sim::{Player, SAMPLES, SERIES, TRANSPORT_GROUPS, World};

use crate::geometry::Rect;
use crate::group::GROUPS;
use crate::paint::{self, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::graph::Plot;
use crate::windows::{self, Outcome, kind};

/// The text each span button shows, which is also the line of help it reports (`0x1fd448`).
const SPAN_NAMES: [u16; SERIES] = [310, 311, 312, 313];

/// The style a span button's text takes (`0x618db`).
const SPAN_STYLE: u16 = 0;

/// The line of help the button that clears the graph reports (`0x61a4c`).
const CLEAR_HELP: u16 = 146;

/// Which groups the view starts with drawn, a bit per group (`0x43933`).
pub const FIRST_GROUP: u16 = 1;

/// Which span it starts on (`0x43929`).
pub const FIRST_SPAN: usize = 1;

/// The help page the window's own button opens (`0x61c5f`).
const HELP_PAGE: u16 = 1001;

/// One group's line, newest sample first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    /// Which group it draws.
    pub group: usize,
    /// What the group counted over each sample.
    pub counts: [u16; SAMPLES],
}

/// What the graph draws, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerchandiseView {
    /// One line per chosen group, in group order.
    pub lines: Vec<Line>,
}

impl MerchandiseView {
    /// The lines for the groups `chosen` marks, over series `span` of `player`'s samples. A bit
    /// above the fourteenth group is ignored, as is a span past the last series.
    #[must_use]
    pub fn read(world: &World, player: Player, chosen: u16, span: usize) -> Self {
        let lines = (0..TRANSPORT_GROUPS)
            .filter(|group| chosen & (1 << group) != 0)
            .map(|group| Line {
                group,
                counts: std::array::from_fn(|age| world.sample(player, span, age).counters[group]),
            })
            .collect();
        Self { lines }
    }
}

/// The graph of ware counts over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MerchandiseWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// Which groups it draws, a bit per group.
    pub chosen: u16,
    /// Which series it draws, 0 to [`SERIES`] less one.
    pub span: usize,
}

impl MerchandiseWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 252;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 310;
    /// Where the graph sits and how big it is.
    const GRAPH: (i32, i32, u32, u32) = (11, 24, 230, 262);
    /// Where the span buttons start, how far apart they sit and how big they are.
    const SPAN: (i32, i32, i32, u32, u32) = (82, 264, 38, 36, 28);
    /// Where the group buttons start and how big one is.
    const GROUP: (i32, i32, u32) = (18, 193, 30);
    /// How far apart the group buttons sit across and down.
    const GROUP_STEP: (i32, i32) = (31, 35);
    /// How many group buttons sit on the upper row.
    const GROUP_ROW: usize = 7;
    /// How far above its button a swatch sits and how tall it is.
    const SWATCH: (i32, u32) = (5, 4);
    /// Where the button that clears the choice sits and how big it is.
    const CLEAR: (i32, i32, u32, u32) = (50, 264, 30, 28);
    /// Where the help button sits above the bottom edge and how big it is.
    const HELP: (i32, i32, u32, u32) = (18, 48, 30, 32);

    /// The window at `id`, as the view starts it: the first group over the second span
    /// (`0x43929`).
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self {
            id,
            chosen: FIRST_GROUP,
            span: FIRST_SPAN,
        }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the graph sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn graph(at: Point) -> Rect {
        let (x, y, width, height) = Self::GRAPH;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where the button for span `span` sits.
    #[must_use]
    pub fn span(span: usize, at: Point) -> Rect {
        let (x, y, step, width, height) = Self::SPAN;
        let step = step * i32::try_from(span).unwrap_or_default();
        Rect::new(at.x + x + step, at.y + y, width, height)
    }

    /// Where the button for group `group` sits.
    #[must_use]
    pub fn group(group: usize, at: Point) -> Rect {
        let (x, y, size) = Self::GROUP;
        let (across, down) = Self::GROUP_STEP;
        let column = i32::try_from(group % Self::GROUP_ROW).unwrap_or_default();
        let row = i32::try_from(group / Self::GROUP_ROW).unwrap_or_default();
        Rect::new(at.x + x + column * across, at.y + y + row * down, size, size)
    }

    /// Where the swatch above that button sits.
    #[must_use]
    pub fn swatch(group: usize, at: Point) -> Rect {
        let (above, height) = Self::SWATCH;
        let button = Self::group(group, at);
        Rect::new(button.x, button.y - above, button.width, height)
    }

    /// Where the button that clears the choice sits.
    #[must_use]
    pub fn clear(at: Point) -> Rect {
        let (x, y, width, height) = Self::CLEAR;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where the help button sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (x, up, width, height) = Self::HELP;
        Rect::new(area.x + x, area.bottom() - up, width, height)
    }

    /// Whether `group` is drawn.
    #[must_use]
    pub const fn drawn(&self, group: usize) -> bool {
        self.chosen & (1 << group) != 0
    }

    /// The largest count any of `view`'s lines reaches, which the lines are measured against.
    #[must_use]
    pub fn most(view: &MerchandiseView) -> u32 {
        view.lines
            .iter()
            .flat_map(|line| line.counts)
            .map(u32::from)
            .max()
            .unwrap_or_default()
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &MerchandiseView,
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
        let plot = Plot::new(Self::graph(at), self.span);
        if let Some(item) = skin.item(Control::GraphPaper, Look::Idle) {
            plot.draw_paper(painter, item);
        }
        plot.draw(painter);
        let most = Self::most(view);
        plot.draw_most(painter, most);
        let scale = Plot::scale(most);
        for line in &view.lines {
            let colour = GROUPS.get(line.group).map_or(0, |group| group.colour);
            let counts = std::array::from_fn(|age| u32::from(line.counts[age]));
            plot.draw_line(painter, &counts, scale, colour);
        }
        let mut span = None;
        for place in 0..SERIES {
            let button = Self::span(place, at);
            let widget = WidgetId::new(
                self.id,
                Self::SPAN_SLOT + u16::try_from(place).unwrap_or_default(),
            );
            if let Some(&name) = SPAN_NAMES.get(place) {
                ui.note_help(button, mouse, Label::Slot(name));
            }
            let acted = ui.press(widget, button, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            // The span the graph draws is marked rather than held, and a marked control
            // lightens its own background (`0x51898`).
            if place == self.span {
                painter.lighten(button.shrink(paint::EDGE));
            }
            if let Some(&name) = SPAN_NAMES.get(place) {
                windows::label(painter, button, SPAN_STYLE, Label::Slot(name));
            }
            if acted && span.is_none() {
                span = Some(place);
            }
        }
        let mut flipped = None;
        for (place, group) in GROUPS.iter().enumerate() {
            let button = Self::group(place, at);
            let widget = WidgetId::new(
                self.id,
                Self::GROUP_SLOT + u16::try_from(place).unwrap_or_default(),
            );
            let acted = ui.press(widget, button, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            // A group that is drawn is marked rather than held, and a marked control lightens
            // its own background (`0x51898`).
            if self.drawn(place) {
                painter.lighten(button.shrink(paint::EDGE));
            }
            group.draw(painter, skin, button);
            painter.fill(Self::swatch(place, at), group.colour);
            if acted && flipped.is_none() {
                flipped = Some(place);
            }
        }
        let clear = Self::clear(at);
        let widget = WidgetId::new(self.id, Self::CLEAR_SLOT);
        ui.note_help(clear, mouse, Label::Slot(CLEAR_HELP));
        let cleared = ui.press(widget, clear, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), clear, look.edges());
        }
        if let Some(item) = skin.picture(Picture::CLEAR_GRAPH) {
            painter.item_centred(item, clear);
        }
        let help = Self::help(area);
        let widget = WidgetId::new(self.id, Self::HELP_SLOT);
        ui.note_help(help, mouse, Label::Slot(windows::HELP_BUTTON));
        let asked = ui.press(widget, help, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), help, look.edges());
        }
        if let Some(item) = skin.picture(Picture::HELP) {
            painter.item_centred(item, help);
        }
        painter.pop_clip();
        if let Some(span) = span {
            self.span = span;
        }
        if let Some(group) = flipped {
            self.chosen ^= 1 << group;
        }
        if cleared {
            self.chosen = 0;
        }
        asked.then_some(Outcome::Help(HELP_PAGE))
    }

    /// The widget slot the first span button takes.
    const SPAN_SLOT: u16 = 0;
    /// The widget slot the first group button takes.
    const GROUP_SLOT: u16 = 8;
    /// The widget slot the button that clears the choice takes.
    const CLEAR_SLOT: u16 = 24;
    /// The widget slot the help button takes.
    const HELP_SLOT: u16 = 25;
}

/// The kind the window opens as.
pub const KIND: WindowId = kind::MERCHANDISE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Tick, Look::Idle, ItemId(3));
        skin.set(Control::Tick, Look::On, ItemId(4));
        skin
    }

    fn open() -> (MerchandiseWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = MerchandiseWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (
            MerchandiseWindow::new(KIND),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(
        window: &mut MerchandiseWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        let view = MerchandiseView { lines: Vec::new() };
        window.run(ui, painter, &skin(), &view, on.pressing());
        painter.reset();
        let outcome = window.run(ui, painter, &skin(), &view, on.releasing());
        painter.reset();
        outcome
    }

    #[test]
    fn a_chosen_group_draws_its_line_in_its_own_colour() {
        let (mut window, mut ui, mut painter) = open();
        let view = MerchandiseView {
            lines: vec![Line {
                group: 2,
                counts: [7; SAMPLES],
            }],
        };
        assert_eq!(MerchandiseWindow::most(&view), 7);
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view,
            Pointer::at(Point::new(-1, -1)),
        );
        let lines = painter
            .ops()
            .iter()
            .filter(
                |op| matches!(op.kind, crate::paint::OpKind::Line { index, .. } if index == GROUPS[2].colour),
            )
            .count();
        assert_eq!(lines, (SAMPLES - 1) * 2, "each segment twice over");
    }

    #[test]
    fn the_controls_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(MerchandiseWindow::graph(at), Rect::new(11, 24, 230, 262));
        assert_eq!(MerchandiseWindow::span(0, at), Rect::new(82, 264, 36, 28));
        assert_eq!(MerchandiseWindow::span(3, at), Rect::new(196, 264, 36, 28));
        assert_eq!(MerchandiseWindow::group(0, at), Rect::new(18, 193, 30, 30));
        assert_eq!(MerchandiseWindow::group(6, at), Rect::new(204, 193, 30, 30));
        assert_eq!(MerchandiseWindow::group(7, at), Rect::new(18, 228, 30, 30));
        assert_eq!(MerchandiseWindow::group(13, at), Rect::new(204, 228, 30, 30));
        assert_eq!(MerchandiseWindow::swatch(0, at), Rect::new(18, 188, 30, 4));
        assert_eq!(MerchandiseWindow::clear(at), Rect::new(50, 264, 30, 28));
        let area = Rect::new(0, 0, MerchandiseWindow::WIDTH, MerchandiseWindow::HEIGHT);
        assert_eq!(MerchandiseWindow::help(area), Rect::new(18, 262, 30, 32));
    }

    #[test]
    fn a_press_flips_one_group_and_the_clear_button_takes_them_all_off() {
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        assert!(!window.drawn(2));
        click(
            &mut window,
            &mut ui,
            &mut painter,
            MerchandiseWindow::group(2, at),
        );
        assert!(window.drawn(2));
        click(
            &mut window,
            &mut ui,
            &mut painter,
            MerchandiseWindow::group(5, at),
        );
        assert!(window.drawn(2) && window.drawn(5));
        click(
            &mut window,
            &mut ui,
            &mut painter,
            MerchandiseWindow::group(2, at),
        );
        assert!(!window.drawn(2), "a second press takes it off again");
        click(&mut window, &mut ui, &mut painter, MerchandiseWindow::clear(at));
        assert_eq!(window.chosen, 0);
    }

    #[test]
    fn a_press_picks_one_span() {
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        assert_eq!(window.span, FIRST_SPAN);
        click(&mut window, &mut ui, &mut painter, MerchandiseWindow::span(2, at));
        assert_eq!(window.span, 2);
        click(&mut window, &mut ui, &mut painter, MerchandiseWindow::span(2, at));
        assert_eq!(window.span, 2, "pressing it again leaves it");
    }

    #[test]
    fn the_help_button_asks_for_its_page() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, MerchandiseWindow::WIDTH, MerchandiseWindow::HEIGHT);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, MerchandiseWindow::help(area)),
            Some(Outcome::Help(HELP_PAGE))
        );
    }

    #[test]
    fn the_view_reads_a_line_for_each_chosen_group() {
        let world = World::empty(32, 32);
        let player = Player::new(0).expect("player 0");
        let view = MerchandiseView::read(&world, player, 0, 0);
        assert!(view.lines.is_empty(), "{:?}", view.lines);
        let view = MerchandiseView::read(&world, player, 0b1001, 0);
        let groups: Vec<_> = view.lines.iter().map(|line| line.group).collect();
        assert_eq!(groups, vec![0, 3]);
        assert!(view.lines[0].counts.iter().all(|&count| count == 0));
        let wide = MerchandiseView::read(&world, player, u16::MAX, 0);
        assert_eq!(
            wide.lines.len(),
            TRANSPORT_GROUPS,
            "bits past the last group are ignored"
        );
    }

    #[test]
    fn every_group_has_its_own_colour() {
        let mut colours: Vec<_> = GROUPS.iter().map(|group| group.colour).collect();
        colours.sort_unstable();
        colours.dedup();
        assert_eq!(colours.len(), TRANSPORT_GROUPS);
    }

    #[test]
    fn a_rolled_up_window_draws_its_frame_and_takes_no_press() {
        let (mut window, mut ui, mut painter) = open();
        ui.roll(KIND);
        let place = MerchandiseWindow::group(0, Point::new(0, 0));
        assert_eq!(click(&mut window, &mut ui, &mut painter, place), None);
        assert!(window.drawn(0), "the window keeps the group it opened with");
    }
}
