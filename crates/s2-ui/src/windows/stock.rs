//! The stock window: the wares and people a player holds.
//!
//! Window kind 28 shows the whole realm's stock and carries no settings. A storehouse's own
//! window ([`super::building`]) lays out its cells the same way and edits its settings.

use s2_render::terrain::Point;
use s2_sim::{Command, Refusal, StockItem, StockSetting, World};
use s2_systems::grid::Node;
use s2_systems::job::Job;
use s2_systems::player::Player;
use s2_systems::ware::WareKind;

use crate::geometry::Rect;
use crate::icons;
use crate::paint::{Align, Painter, Sheet};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{self, Outcome};

/// Where the names of the people start in the names table. A ware's name is its own number and a
/// person's is this plus the job, both with bit 16 set to pick the names table.
const PERSON_NAMES: u16 = 120;

/// The help page the window's own button opens, which command 10952 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1010;

/// The line of help the button that turns the page reports (`0x6385d`).
const TURN_HELP: u16 = 502;

/// The line of help a cell reports, which is the thing's name in the names table.
fn names(item: StockItem) -> Label {
    let named = match item {
        StockItem::Ware(ware) => u16::try_from(ware.index()).unwrap_or_default(),
        StockItem::Job(job) => PERSON_NAMES + u16::try_from(job.index()).unwrap_or_default(),
    };
    Label::Named(named)
}

/// One line of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StockRow {
    /// How many the storehouse holds.
    pub count: u16,
    /// What the player set for it.
    pub setting: StockSetting,
}

/// Which page the window shows. The original keeps the wares and the people apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StockPage {
    /// The wares, in ware order.
    #[default]
    Wares,
    /// The people, in job order.
    People,
}

impl StockPage {
    /// The other page, which the button along the bottom turns to.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Wares => Self::People,
            Self::People => Self::Wares,
        }
    }
}

/// What the storehouse window shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockView {
    /// Where the storehouse stands, which a command names it by. The screen the hub opens shows
    /// the realm's own counts and has no storehouse to command.
    pub node: Option<Node>,
    /// Every ware, in ware order.
    pub wares: Vec<StockRow>,
    /// Every job a storehouse counts, in job order.
    pub people: Vec<StockRow>,
}

impl StockView {
    /// What `player`'s storehouse on `node` holds, or the reason there is nothing to show.
    ///
    /// The check is [`World::player_storehouse`], which is the same one
    /// [`Command::SetStock`] makes, so a window that will not open and a command the world
    /// refuses give the same reason.
    ///
    /// # Errors
    ///
    /// Whatever `World::player_storehouse` reports: no building on the node, a building owned by
    /// someone else, or a building that is not a finished storehouse.
    pub fn read(world: &World, player: Player, node: Node) -> Result<Self, Refusal> {
        let record = world.player_storehouse(player, node)?;
        let store = world.storehouse(record).ok_or(Refusal::NotStorehouse)?;
        let rows = |counts: &[u16], settings: &[StockSetting]| {
            counts
                .iter()
                .zip(settings)
                .map(|(&count, &setting)| StockRow { count, setting })
                .collect()
        };
        Ok(Self {
            node: Some(node),
            wares: rows(&store.wares, &store.ware_settings),
            people: rows(&store.jobs, &store.job_settings),
        })
    }

    /// What `player` holds in all, which is the screen the hub opens: the ware counts
    /// `World::owned_wares` keeps and the people `World::people` counts, with no settings on
    /// them.
    #[must_use]
    pub fn realm(world: &World, player: Player) -> Self {
        let count = |count: u16| StockRow {
            count,
            setting: StockSetting::empty(),
        };
        Self {
            node: None,
            wares: (0..WARE_KINDS)
                .map(|ware| count(world.owned_wares(player, WareKind(ware))))
                .collect(),
            people: (0..Job::PEOPLE)
                .map(|job| count(world.people(player, Job(u8::try_from(job).unwrap_or_default()))))
                .collect(),
        }
    }

    /// What the window shows for `item`, which is how the draw reads a cell: the pages put
    /// the people in their own order, so a place is not a job.
    #[must_use]
    pub fn row(&self, item: StockItem) -> StockRow {
        let row = match item {
            StockItem::Ware(ware) => self.wares.get(ware.index()),
            StockItem::Job(job) => self.people.get(job.index()),
        };
        row.copied().unwrap_or_default()
    }

    /// The rows of `page`.
    #[must_use]
    pub fn rows(&self, page: StockPage) -> &[StockRow] {
        match page {
            StockPage::Wares => &self.wares,
            StockPage::People => &self.people,
        }
    }
}

