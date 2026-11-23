//! The graph of how each player is doing.
//!
//! The window keeps which statistic and which span it draws. A strip of boxes along the top shows
//! each player in the game: its face, a mark for each ally, its colour and who plays it. The
//! graph itself is drawn by [`Plot`](crate::windows::graph::Plot), which the merchandise screen
//! shares.

use s2_render::colour_base;
use s2_render::terrain::Point;
use s2_sim::{Controller, Player, PlayerSlot, SAMPLES, SERIES, Statistic, World};

use crate::geometry::Rect;
use crate::paint::{self, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::graph::Plot;
use crate::windows::{self, Outcome, kind};

/// The help page the window's own button opens (`0x63137`).
const HELP_PAGE: u16 = 1003;

/// The text slot the first statistic's name takes, which the graph carries as its title. The
/// eight sit in slot order (`0x1fd4b7`).
const NAME_SLOT: u16 = 520;

/// The text each span button shows, which is also the line of help it reports
/// (`0x1fd480`).
const SPAN_NAMES: [u16; SERIES] = [310, 311, 312, 313];

/// The style a span button's text takes (`0x62fa9`).
const SPAN_STYLE: u16 = 0;

/// Which span the view starts on (`0x4393d`).
const FIRST_SPAN: usize = 1;

/// What the graph measures productivity against, whatever any player has reached (`0x634ea`).
const FULL_PRODUCTIVITY: u32 = 100;

/// The style a box's status takes (`0x62dd2`).
const STATUS_STYLE: u16 = 6;

/// What a box's status says for each controller, in [`Controller::ALL`] order (`0x1fd460`).
const STATUS: [&[u8]; 4] = [b"#1", b"#2", b"Comp", b"---"];

/// One player in the game: its box in the strip and its line on the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticsLine {
    /// Whose line it is.
    pub player: Player,
    /// Who plays it and its face.
    pub slot: PlayerSlot,
    /// The other players in the game it counts as allies, lowest first.
    pub allies: Vec<Player>,
    /// What the statistic stood at over each sample.
    pub counts: [u32; SAMPLES],
}

/// What the window draws, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticsView {
    /// One line per player in the game, lowest first.
    pub lines: Vec<StatisticsLine>,
}

impl StatisticsView {
    /// Every player in the game, with its line for `statistic` over series `span`.
    #[must_use]
    pub fn read(world: &World, statistic: Statistic, span: usize) -> Self {
        let slots = world.slots();
        let lines = slots
            .players()
            .filter_map(|player| {
                let slot = slots.get(player)?;
                let allies = slots
                    .players()
                    .filter(|&other| other != player && world.allied(player, other))
                    .collect();
                Some(StatisticsLine {
                    player,
                    slot,
                    allies,
                    counts: std::array::from_fn(|age| world.sample(player, span, age).value(statistic)),
                })
            })
            .collect();
        Self { lines }
    }
}

/// What the status under a face says for `controller`.
fn status(controller: Controller) -> &'static [u8] {
    Controller::ALL
        .iter()
        .position(|&known| known == controller)
        .and_then(|place| STATUS.get(place))
        .copied()
        .unwrap_or_default()
}

/// The graph of how each player is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatisticsWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// Which statistic it draws.
    pub statistic: Statistic,
    /// Which series it draws, 0 to [`SERIES`] less one.
    pub span: usize,
}

