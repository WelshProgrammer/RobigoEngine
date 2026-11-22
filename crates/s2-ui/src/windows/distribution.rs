//! The goods distribution window: six pages of sliders, each weighing one ware for the buildings
//! that want it.

use s2_render::terrain::Point;
use s2_sim::{Command, MAX_CONSUMERS, MAX_WEIGHT, World};
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

/// Pages the window has, one per tab.
pub const PAGES: usize = 6;

/// How far one press of a slider's button moves its weight.
pub const STEP: u8 = 10;

/// The weight a raise takes anything at or above up to [`MAX_WEIGHT`].
const RAISE_FROM: u8 = 91;

/// The text slot each tab reports as its line of help, in tab order (`0x1fcdb3`).
const TAB_HELP: [u16; PAGES] = [300, 303, 301, 304, 302, 305];

/// What each slider's caption names, page by page (`0x1fcdb3 + 0x10`). A dword with bit 16 set
/// names the second text table, which holds the buildings' names.
const CAPTIONS: [&[Label]; PAGES] = [
    &[
        Label::Named(71),
        Label::Named(70),
        Label::Named(69),
        Label::Named(68),
    ],
    &[
        Label::Named(60),
        Label::Named(57),
        Label::Named(58),
        Label::Named(67),
    ],
    &[Label::Named(64), Label::Named(63)],
    &[Label::Named(64), Label::Named(62), Label::Named(65)],
    &[Label::Slot(306), Label::Named(63), Label::Named(66)],
    &[
        Label::Named(61),
        Label::Named(67),
        Label::Named(57),
        Label::Named(58),
    ],
];

/// The style the page's own name draws in, and the style a caption takes (`0x5cfad`).
const TITLE_STYLE: u16 = 0;
const CAPTION_STYLE: u16 = 6;

/// Where the page's own name sits (`0x5cdc9`, with the y `0x5cfc6` writes over it).
const TITLE: (i32, i32, u32, u32) = (15, 53, 200, 20);

/// The help page the window's own button opens, which command 10942 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1006;

/// The picture each tab draws, in page order, which is the order of
/// [`EconomyTables::groups`](s2_systems::economy::EconomyTables::groups).
pub const TAB_PICTURES: [Picture; PAGES] = [
    Picture::DISTRIBUTION_FOOD,
    Picture::DISTRIBUTION_GRAIN,
    Picture::DISTRIBUTION_IRON,
    Picture::DISTRIBUTION_COAL,
    Picture::DISTRIBUTION_BOARDS,
    Picture::DISTRIBUTION_WATER,
];

/// Which way a slider's button moves its weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Towards 0.
    Down,
    /// Towards [`MAX_WEIGHT`].
    Up,
}

impl Step {
    /// What one press makes of `weight`, as `0x5d0ed` and `0x5d179` work it out.
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

/// What the distribution window shows for one page, read without a renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistributionView {
    /// Which page it was read for.
    pub page: u8,
    /// The weight of each slider the page has.
    pub weights: [Option<u8>; MAX_CONSUMERS],
}

impl DistributionView {
    /// `player`'s weights on `page`, which is every slider that page has.
    #[must_use]
    pub fn read(world: &World, player: Player, page: u8) -> Self {
        let mut weights = [None; MAX_CONSUMERS];
        for (consumer, slot) in weights.iter_mut().enumerate() {
            *slot = world.slider(player, usize::from(page), consumer);
        }
        Self { page, weights }
    }

    /// How many sliders the page has.
    #[must_use]
    pub fn sliders(&self) -> usize {
        self.weights.iter().take_while(|slot| slot.is_some()).count()
    }
}

/// The goods distribution window.
///
/// `text_colour` stands in until the window's style record is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistributionWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// Which page it shows.
    pub page: u8,
    /// The palette index the weights draw in.
    pub text_colour: u8,
}