/// How many wares the stock screen lists.
const WARE_KINDS: u8 = 31;

/// Where the icon sits inside its cell, right and down from the cell's own corner.
const ICON_AT: (i32, i32) = (0, 0);

/// Where the count sits: its right edge and how far down from the cell's corner.
const COUNT_AT: (i32, i32) = (12, 15);

/// The palette index a count above 0 draws in.
const COUNTED: u8 = 1;

/// The index a count of 0 draws in.
const NONE_LEFT: u8 = 136;

/// Where the people page draws each job, as the 30-byte table at `0x1fa8c0` orders them.
pub const PEOPLE_ORDER: [u8; Job::PEOPLE] = [
    0, 19, 20, 1, 3, 5, 2, 6, 4, 7, 13, 14, 8, 9, 10, 12, 11, 15, 18, 16, 17, 28, 21, 27, 29, 22, 23, 24, 25,
    26,
];

/// The storehouse window. It opens as the window kind the realm's stock screen uses, which is where
/// its placement came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// Which page it shows.
    pub page: StockPage,
    /// Which setting an icon toggles.
    pub setting: StockSetting,
}

impl StockWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 167;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 370;
    /// Icons in one group, four on its upper line and five on its lower.
    pub const COLUMNS: u32 = 9;
    /// Icons on a group's upper line, which is indented.
    const UPPER: u32 = 4;
    /// An icon's cell.
    pub const CELL: (u32, u32) = (28, 40);
    /// How far a group sits below the one above.
    const GROUP: i32 = 80;
    /// How far the upper line is indented.
    const INDENT: i32 = 14;
    /// How far the lower line sits below its group's top.
    const DROP: i32 = 40;
    /// Where the first cell lands inside the window: the panel's own corner (11, 22) and the
    /// (15, 13) the panel adds before it draws a cell.
    const ORIGIN: (i32, i32) = (26, 35);
    /// Where the button that opens the help page sits, how far up from the bottom edge the two
    /// buttons sit, and how big it is.
    const HELP_AT: (i32, i32, u32, u32) = (16, 47, 30, 32);
    /// How far right of it the button that turns the page sits, and how big that one is.
    const TURN_AT: (i32, u32, u32) = (34, 64, 32);
    /// The widget slot the button that opens the help page takes, past the cells.
    const HELP_SLOT: u16 = 64;
    /// The widget slot the button that turns the page takes.
    const TURN_SLOT: u16 = 65;

    /// The window at `id`, showing the wares and toggling stop.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self {
            id,
            page: StockPage::Wares,
            setting: StockSetting::STOP,
        }
    }

    /// How big the original's window is, whichever page it shows.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the icon at `place` sits in a window whose top-left corner is `at`, as `0x61692`
    /// places it: four cells on a group's indented upper line, then five flush with the left.
    #[must_use]
    pub fn cell(place: u32, at: Point) -> Rect {
        let column = place % Self::COLUMNS;
        let row = i32::try_from(place / Self::COLUMNS).unwrap_or(i32::MAX);
        let column = i32::try_from(column).unwrap_or_default();
        let width = i32::try_from(Self::CELL.0).unwrap_or_default();
        let (x, y) = if column < i32::try_from(Self::UPPER).unwrap_or_default() {
            (column * width + Self::INDENT, row * Self::GROUP)
        } else {
            (
                (column - i32::try_from(Self::UPPER).unwrap_or_default()) * width,
                row * Self::GROUP + Self::DROP,
            )
        };
        Rect::new(
            at.x.saturating_add(x + Self::ORIGIN.0),
            at.y.saturating_add(y + Self::ORIGIN.1),
            Self::CELL.0,
            Self::CELL.1,
        )
    }

    /// Where the button that opens the help page sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (x, up, width, height) = Self::HELP_AT;
        Rect::new(area.x + x, area.bottom() - up, width, height)
    }

    /// Where the button that turns the page sits in a window of `area`.
    #[must_use]
    pub fn turn(area: Rect) -> Rect {
        let (right, width, height) = Self::TURN_AT;
        let help = Self::help(area);
        Rect::new(help.x + right, help.y, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    /// Nothing happens when the window is closed, and a rolled-up window draws its frame alone.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &StockView,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        painter.push_clip(area);
        let mut outcome = None;
        if !window.rolled() {
            for index in 0..self.places() {
                let (Ok(place), Ok(slot)) = (u32::try_from(index), u16::try_from(index)) else {
                    continue;
                };
                let item = self.item(place);
                let row = view.row(item);
                let cell = Self::cell(place, Point::new(area.x, area.y));
                if cell.intersect(area).is_empty() {
                    continue;
                }
                ui.note_help(cell, mouse, names(item));
                let on = row.setting.contains(self.setting);
                if let Some(node) = view.node {
                    let widget = WidgetId::new(self.id, slot);
                    let acted = ui.press(widget, cell, mouse, true);
                    let look = ui.button(widget).look(true, on);
                    if let Some(item) = skin.item(Control::Tick, look) {
                        painter.frame(item, skin.item(Control::Edge, Look::Idle), cell);
                    }
                    if acted && outcome.is_none() {
                        outcome = Some(Outcome::Command(Command::SetStock {
                            node,
                            item,
                            setting: self.setting,
                            on: !on,
                        }));
                    }
                }
                Self::draw_cell(painter, cell, item, row.count);
            }
            if self.foot(ui, painter, skin, mouse, area) && outcome.is_none() {
                outcome = Some(Outcome::Help(HELP_PAGE));
            }
        }
        painter.pop_clip();
        outcome
    }

    /// Draws the two buttons along the bottom and gives back whether the help page was asked
    /// for. A release on the other one turns to the other page.
    fn foot(&mut self, ui: &mut Ui, painter: &mut Painter, skin: &Skin, mouse: Pointer, area: Rect) -> bool {
        let mut asked = false;
        for (slot, place, picture, line) in [
            (
                Self::HELP_SLOT,
                Self::help(area),
                Picture::HELP,
                windows::HELP_BUTTON,
            ),
            (Self::TURN_SLOT, Self::turn(area), Picture::TURN_PAGE, TURN_HELP),
        ] {
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(place, mouse, Label::Slot(line));
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if let Some(item) = skin.picture(picture) {
                painter.item_centred(item, place);
            }
            if acted {
                if slot == Self::HELP_SLOT {
                    asked = true;
                } else {
                    self.page = self.page.other();
                }
            }
        }
        asked
    }

    /// Draws one cell: the box, the thing's own picture over it and its count, the way
    /// `0x63cfe` draws them.
    fn draw_cell(painter: &mut Painter, cell: Rect, item: StockItem, count: u16) {
        let at = Point::new(cell.x + ICON_AT.0, cell.y + ICON_AT.1);
        painter.game(Sheet::Landscape, icons::CELL_TOP, at);
        let picture = match item {
            StockItem::Ware(ware) => icons::ware(ware),
            StockItem::Job(job) => icons::person(job),
        };
        if let Some(picture) = picture {
            painter.game(Sheet::Landscape, picture, at);
        }
        painter.game(
            Sheet::Landscape,
            icons::CELL_BOTTOM,
            Point::new(at.x, at.y + icons::CELL_DROP),
        );
        painter.text_aligned(
            Point::new(cell.x + COUNT_AT.0, cell.y + COUNT_AT.1),
            FontId::Small,
            if count == 0 { NONE_LEFT } else { COUNTED },
            Label::Number(i32::from(count)),
            Pen::TIGHT,
            Align::Right,
        );
    }

    /// How many cells the page it shows holds.
    #[must_use]
    pub const fn places(self) -> usize {
        match self.page {
            StockPage::Wares => WARE_KINDS as usize,
            StockPage::People => Job::PEOPLE,
        }
    }

    /// The ware or job at `place` on the page it shows. A place on the people page reads
    /// [`PEOPLE_ORDER`], which is the order the original draws them in.
    fn item(self, place: u32) -> StockItem {
        let index = usize::try_from(place).unwrap_or(usize::MAX);
        match self.page {
            StockPage::Wares => StockItem::Ware(WareKind(u8::try_from(place).unwrap_or(u8::MAX))),
            StockPage::People => StockItem::Job(Job(PEOPLE_ORDER.get(index).copied().unwrap_or(u8::MAX))),
        }
    }
}