impl StatisticsWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 252;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 336;
    /// Where a player's portrait sits and how big it is.
    const PORTRAIT: (i32, u32, u32) = (21, 33, 46);
    /// Where a player's colour bar sits and how tall it is.
    const BAR: (i32, u32) = (67, 12);
    /// Where the first ally mark sits inside a box, how far apart they sit and how big one is.
    const ALLY: (i32, i32, i32, u32, u32) = (1, 1, 5, 4, 6);
    /// Where the graph sits and how big it is.
    const GRAPH: (i32, i32, u32, u32) = (11, 84, 230, 262);
    /// Where the span buttons start, how far apart they sit and how big they are.
    const SPAN: (i32, i32, i32, u32, u32) = (51, 288, 45, 43, 28);
    /// Where the statistic buttons start, how far apart they sit and how big they are.
    const CHOICE: (i32, i32, i32, u32, u32) = (18, 250, 27, 26, 30);
    /// Where the help button sits above the bottom edge and how big it is.
    const HELP: (i32, i32, u32, u32) = (18, 48, 30, 32);

    /// The window at `id`, as the view starts it: the land over the second span (`0x4393d`).
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self {
            id,
            statistic: Statistic::Land,
            span: FIRST_SPAN,
        }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the `place`th box of a strip of `count` sits.
    #[must_use]
    pub fn portrait(place: usize, count: usize, at: Point) -> Rect {
        let (y, width, height) = Self::PORTRAIT;
        Rect::new(at.x + Self::column(place, count), at.y + y, width, height)
    }

    /// Where that box's colour bar sits.
    #[must_use]
    pub fn bar(place: usize, count: usize, at: Point) -> Rect {
        let (y, height) = Self::BAR;
        let (_, width, _) = Self::PORTRAIT;
        Rect::new(at.x + Self::column(place, count), at.y + y, width, height)
    }

    /// Where the `ally`th ally mark of that box sits.
    #[must_use]
    pub fn ally(place: usize, count: usize, ally: usize, at: Point) -> Rect {
        let (left, top, step, width, height) = Self::ALLY;
        let (y, _, _) = Self::PORTRAIT;
        let across = step * i32::try_from(ally).unwrap_or_default();
        Rect::new(
            at.x + Self::column(place, count) + left + across,
            at.y + y + top,
            width,
            height,
        )
    }

    /// Where the graph sits.
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

    /// Where the button for `statistic` sits.
    #[must_use]
    pub fn choice(statistic: Statistic, at: Point) -> Rect {
        let (x, y, step, width, height) = Self::CHOICE;
        let step = step * i32::try_from(statistic.index()).unwrap_or_default();
        Rect::new(at.x + x + step, at.y + y, width, height)
    }

    /// Where the help button sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (x, up, width, height) = Self::HELP;
        Rect::new(area.x + x, area.bottom() - up, width, height)
    }

    /// How far across the window the `place`th box of a strip of `count` starts.
    fn column(place: usize, count: usize) -> i32 {
        let (_, width, _) = Self::PORTRAIT;
        let width = i32::try_from(width).unwrap_or_default();
        let span = i32::try_from(Self::WIDTH).unwrap_or_default();
        let count = i32::try_from(count).unwrap_or_default();
        let place = i32::try_from(place).unwrap_or_default();
        span / 2 - count * width / 2 + place * width
    }

    /// The text slot that names `statistic`.
    #[must_use]
    pub fn name(statistic: Statistic) -> u16 {
        NAME_SLOT + u16::try_from(statistic.index()).unwrap_or_default()
    }

    /// The value the lines are measured against: the largest any of `view`'s lines reaches, or a
    /// fixed hundred for productivity.
    #[must_use]
    pub fn most(statistic: Statistic, view: &StatisticsView) -> u32 {
        if statistic == Statistic::Productivity {
            return FULL_PRODUCTIVITY;
        }
        view.lines
            .iter()
            .flat_map(|line| line.counts)
            .max()
            .unwrap_or_default()
    }

    /// Draws a box for each of `view`'s players along the top of a window whose top-left corner
    /// is `at`: its face, then a mark for each ally, then its colour bar with who plays it on
    /// top, one box after another as the constructor builds them (`0x62e0e`).
    fn draw_strip(painter: &mut Painter, skin: &Skin, view: &StatisticsView, at: Point) {
        let count = view.lines.len();
        for (place, line) in view.lines.iter().enumerate() {
            let face = u16::from(line.slot.portrait.0);
            if let Some(item) = skin.picture(Picture::portrait(face)) {
                // A face is a pixel wider and taller than its box, and the picture widget clips
                // it to the box (`0x4fb68`), which leaves the next box's first column alone.
                let frame = Self::portrait(place, count, at);
                painter.push_clip(frame);
                painter.item_centred(item, frame);
                painter.pop_clip();
            }
            for (mark, &ally) in line.allies.iter().enumerate() {
                painter.fill(Self::ally(place, count, mark, at), colour_base(ally) + 1);
            }
            let bar = Self::bar(place, count, at);
            painter.fill(bar, colour_base(line.player) + 1);
            let text = painter.run(status(line.slot.controller));
            windows::label(painter, bar, STATUS_STYLE, text);
        }
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &StatisticsView,
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
        Self::draw_strip(painter, skin, view, at);
        let plot = Plot::new(Self::graph(at), self.span);
        if let Some(item) = skin.item(Control::GraphPaper, Look::Idle) {
            plot.draw_paper(painter, item);
        }
        plot.draw(painter);
        plot.draw_title(painter, Label::Slot(Self::name(self.statistic)));
        let most = Self::most(self.statistic, view);
        plot.draw_most(painter, most);
        let scale = Plot::scale(most);
        for line in &view.lines {
            plot.draw_line(painter, &line.counts, scale, colour_base(line.player));
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
        let mut chosen = None;
        for statistic in Statistic::ALL {
            let button = Self::choice(statistic, at);
            let slot = Self::CHOICE_SLOT + u16::try_from(statistic.index()).unwrap_or_default();
            let widget = WidgetId::new(self.id, slot);
            let name = Self::name(statistic);
            ui.note_help(button, mouse, Label::Slot(name));
            let acted = ui.press(widget, button, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            let picture = Picture::statistic(u16::try_from(statistic.index()).unwrap_or_default());
            if let Some(item) = skin.picture(picture) {
                painter.item_centred(item, button);
            }
            if acted && chosen.is_none() {
                chosen = Some(statistic);
            }
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
        if let Some(statistic) = chosen {
            self.statistic = statistic;
        }
        asked.then_some(Outcome::Help(HELP_PAGE))
    }

    /// The widget slot the first span button takes.
    const SPAN_SLOT: u16 = 0;
    /// The widget slot the first statistic button takes.
    const CHOICE_SLOT: u16 = 8;
    /// The widget slot the help button takes.
    const HELP_SLOT: u16 = 20;
}

/// The kind the window opens as.
pub const KIND: WindowId = kind::STATISTICS;

#[cfg(test)]
mod tests {
    use s2_sim::{PlayerSlots, Portraits};

    use super::*;
    use crate::paint::{ItemId, OpKind};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Tick, Look::Idle, ItemId(3));
        skin.set(Control::Tick, Look::On, ItemId(4));
        skin
    }

    fn open() -> (StatisticsWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = StatisticsWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (
            StatisticsWindow::new(KIND),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn players(count: u8) -> Vec<Player> {
        (0..count).filter_map(Player::new).collect()
    }

    fn world(count: usize) -> World {
        let mut world = World::empty(32, 32);
        world.set_slots(PlayerSlots::offered(count, &Portraits::ORIGINAL));
        world
    }

    fn view(count: u8) -> StatisticsView {
        StatisticsView::read(&world(usize::from(count)), Statistic::Land, 0)
    }

    fn click(
        window: &mut StatisticsWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        let view = view(1);
        window.run(ui, painter, &skin(), &view, on.pressing());
        painter.reset();
        let outcome = window.run(ui, painter, &skin(), &view, on.releasing());
        painter.reset();
        outcome
    }

    #[test]
    fn the_graph_names_its_statistic_and_measures_productivity_against_a_hundred() {
        assert_eq!(StatisticsWindow::name(Statistic::Land), 520);
        assert_eq!(StatisticsWindow::name(Statistic::FightsWon), 527);
        let mut view = view(2);
        view.lines.truncate(1);
        view.lines[0].counts = [3; SAMPLES];
        assert_eq!(StatisticsWindow::most(Statistic::Land, &view), 3);
        assert_eq!(
            StatisticsWindow::most(Statistic::Productivity, &view),
            100,
            "productivity always draws against a full hundred"
        );
    }

    #[test]
    fn the_controls_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(StatisticsWindow::graph(at), Rect::new(11, 84, 230, 262));
        assert_eq!(StatisticsWindow::span(0, at), Rect::new(51, 288, 43, 28));
        assert_eq!(StatisticsWindow::span(3, at), Rect::new(186, 288, 43, 28));
        assert_eq!(
            StatisticsWindow::choice(Statistic::Land, at),
            Rect::new(18, 250, 26, 30)
        );
        assert_eq!(
            StatisticsWindow::choice(Statistic::FightsWon, at),
            Rect::new(207, 250, 26, 30)
        );
        let area = Rect::new(0, 0, StatisticsWindow::WIDTH, StatisticsWindow::HEIGHT);
        assert_eq!(StatisticsWindow::help(area), Rect::new(18, 288, 30, 32));
    }

    #[test]
    fn the_player_boxes_are_centred_on_the_window() {
        let at = Point::new(0, 0);
        assert_eq!(StatisticsWindow::portrait(0, 1, at), Rect::new(110, 21, 33, 46));
        assert_eq!(StatisticsWindow::portrait(0, 2, at), Rect::new(93, 21, 33, 46));
        assert_eq!(StatisticsWindow::portrait(1, 2, at), Rect::new(126, 21, 33, 46));
        assert_eq!(StatisticsWindow::bar(1, 2, at), Rect::new(126, 67, 33, 12));
        assert_eq!(StatisticsWindow::ally(1, 2, 0, at), Rect::new(127, 22, 4, 6));
        assert_eq!(StatisticsWindow::ally(1, 2, 2, at), Rect::new(137, 22, 4, 6));
    }

    #[test]
    fn each_box_draws_its_own_players_shade() {
        let (window, mut ui, mut painter) = open();
        let mut window = window;
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view(3),
            Pointer::at(Point::new(-1, -1)),
        );
        let fills: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Fill { rect, index } => Some((rect.x, index)),
                _ => None,
            })
            .collect();
        assert_eq!(fills, vec![(77, 129), (110, 133), (143, 137)]);
    }

    #[test]
    fn each_box_shows_its_face_its_allies_and_who_plays_it() {
        let (mut window, mut ui, mut painter) = open();
        let mut skin = skin();
        for face in 0..Picture::PORTRAITS {
            skin.set_picture(Picture::portrait(face), ItemId(100 + face));
        }
        let mut view = view(3);
        view.lines[1].allies = players(3).into_iter().filter(|p| p.index() != 1).collect();
        view.lines[2].slot.controller = Controller::Beaten;
        window.run(
            &mut ui,
            &mut painter,
            &skin,
            &view,
            Pointer::at(Point::new(-1, -1)),
        );
        let faces: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::ItemIn { item, within, .. } if item.0 >= 100 => Some((item.0 - 100, within.x)),
                _ => None,
            })
            .collect();
        assert_eq!(faces, vec![(0, 77), (3, 110), (4, 143)]);
        let marks: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Fill { rect, index } if rect.height == 6 => Some((rect.x, index)),
                _ => None,
            })
            .collect();
        assert_eq!(
            marks,
            vec![(111, 129), (116, 137)],
            "a mark for each ally, in the ally's colour"
        );
        let said: Vec<_> = (0..3).map(|run| painter.runs().get(run).to_vec()).collect();
        assert_eq!(said, [b"#1".to_vec(), b"Comp".to_vec(), b"---".to_vec()]);
    }

    #[test]
    fn a_press_picks_one_statistic_and_one_span() {
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        assert_eq!(window.statistic, Statistic::Land);
        click(
            &mut window,
            &mut ui,
            &mut painter,
            StatisticsWindow::choice(Statistic::Gold, at),
        );
        assert_eq!(window.statistic, Statistic::Gold);
        click(&mut window, &mut ui, &mut painter, StatisticsWindow::span(1, at));
        assert_eq!(window.span, 1);
        assert_eq!(window.statistic, Statistic::Gold, "the statistic stays");
    }

    #[test]
    fn the_help_button_asks_for_its_page() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, StatisticsWindow::WIDTH, StatisticsWindow::HEIGHT);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, StatisticsWindow::help(area)),
            Some(Outcome::Help(HELP_PAGE))
        );
    }

    #[test]
    fn the_view_reads_one_line_for_each_player_in_the_game() {
        let mut world = world(3);
        let view = StatisticsView::read(&world, Statistic::Buildings, 0);
        let order: Vec<_> = view.lines.iter().map(|line| line.player).collect();
        assert_eq!(order, players(3));
        assert!(view.lines[0].counts.iter().all(|&count| count == 0));
        assert_eq!(view.lines[0].counts.len(), SAMPLES);
        assert!(view.lines.iter().all(|line| line.allies.is_empty()));
        let mut slots = *world.slots();
        slots.set(Player::new(1).expect("player 1"), None);
        world.set_slots(slots);
        let view = StatisticsView::read(&world, Statistic::Buildings, 0);
        let order: Vec<_> = view.lines.iter().map(|line| line.player.index()).collect();
        assert_eq!(order, [0, 2], "a player out of the game has no box");
    }
}