impl DistributionWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 226;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 256;
    /// A tab along the top.
    pub const TAB: (u32, u32) = (34, 34);
    /// Where the first tab sits.
    const TAB_AT: (i32, i32) = (11, 21);
    /// How far apart two tabs sit.
    const TAB_STEP: i32 = 34;
    /// A slider's button.
    pub const BUTTON: (u32, u32) = (26, 26);
    /// Where a slider's left button sits, and the right one.
    const BUTTON_X: (i32, i32) = (25, 176);
    /// Where the first slider's buttons sit.
    const SLIDER_TOP: i32 = 78;
    /// How far each slider sits below the one above.
    const SLIDER_PITCH: i32 = 34;
    /// A slider's bar: x, how far it drops below the buttons, and its size.
    const BAR: (i32, i32, u32, u32) = (53, 6, 120, 16);
    /// How far above the bar's top the caption sits.
    const CAPTION_UP: i32 = 10;
    /// The two buttons at the bottom.
    const FOOT: (u32, u32) = (30, 32);
    /// Where the help button sits, and how far up from the bottom edge.
    const HELP_AT: (i32, i32) = (16, 47);
    /// How far in from the right edge and up from the bottom the other one sits.
    const DEFAULTS_AT: (i32, i32) = (48, 48);

    /// The window at `id`, showing the first page.
    #[must_use]
    pub const fn new(id: WindowId, text_colour: u8) -> Self {
        Self {
            id,
            page: 0,
            text_colour,
        }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where tab `page` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn tab(page: u8, at: Point) -> Rect {
        Rect::new(
            at.x + Self::TAB_AT.0 + i32::from(page) * Self::TAB_STEP,
            at.y + Self::TAB_AT.1,
            Self::TAB.0,
            Self::TAB.1,
        )
    }

    /// Where slider `slider`'s buttons sit, the one that lowers its weight first.
    #[must_use]
    pub fn buttons(slider: usize, at: Point) -> (Rect, Rect) {
        let y = at.y + Self::SLIDER_TOP + Self::SLIDER_PITCH * i32::try_from(slider).unwrap_or(0);
        (
            Rect::new(at.x + Self::BUTTON_X.0, y, Self::BUTTON.0, Self::BUTTON.1),
            Rect::new(at.x + Self::BUTTON_X.1, y, Self::BUTTON.0, Self::BUTTON.1),
        )
    }

    /// Where slider `slider`'s bar sits.
    #[must_use]
    pub fn bar(slider: usize, at: Point) -> Rect {
        let y = at.y + Self::SLIDER_TOP + Self::SLIDER_PITCH * i32::try_from(slider).unwrap_or(0);
        let (x, drop, width, height) = Self::BAR;
        Rect::new(at.x + x, y + drop, width, height)
    }

    /// Where slider `slider`'s caption sits, which overlaps the top of its bar.
    #[must_use]
    pub fn caption(slider: usize, at: Point) -> Rect {
        let bar = Self::bar(slider, at);
        Rect::new(bar.x, bar.y - Self::CAPTION_UP, bar.width, bar.height)
    }

    /// Where the button that opens the help page sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (x, up) = Self::HELP_AT;
        Rect::new(area.x + x, area.bottom() - up, Self::FOOT.0, Self::FOOT.1)
    }

    /// Where the button that puts the page's defaults back sits in a window of `area`.
    #[must_use]
    pub fn defaults(area: Rect) -> Rect {
        let (inset, up) = Self::DEFAULTS_AT;
        Rect::new(
            area.right() - inset,
            area.bottom() - up,
            Self::FOOT.0,
            Self::FOOT.1,
        )
    }

    /// Draws the six tabs and turns to the page a release on one of them asks for.
    fn tabs(&mut self, ui: &mut Ui, painter: &mut Painter, skin: &Skin, mouse: Pointer, at: Point) {
        for page in 0..u8::try_from(PAGES).unwrap_or(u8::MAX) {
            let tab = Self::tab(page, at);
            let widget = WidgetId::new(self.id, u16::from(page));
            if let Some(&line) = TAB_HELP.get(usize::from(page)) {
                ui.note_help(tab, mouse, Label::Slot(line));
            }
            let acted = ui.press(widget, tab, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), tab, look.edges());
            }
            if let Some(picture) = TAB_PICTURES.get(usize::from(page))
                && let Some(item) = skin.picture(*picture)
            {
                painter.item_centred(item, tab);
            }
            if acted {
                self.page = page;
            }
        }
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does. A
    /// release on a tab turns to its page and makes no command.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &DistributionView,
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
        self.tabs(ui, painter, skin, mouse, at);
        if let Some(&name) = TAB_HELP.get(usize::from(view.page)) {
            let (x, y, width, height) = TITLE;
            let rect = Rect::new(at.x + x, at.y + y, width, height);
            windows::label(painter, rect, TITLE_STYLE, Label::Slot(name));
        }
        for slider in 0..view.sliders() {
            let Some(weight) = view.weights.get(slider).copied().flatten() else {
                continue;
            };
            let (down, up) = Self::buttons(slider, at);
            for (step, place) in [(Step::Down, down), (Step::Up, up)] {
                let landing = step.applied(weight);
                let enabled = landing != weight;
                let widget = Self::widget(self.id, slider, step);
                let acted = ui.press(widget, place, mouse, enabled);
                let look = ui.button(widget).look(enabled, false);
                if let Some(item) = skin.item(Control::Button, look) {
                    painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
                }
                let picture = match step {
                    Step::Up => Picture::WEIGHT_UP,
                    Step::Down => Picture::WEIGHT_DOWN,
                };
                if let Some(item) = skin.picture(picture) {
                    painter.item_centred(item, place);
                }
                if acted && outcome.is_none() {
                    outcome = Some(Outcome::Command(Command::SetDistribution {
                        group: view.page,
                        consumer: u8::try_from(slider).unwrap_or_default(),
                        weight: landing,
                    }));
                }
            }
            bar::draw(painter, skin, Self::bar(slider, at), u32::from(weight), false);
            if let Some(&caption) = CAPTIONS
                .get(usize::from(view.page))
                .and_then(|page| page.get(slider))
            {
                windows::label(painter, Self::caption(slider, at), CAPTION_STYLE, caption);
            }
        }
        for (slot, place, page, line) in [
            (
                Self::HELP_SLOT,
                Self::help(area),
                Some(HELP_PAGE),
                windows::HELP_BUTTON,
            ),
            (
                Self::DEFAULTS_SLOT,
                Self::defaults(area),
                None,
                windows::DEFAULTS_BUTTON,
            ),
        ] {
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(place, mouse, Label::Slot(line));
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            let face = if page.is_some() {
                Picture::HELP
            } else {
                Picture::DEFAULTS
            };
            if let Some(item) = skin.picture(face) {
                painter.item_centred(item, place);
            }
            if acted && outcome.is_none() {
                outcome = Some(match page {
                    Some(page) => Outcome::Help(page),
                    None => Outcome::Command(Command::ResetDistribution { group: self.page }),
                });
            }
        }
        painter.pop_clip();
        outcome
    }

    /// The first widget slot the sliders take, past the tabs.
    const SLIDER_SLOT: u16 = 16;
    /// The widget slot the help button takes.
    const HELP_SLOT: u16 = 48;
    /// The widget slot the button that puts the defaults back takes.
    const DEFAULTS_SLOT: u16 = 49;

    /// The widget slot for one button of one slider.
    fn widget(id: WindowId, slider: usize, step: Step) -> WidgetId {
        let slider = u16::try_from(slider).unwrap_or_default();
        let step = match step {
            Step::Down => 0,
            Step::Up => 1,
        };
        WidgetId::new(id, Self::SLIDER_SLOT + slider * 2 + step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{ItemId, OpKind};
    use crate::windows::kind;

    const WINDOW: WindowId = kind::DISTRIBUTION;
    const TEXT: u8 = 1;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Button, Look::Disabled, ItemId(3));
        skin.set(Control::Tick, Look::Idle, ItemId(4));
        skin.set(Control::Tick, Look::On, ItemId(5));
        skin.set(Control::ScrollTrack, Look::Idle, ItemId(6));
        skin
    }

    fn view(page: u8, weights: [Option<u8>; MAX_CONSUMERS]) -> DistributionView {
        DistributionView { page, weights }
    }

    fn open() -> (DistributionWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = DistributionWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        (
            DistributionWindow::new(WINDOW, TEXT),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(
        window: &mut DistributionWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: DistributionView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), &view, on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), &view, on.releasing())
    }

    #[test]
    fn the_tabs_and_sliders_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(DistributionWindow::tab(0, at), Rect::new(11, 21, 34, 34));
        let last = DistributionWindow::tab(5, at);
        assert_eq!(last, Rect::new(11 + 5 * 34, 21, 34, 34));
        assert!(
            last.right() <= i32::try_from(DistributionWindow::WIDTH).expect("fits"),
            "six tabs fit the window"
        );
        let (down, up) = DistributionWindow::buttons(0, at);
        assert_eq!(down, Rect::new(25, 78, 26, 26));
        assert_eq!(up, Rect::new(176, 78, 26, 26));
        assert_eq!(DistributionWindow::bar(0, at), Rect::new(53, 84, 120, 16));
        assert_eq!(DistributionWindow::caption(0, at), Rect::new(53, 74, 120, 16));
        assert_eq!(DistributionWindow::buttons(4, at).0.y, 78 + 4 * 34);
        let area = Rect::new(0, 0, DistributionWindow::WIDTH, DistributionWindow::HEIGHT);
        assert_eq!(DistributionWindow::help(area), Rect::new(16, 209, 30, 32));
        assert_eq!(DistributionWindow::defaults(area), Rect::new(178, 208, 30, 32));
        assert_eq!(
            DistributionWindow::tab(0, Point::new(4, 6)),
            Rect::new(15, 27, 34, 34),
            "a tab moves with the window"
        );
    }

    #[test]
    fn a_step_lands_inside_the_range() {
        assert_eq!(Step::Up.applied(0), 10);
        assert_eq!(Step::Up.applied(90), 100);
        assert_eq!(
            Step::Up.applied(91),
            MAX_WEIGHT,
            "anything higher goes to the top"
        );
        assert_eq!(Step::Up.applied(95), MAX_WEIGHT);
        assert_eq!(Step::Up.applied(MAX_WEIGHT), MAX_WEIGHT);
        assert_eq!(Step::Down.applied(10), 0);
        assert_eq!(Step::Down.applied(9), 0, "anything lower goes to the bottom");
        assert_eq!(Step::Down.applied(0), 0);
    }

    #[test]
    fn the_tab_count_matches_the_tables_the_sim_ships() {
        let tables = s2_sim::EconomyTables::ORIGINAL;
        assert_eq!(tables.groups().len(), PAGES);
        assert_eq!(TAB_PICTURES.len(), PAGES);
        for group in tables.groups() {
            assert!(group.consumers.len() <= MAX_CONSUMERS);
        }
    }

    #[test]
    fn a_view_stops_at_the_sliders_the_page_has() {
        let page = view(2, [Some(50), Some(60), None, None, None]);
        assert_eq!(page.sliders(), 2);
        assert_eq!(view(0, [None; MAX_CONSUMERS]).sliders(), 0);
        assert_eq!(view(0, [Some(0); MAX_CONSUMERS]).sliders(), MAX_CONSUMERS);
    }

    #[test]
    fn a_slider_button_carries_the_weight_it_lands_on() {
        let (mut window, mut ui, mut painter) = open();
        let page = view(3, [Some(40), Some(100), Some(0), None, None]);
        let (down, up) = DistributionWindow::buttons(0, Point::new(0, 0));
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, page, up),
            Some(Outcome::Command(Command::SetDistribution {
                group: 3,
                consumer: 0,
                weight: 50
            }))
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, page, down),
            Some(Outcome::Command(Command::SetDistribution {
                group: 3,
                consumer: 0,
                weight: 30
            }))
        );
        let (_, up) = DistributionWindow::buttons(1, Point::new(0, 0));
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, page, up),
            None,
            "a slider at the top does not step up"
        );
        let (down, _) = DistributionWindow::buttons(2, Point::new(0, 0));
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, page, down),
            None,
            "a slider at the bottom does not step down"
        );
    }

    #[test]
    fn a_click_on_a_tab_turns_to_its_page() {
        let (mut window, mut ui, mut painter) = open();
        let page = view(0, [Some(50), None, None, None, None]);
        assert_eq!(window.page, 0);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                page,
                DistributionWindow::tab(4, Point::new(0, 0))
            ),
            None
        );
        assert_eq!(window.page, 4);
    }

    #[test]
    fn the_two_buttons_at_the_bottom_do_their_own_jobs() {
        let (mut window, mut ui, mut painter) = open();
        let page = view(0, [Some(50), None, None, None, None]);
        let area = Rect::new(0, 0, DistributionWindow::WIDTH, DistributionWindow::HEIGHT);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                page,
                DistributionWindow::help(area)
            ),
            Some(Outcome::Help(HELP_PAGE))
        );
        window.page = 5;
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                page,
                DistributionWindow::defaults(area)
            ),
            Some(Outcome::Command(Command::ResetDistribution { group: 5 })),
            "the defaults go back on the page the window shows"
        );
    }

    #[test]
    fn only_the_sliders_the_page_has_are_drawn() {
        let (mut window, mut ui, mut painter) = open();
        let page = view(2, [Some(50), Some(60), None, None, None]);
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &page,
            Pointer::at(Point::new(-1, -1)),
        );
        let shares = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Fill { index, .. } if index == bar::colour(50)))
            .count();
        assert_eq!(shares, 2, "one share per slider the page has");
        let texts: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Text { label, .. } => Some(label),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec![Label::Slot(TAB_HELP[2]), Label::Named(64), Label::Named(63)],
            "the page's own name and one caption per slider, and no weight"
        );
        let tabs = (0..PAGES)
            .filter(|&page| {
                let tab = DistributionWindow::tab(u8::try_from(page).unwrap_or(0), Point::new(0, 0));
                painter
                    .ops()
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::Frame { within, .. } if within == tab))
            })
            .count();
        assert_eq!(tabs, PAGES, "every tab draws whatever page is open");
    }
}