#[cfg(test)]
mod tests {
    use s2_sim::{Building, BuildingKind, BuildingState, MapObject, Storehouse, WARE_TYPES};

    use super::*;
    use crate::paint::{ItemId, OpKind};
    use crate::widget::Press;
    use crate::windows::kind;

    const WINDOW: WindowId = kind::STOCK;
    const NODE: Node = Node { x: 10, y: 10 };

    fn player() -> Player {
        Player::new(0).expect("player 0")
    }

    /// A world holding one finished headquarters with a little stock in it.
    fn world() -> World {
        let mut world = World::empty(32, 32);
        let building = Building::new(
            BuildingKind::HEADQUARTERS,
            player(),
            NODE,
            0,
            BuildingState::NEW_HEADQUARTERS,
        );
        let record = world.add_building(building).expect("a free building slot");
        world.set_object(NODE, MapObject::Building(record));
        let mut store = Storehouse::new(record);
        store.wares[WareKind::BOARDS.index()] = 12;
        store.wares[WareKind::STONES.index()] = 7;
        store.ware_settings[WareKind::STONES.index()] = StockSetting::STOP;
        store.jobs[Job::MINER.index()] = 3;
        world.add_storehouse(store).expect("a free storehouse slot");
        world
    }

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Tick, Look::Idle, ItemId(2));
        skin.set(Control::Tick, Look::On, ItemId(3));
        skin.set(Control::Tick, Look::Pressed, ItemId(4));
        skin
    }

    #[test]
    fn the_window_shows_what_the_storehouse_holds() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        assert_eq!(view.node, Some(NODE));
        assert_eq!(view.wares.len(), WARE_TYPES);
        assert_eq!(view.people.len(), Job::PEOPLE);
        assert_eq!(view.wares[WareKind::BOARDS.index()].count, 12);
        assert_eq!(
            view.wares[WareKind::BOARDS.index()].setting,
            StockSetting::empty()
        );
        assert_eq!(
            view.wares[WareKind::STONES.index()],
            StockRow {
                count: 7,
                setting: StockSetting::STOP
            }
        );
        assert_eq!(view.wares[WareKind::WOOD.index()], StockRow::default());
        assert_eq!(view.people[Job::MINER.index()].count, 3);
        assert_eq!(view.rows(StockPage::People).len(), Job::PEOPLE);
    }

    #[test]
    fn the_people_page_reads_a_cell_by_its_job() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        let mut window = StockWindow::new(WINDOW);
        window.page = StockPage::People;
        assert_eq!(window.places(), Job::PEOPLE);
        let place = PEOPLE_ORDER
            .iter()
            .position(|&job| job == Job::MINER.0)
            .expect("the miner has a place");
        let place = u32::try_from(place).expect("fits");
        assert_eq!(window.item(place), StockItem::Job(Job::MINER));
        assert_eq!(view.row(window.item(place)).count, 3, "the miner's own count");
    }

    #[test]
    fn the_realm_screen_counts_what_the_player_holds() {
        let view = StockView::realm(&world(), player());
        assert_eq!(view.node, None, "nothing to send a command about");
        assert_eq!(view.wares.len(), WARE_TYPES);
        assert_eq!(view.people.len(), Job::PEOPLE);
        assert!(
            view.wares.iter().all(|row| row.setting.is_empty()),
            "the screen carries no settings"
        );
    }

    #[test]
    fn a_window_and_a_command_refuse_for_the_same_reason() {
        let world = world();
        let elsewhere = Node { x: 1, y: 1 };
        assert_eq!(
            StockView::read(&world, player(), elsewhere).unwrap_err(),
            Refusal::NoBuilding
        );
        let other = Player::new(1).expect("player 1");
        assert_eq!(
            StockView::read(&world, other, NODE).unwrap_err(),
            Refusal::NotYours
        );
    }

    #[test]
    fn a_click_on_an_icon_gives_the_command_that_turns_the_setting_on() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        let mut window = StockWindow::new(WINDOW);
        let (width, height) = StockWindow::size();
        let mut ui = Ui::new();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 320, 240));
        let boards = StockWindow::cell(1, Point::new(0, 0));
        let on_boards = Pointer::at(Point::new(boards.x + 2, boards.y + 2));

        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &view, on_boards.pressing()),
            None
        );
        assert_eq!(
            ui.button(WidgetId::new(WINDOW, 1)).press(),
            Some(Press::Inside),
            "the press is held on the icon"
        );
        painter.reset();
        let command = window.run(&mut ui, &mut painter, &skin(), &view, on_boards.releasing());
        assert_eq!(
            command,
            Some(Outcome::Command(Command::SetStock {
                node: NODE,
                item: StockItem::Ware(WareKind::BOARDS),
                setting: StockSetting::STOP,
                on: true
            }))
        );
    }

    #[test]
    fn an_icon_already_set_gives_the_command_that_turns_it_off() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        let mut window = StockWindow::new(WINDOW);
        let (width, height) = StockWindow::size();
        let mut ui = Ui::new();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 320, 240));
        let stones = StockWindow::cell(2, Point::new(0, 0));
        let on_stones = Pointer::at(Point::new(stones.x + 2, stones.y + 2));
        window.run(&mut ui, &mut painter, &skin(), &view, on_stones.pressing());
        let command = window.run(&mut ui, &mut painter, &skin(), &view, on_stones.releasing());
        assert_eq!(
            command,
            Some(Outcome::Command(Command::SetStock {
                node: NODE,
                item: StockItem::Ware(WareKind::STONES),
                setting: StockSetting::STOP,
                on: false
            }))
        );
    }

    #[test]
    fn sliding_off_an_icon_before_letting_go_gives_no_command() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        let mut window = StockWindow::new(WINDOW);
        let (width, height) = StockWindow::size();
        let mut ui = Ui::new();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 320, 240));
        let boards = StockWindow::cell(1, Point::new(0, 0));
        let on_boards = Pointer::at(Point::new(boards.x + 2, boards.y + 2));
        let elsewhere = StockWindow::cell(5, Point::new(0, 0));
        let away = Pointer::at(Point::new(elsewhere.x + 2, elsewhere.y + 2));
        window.run(&mut ui, &mut painter, &skin(), &view, on_boards.pressing());
        window.run(&mut ui, &mut painter, &skin(), &view, away);
        assert_eq!(
            ui.button(WidgetId::new(WINDOW, 1)).press(),
            Some(Press::Outside),
            "the press follows the pointer off the icon"
        );
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &view, away.releasing()),
            None
        );
    }

    #[test]
    fn neither_a_closed_window_nor_a_rolled_one_draws() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        let mut window = StockWindow::new(WINDOW);
        let mut ui = Ui::new();
        let mut painter = Painter::new(Rect::new(0, 0, 320, 240));
        let resting = Pointer::at(Point::new(0, 0));
        assert_eq!(window.run(&mut ui, &mut painter, &skin(), &view, resting), None);
        assert!(painter.ops().is_empty(), "a closed window draws nothing");

        let (width, height) = StockWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        ui.roll(WINDOW);
        window.run(&mut ui, &mut painter, &skin(), &view, resting);
        assert!(
            painter.ops().is_empty(),
            "a rolled window draws nothing of its own"
        );
    }

    #[test]
    fn only_the_icons_the_window_covers_are_drawn() {
        let view = StockView::read(&world(), player(), NODE).expect("a storehouse");
        let mut window = StockWindow::new(WINDOW);
        let mut ui = Ui::new();
        let upper = StockWindow::cell(0, Point::new(0, 0));
        let mut painter = Painter::new(Rect::new(0, 0, 320, 480));
        ui.open(
            WINDOW,
            Rect::new(
                0,
                0,
                StockWindow::WIDTH,
                u32::try_from(upper.bottom()).expect("positive"),
            ),
        );
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view,
            Pointer::at(Point::new(0, 0)),
        );
        let ticks = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Frame { face: item, .. } if item != ItemId(1)))
            .count();
        assert_eq!(ticks, 4, "a group's indented upper line alone");
    }

    #[test]
    fn a_group_puts_four_icons_above_five() {
        let at = Point::new(0, 0);
        assert_eq!(StockWindow::cell(0, at), Rect::new(40, 35, 28, 40));
        assert_eq!(StockWindow::cell(3, at), Rect::new(40 + 3 * 28, 35, 28, 40));
        assert_eq!(
            StockWindow::cell(4, at),
            Rect::new(26, 75, 28, 40),
            "the lower line starts flush with the left and drops 40"
        );
        let last = StockWindow::cell(8, at);
        assert_eq!(last, Rect::new(26 + 4 * 28, 75, 28, 40));
        assert_eq!(
            StockWindow::cell(9, at),
            Rect::new(40, 115, 28, 40),
            "the next group sits 80 below the one above"
        );
        assert_eq!(
            StockWindow::cell(0, Point::new(100, 200)),
            Rect::new(140, 235, 28, 40),
            "a cell moves with the window"
        );
    }

    #[test]
    fn the_people_page_draws_the_jobs_in_the_originals_order() {
        let mut order = PEOPLE_ORDER;
        order.sort_unstable();
        let every: Vec<u8> = (0..u8::try_from(Job::PEOPLE).expect("fits")).collect();
        assert_eq!(order.as_slice(), every.as_slice(), "every job once");
        assert_eq!(PEOPLE_ORDER[1], Job::BUILDER.0);
        assert_eq!(PEOPLE_ORDER[Job::PEOPLE - 1], Job::GENERAL.0);
    }
}
