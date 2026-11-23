//! A building's window.
//!
//! One opens for each of the player's buildings clicked on the map. Its page is fixed when it
//! opens: a site's window closes when the site finishes, and the player opens a new one.

use s2_render::terrain::Point;
use s2_sim::Category;
use s2_sim::{
    BuildingKind, BuildingState, Command, MapObject, Node, Reserve, StockItem, StockSetting, World,
};
use s2_systems::job::Job;
use s2_systems::player::Player;
use s2_systems::ware::WareKind;

use crate::bar;
use crate::geometry::Rect;
use crate::icons;
use crate::paint::{self, Align, Painter, Sheet};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::stock::{PEOPLE_ORDER, StockRow, StockView, StockWindow};
use crate::windows::{BUILDING_NAMES, HELP_BUTTON, Outcome};

/// Which page a building's window shows, picked from the building when it opens (`+0x8f`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Any building not finished.
    Site,
    /// A finished building of category 1.
    Workplace,
    /// A finished storehouse, harbour or headquarters.
    Storehouse,
    /// A finished military building.
    Military,
}

impl Page {
    /// The size the window opens at.
    #[must_use]
    pub const fn size(self) -> (u32, u32) {
        match self {
            Self::Storehouse => (167, 416),
            _ => (226, 194),
        }
    }
}

/// Which of its pages a storehouse's window shows (the panel's `+0x18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shelf {
    /// The wares, in ware order.
    #[default]
    Wares,
    /// The people, in the stock screen's order.
    People,
    /// The headquarters' soldier reserve.
    Reserve,
}

impl Shelf {
    /// The page the next page button turns to: the headquarters has three, the others two.
    #[must_use]
    pub const fn next(self, headquarters: bool) -> Self {
        match (self, headquarters) {
            (Self::Wares, _) => Self::People,
            (Self::People, true) => Self::Reserve,
            _ => Self::Wares,
        }
    }
}

/// One input a workplace or military building stocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    /// The ware.
    pub ware: WareKind,
    /// How many the building stocks.
    pub capacity: u8,
    /// How many it holds.
    pub held: u8,
}

/// What a building's window shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingView {
    /// Where the building stands.
    pub node: Node,
    /// Its record.
    pub record: u16,
    /// Its type.
    pub kind: BuildingKind,
    /// Its graphics group, which names its picture in the nation archive.
    pub group: u16,
    /// The page it takes.
    pub page: Page,
    /// Its state flags: stopped (`0x400`) and manned (`0x10`) are the ones the window reads.
    pub state: BuildingState,
    /// A shipyard building ships.
    pub ships: bool,
    /// Its productivity, 0 to 100.
    pub productivity: u8,
    /// The worker its type has.
    pub worker: Option<Job>,
    /// The ware its type makes.
    pub output: Option<WareKind>,
    /// Its inputs, up to the first its type leaves empty.
    pub inputs: Vec<Input>,
    /// Whether its type offers the production switch and the bar.
    pub stoppable: bool,
    /// How many soldiers its type holds.
    pub capacity: u8,
    /// The soldiers inside, lowest rank first.
    pub soldiers: Vec<Job>,
    /// A storehouse's stock and settings.
    pub stock: Option<StockView>,
    /// A headquarters' soldier reserve, by rank.
    pub reserve: [Reserve; 5],
    /// For a harbour, whether an expedition is being prepared (`+0xfe`).
    pub preparing: Option<bool>,
}

impl BuildingView {
    /// What the window of `player`'s building on `node` shows, or nothing when no building of
    /// the player's stands there.
    #[must_use]
    pub fn read(world: &World, player: Player, node: Node) -> Option<Self> {
        let MapObject::Building(record) = world.object(node) else {
            return None;
        };
        let building = world.building(record)?;
        if building.owner != player {
            return None;
        }
        let row = world.building_types().get(building.kind)?;
        let state = building.state;
        let page = if state.contains(BuildingState::FINISHED) {
            match row.category {
                Category::Workplace => Page::Workplace,
                Category::Storehouse => Page::Storehouse,
                Category::Military => Page::Military,
            }
        } else {
            Page::Site
        };
        let inputs = row
            .inputs
            .iter()
            .zip(row.capacity)
            .zip(building.stock)
            .map_while(|((ware, capacity), held)| ware.map(|ware| Input { ware, capacity, held }))
            .collect();
        let soldiers = world
            .military_of(record)
            .map(|(_, military)| {
                Job::RANKS
                    .iter()
                    .zip(military.ranks)
                    .flat_map(|(&rank, count)| std::iter::repeat_n(rank, usize::from(count)))
                    .take(usize::from(military.inside))
                    .collect()
            })
            .unwrap_or_default();
        let stock = (page == Page::Storehouse)
            .then(|| StockView::read(world, player, node).ok())
            .flatten();
        let preparing = (page == Page::Storehouse && building.kind == BuildingKind::HARBOUR).then(|| {
            world
                .harbour_of(record)
                .and_then(|harbour| world.harbour(harbour))
                .is_some_and(|harbour| harbour.expedition)
        });
        Some(Self {
            node,
            record,
            kind: building.kind,
            group: row.group,
            page,
            state,
            ships: building.ships,
            productivity: building.productivity,
            worker: row.worker,
            output: row.output,
            inputs,
            stoppable: row.stoppable,
            capacity: row.soldiers,
            soldiers,
            stock,
            reserve: building.reserve,
            preparing,
        })
    }

    /// Whether its production, or its coins, are stopped.
    #[must_use]
    pub const fn stopped(&self) -> bool {
        self.state.contains(BuildingState::STOPPED)
    }

    /// Whether the building is a headquarters.
    #[must_use]
    pub fn headquarters(&self) -> bool {
        self.kind == BuildingKind::HEADQUARTERS
    }
}

/// The window style every control here is built in.
const STYLE: u16 = 0;

/// The palette index the panel's text draws in, the pen's starting value (`0x50868`).
const TEXT: u8 = 1;

/// The index a storehouse's count of 0 draws in (`0x5786a`).
const NONE_LEFT: u8 = 136;

/// The font the panel draws in, font index 0 (`0x50310`).
const FONT: FontId = FontId::Medium;

/// Where the names of the people start in the names table.
const PERSON_NAMES: u16 = 120;

/// Text slots the window shows or reports.
mod slot {
    pub const GO_TO: u16 = 219;
    pub const DEMOLISH: u16 = 503;
    pub const NEXT_PAGE: u16 = 502;
    pub const SITE: u16 = 505;
    pub const SHIPS: u16 = 519;
    pub const EXPEDITION: u16 = 535;
    pub const UNOCCUPIED: u16 = 544;
    pub const PRODUCTION: u16 = 545;
    pub const TAKE_OUT: u16 = 547;
    pub const STOP_STORAGE: u16 = 548;
    pub const SELECT_ALL: u16 = 553;
    pub const COINS: u16 = 273;
    pub const RESERVE: u16 = 272;
}

/// The control slots of the window, below the cells.
mod widget {
    pub const GO_TO: u16 = 40;
    pub const HELP: u16 = 41;
    pub const DEMOLISH: u16 = 42;
    pub const SHIPS: u16 = 43;
    pub const SWITCH: u16 = 44;
    pub const NEXT_PAGE: u16 = 45;
    pub const TAKE_OUT: u16 = 46;
    pub const STOP_STORAGE: u16 = 47;
    pub const SELECT_ALL: u16 = 48;
    pub const EXPEDITION: u16 = 49;
    pub const FEWER: u16 = 50;
    pub const MORE: u16 = 55;
}

/// The panel's corner in the window (`0x55d34`).
const PANEL: (i32, i32) = (11, 22);

/// How much narrower than the window the panel is.
const PANEL_NARROWER: u32 = 10;

/// How far up from the window's bottom edge the row of buttons along the foot sits.
const FOOT_UP: i32 = 47;

/// How far right of the demolish button's right edge the first page button goes.
const AFTER_DEMOLISH: i32 = 6;

/// Where the productivity bar sits (`0x562db`).
const BAR: (i32, i32, u32, u32) = (59, 31, 106, 16);

/// How far above the panel's bottom the building's origin goes, per page (`0x5702a`, `0x5709e`,
/// `0x578a0`).
const fn picture_up(page: Page) -> i32 {
    match page {
        Page::Site => 85,
        Page::Workplace => 105,
        Page::Storehouse | Page::Military => 90,
    }
}

/// The nation archive's first building item; group `g` stands at `250 + 5·g` with its shadow
/// after it.
const FIRST_BUILDING_ITEM: u16 = 250;

/// How many nation items each graphics group takes.
const ITEMS_PER_GROUP: u16 = 5;

/// The x the input and soldier rows centre on.
const ROW_MIDDLE: i32 = 113;

/// Where an input row's icons start down the panel, and how far apart the rows are.
const INPUT_TOP: i32 = 50;
const INPUT_STEP: i32 = 28;

/// How far left of and above an icon its darkened box starts, and how tall the box is.
const BOX_LEFT: i32 = 12;
const BOX_UP: i32 = 12;
const BOX_TALL: u32 = 24;

/// How many times each box is darkened.
const DARKENINGS: usize = 3;

/// How far apart a workplace's input icons sit, and a military building's.
const WORK_STEP: i32 = 24;
const MILITARY_STEP: i32 = 22;

/// Where the worker's icon, and the sword, sit.
const LEFT_ICON: (i32, i32) = (17, 17);

/// How far in from the panel's right edge the output's icon, and the shield, sit.
const RIGHT_ICON: i32 = 31;

/// How far down the "(House unoccupied)" line and the site's line sit.
const UNOCCUPIED_ROW: i32 = 28;
const SITE_ROW: i32 = 22;

/// Where the soldiers' icons sit down the panel.
const SOLDIER_ROW: i32 = 88;

/// How far up from the window's bottom edge the row of storehouse buttons sits.
const STORE_ROW_UP: i32 = 81;

/// Where the storehouse's help line sits and how tall it is.
const HELP_LINE: (i32, i32, u32) = (12, 305, 24);

/// Where a storehouse cell's count goes from the icon: its right edge and its top.
const COUNT_AT: (i32, i32) = (13, 15);

/// Where the stop and send out marks go from the icon.
const STOP_MARK: (i32, i32) = (8, -7);
const SEND_MARK: (i32, i32) = (8, 7);

/// The box a press on a cell lands in, from the icon's point: 15 left and 13 up, 27 by 39
/// (`0x565bf`).
const CELL_HIT: (i32, i32, u32, u32) = (-15, -13, 27, 39);

/// The box a cell's help covers, from the icon's point: the cell at `0x61692` plus (15, 21).
const CELL_HELP: (i32, i32, u32, u32) = (-11, -14, 28, 40);

/// The reserve page: the "Reserve" line, the counts' right edge and rows, the rank icon, and
/// the two buttons in the window.
const RESERVE_ROW: i32 = 65;
const RESERVE_COUNT: (i32, i32) = (98, 92);
const RESERVE_ICON: (i32, i32) = (22, 98);
const RESERVE_STEP: i32 = 30;
const RESERVE_BUTTONS: (i32, i32, i32, u32) = (52, 116, 109, 24);

/// The most of one rank the reserve keeps back.
const MOST_RESERVED: u8 = 15;

/// The pictures a building's window shows on its switch and its shipyard button. Only the body
/// and each control's own press set them, so a change made elsewhere leaves the old picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// The switch shows stopped.
    pub stopped: bool,
    /// The shipyard button shows ships.
    pub ships: bool,
}

/// One building's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingWindow {
    /// Which window it opens as.
    pub id: WindowId,
    /// Where the building stands.
    pub node: Node,
    /// The building's record.
    pub record: u16,
    /// The building's type.
    pub kind: BuildingKind,
    /// The page the window was built with.
    pub page: Page,
    /// The page a storehouse's window shows.
    pub shelf: Shelf,
    /// The set a storehouse's cells and "select all" edit (`+0x97`): send out, then stop.
    pub editing: StockSetting,
    /// The value "select all" last filled a set with (`+0x99`).
    pub all: bool,
    /// What the switch and the shipyard button show.
    pub shown: Shown,
    /// Whether a campaign is in play, which gives the shipyard its button (`0x1f78ea`).
    pub campaign: bool,
}

impl BuildingWindow {
    /// The window at `id` for `view`, as the body builds it.
    #[must_use]
    pub const fn new(id: WindowId, view: &BuildingView, campaign: bool) -> Self {
        Self {
            id,
            node: view.node,
            record: view.record,
            kind: view.kind,
            page: view.page,
            shelf: Shelf::Wares,
            editing: StockSetting::SEND,
            all: false,
            shown: Shown {
                stopped: view.stopped(),
                ships: view.ships,
            },
            campaign,
        }
    }

    /// The title: the building type's name (`0x10032` plus the type).
    #[must_use]
    pub fn title(&self) -> Label {
        Label::Named(BUILDING_NAMES + u16::from(self.kind.0))
    }

    /// Whether `view` is still the building this window was built for. A finished site, a
    /// building gone or one that changed hands closes it.
    #[must_use]
    pub fn current(&self, view: Option<&BuildingView>) -> bool {
        view.is_some_and(|view| view.record == self.record && view.page == self.page)
    }

    /// Where the help line sits in a window of `area`, which only a storehouse's carries.
    #[must_use]
    pub fn help_strip(&self, area: Rect) -> Option<Rect> {
        let (x, y, tall) = HELP_LINE;
        (self.page == Page::Storehouse).then(|| {
            Rect::new(
                area.x + x,
                area.y + y,
                area.width
                    .saturating_sub(2 * u32::try_from(x).unwrap_or_default()),
                tall,
            )
        })
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a press or a release
    /// asks for. A rolled-up window draws its frame alone.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &BuildingView,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        if window.rolled() {
            return None;
        }
        let area = window.area;
        painter.push_clip(area);
        let acts = ui.may_command();
        let mut run = Run {
            id: self.id,
            acts,
            ui,
            painter,
            skin,
            mouse,
            area,
            outcome: None,
        };
        self.panel(&mut run, view);
        self.foot(&mut run, view);
        match self.page {
            Page::Workplace => self.workplace(&mut run, view),
            Page::Military => self.military(&mut run, view),
            Page::Storehouse => self.storehouse(&mut run, view),
            Page::Site => {}
        }
        let outcome = run.outcome;
        painter.pop_clip();
        outcome
    }

    /// Draws what the panel draws (`0x56f48`), clipped to the panel as its drawing context
    /// clips it (`0x50310`).
    fn panel(&mut self, run: &mut Run<'_>, view: &BuildingView) {
        let panel = panel(run.area);
        run.painter.push_clip(panel);
        if self.page != Page::Storehouse {
            let body = FIRST_BUILDING_ITEM + ITEMS_PER_GROUP * view.group;
            let half = i32::try_from(panel.width / 2).unwrap_or_default();
            let tall = i32::try_from(panel.height).unwrap_or_default();
            let origin = Point::new(panel.x + half, panel.y + tall - picture_up(self.page));
            run.painter.shadow(Sheet::Nation, body + 1, origin);
            run.painter.game(Sheet::Nation, body, origin);
        }
        match self.page {
            Page::Site => run.line(panel, SITE_ROW, Label::Slot(slot::SITE)),
            Page::Workplace => draw_workplace(run, panel, view),
            Page::Military => draw_military(run, panel, view),
            Page::Storehouse => self.draw_storehouse(run, panel, view),
        }
        run.painter.pop_clip();
    }

    /// The go to, information and demolish buttons every page carries.
    fn foot(&self, run: &mut Run<'_>, view: &BuildingView) {
        let (left, foot) = (run.area.x, run.area.bottom() - FOOT_UP);
        let go_to = Rect::new(run.area.right() - FOOT_UP, foot, 30, 32);
        if run.icon(widget::GO_TO, go_to, Picture::GO_TO, slot::GO_TO, true) {
            run.answer(Outcome::Centre(view.node));
        }
        let help = Rect::new(left + 16, foot, 30, 32);
        if run.icon(widget::HELP, help, Picture::HELP, HELP_BUTTON, true) {
            run.answer(Outcome::Help(u16::from(view.kind.0)));
        }
        if !view.headquarters() {
            let demolish = Rect::new(left + 50, foot, 34, 32);
            if run.icon(
                widget::DEMOLISH,
                demolish,
                Picture::DEMOLISH,
                slot::DEMOLISH,
                true,
            ) {
                run.answer(Outcome::Demolish(view.node, self.id));
            }
        }
    }

    /// A workplace's shipyard button, production switch and bar (`0x56155`).
    fn workplace(&mut self, run: &mut Run<'_>, view: &BuildingView) {
        let foot = run.area.bottom() - FOOT_UP;
        let mut x = run.area.x + first_after_demolish();
        if view.kind == BuildingKind::SHIPYARD && self.campaign {
            let rect = Rect::new(x, foot, 48, 32);
            let picture = if self.shown.ships {
                Picture::SHIPS
            } else {
                Picture::BOATS
            };
            if run.icon(widget::SHIPS, rect, picture, slot::SHIPS, true) {
                self.shown.ships = !view.ships;
                run.answer(Outcome::Command(Command::ShipyardMode {
                    node: view.node,
                    ships: !view.ships,
                }));
            }
            x = rect.right() + AFTER_DEMOLISH;
        }
        if view.stoppable {
            let picture = if self.shown.stopped {
                Picture::PRODUCTION_STOPPED
            } else {
                Picture::PRODUCING
            };
            self.switch(run, view, Rect::new(x, foot, 32, 32), picture, slot::PRODUCTION);
            let (bx, by, bw, bh) = BAR;
            let rect = Rect::new(run.area.x + bx, run.area.y + by, bw, bh);
            bar::draw(run.painter, run.skin, rect, u32::from(view.productivity), true);
        }
    }

    /// A military building's coin switch (`0x5633c`).
    fn military(&mut self, run: &mut Run<'_>, view: &BuildingView) {
        let foot = run.area.bottom() - FOOT_UP;
        let rect = Rect::new(run.area.x + first_after_demolish(), foot, 32, 32);
        let picture = if self.shown.stopped {
            Picture::COINS_STOPPED
        } else {
            Picture::COINS_COMING
        };
        self.switch(run, view, rect, picture, slot::COINS);
    }

    /// The switch both pages share (`0x568a4`): it flips the stop and shows the new state.
    fn switch(&mut self, run: &mut Run<'_>, view: &BuildingView, rect: Rect, picture: Picture, help: u16) {
        if run.icon(widget::SWITCH, rect, picture, help, true) {
            self.shown.stopped = !view.stopped();
            run.answer(Outcome::Command(Command::StopProduction {
                node: view.node,
                stop: !view.stopped(),
            }));
        }
    }

    /// A storehouse's buttons, its cells and its reserve page (`0x55ed9`, `0x56467`).
    fn storehouse(&mut self, run: &mut Run<'_>, view: &BuildingView) {
        let area = run.area;
        let foot = area.bottom() - FOOT_UP;
        let next = if view.headquarters() {
            Rect::new(area.x + 50, foot, 66, 32)
        } else {
            Rect::new(area.x + 84, foot, 34, 32)
        };
        if run.icon(widget::NEXT_PAGE, next, Picture::TURN_PAGE, slot::NEXT_PAGE, true) {
            self.shelf = self.shelf.next(view.headquarters());
        }
        let harbour = view.preparing.is_some();
        let across = i32::try_from(area.width).unwrap_or_default() - 32;
        let step = across / if harbour { 4 } else { 3 };
        let width = u32::try_from(step - 2).unwrap_or_default();
        let row = area.bottom() - STORE_ROW_UP;
        let place = |index: i32| Rect::new(area.x + 16 + index * (step + 2), row, width, 32);
        let stocked = self.shelf != Shelf::Reserve;
        for (index, (slot_id, picture, help, setting)) in [
            (
                widget::TAKE_OUT,
                Picture::TAKE_OUT,
                slot::TAKE_OUT,
                StockSetting::SEND,
            ),
            (
                widget::STOP_STORAGE,
                Picture::STOP_STORAGE,
                slot::STOP_STORAGE,
                StockSetting::STOP,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = place(i32::try_from(index).unwrap_or_default());
            let marked = self.editing == setting;
            if run.marked_icon(slot_id, rect, picture, help, (stocked, marked)) {
                self.editing = setting;
            }
        }
        if run.icon(
            widget::SELECT_ALL,
            place(2),
            Picture::SELECT_ALL,
            slot::SELECT_ALL,
            stocked,
        ) {
            self.all = !self.all;
            run.answer(Outcome::Command(Command::SetAllStock {
                node: view.node,
                people: self.shelf == Shelf::People,
                setting: self.editing,
                on: self.all,
            }));
        }
        if let Some(preparing) = view.preparing
            && run.icon(
                widget::EXPEDITION,
                place(3),
                Picture::EXPEDITION,
                slot::EXPEDITION,
                !preparing,
            )
        {
            run.answer(Outcome::Command(Command::PrepareExpedition { node: view.node }));
        }
        match self.shelf {
            Shelf::Wares | Shelf::People => self.cells(run, view),
            Shelf::Reserve => Self::reserve(run, view),
        }
    }

    /// The cells of the wares or the people page: help over each, and a press flips the set
    /// being edited for the first cell it lands in. The press acts as the button goes down.
    fn cells(&self, run: &mut Run<'_>, view: &BuildingView) {
        let Some(stock) = &view.stock else {
            return;
        };
        let origin = Point::new(run.area.x, run.area.y);
        for (place, item) in self.items() {
            let icon = StockWindow::cell(place, origin);
            let help = offset(icon, CELL_HELP);
            run.ui.note_help(help, run.mouse, names(item));
            let hit = offset(icon, CELL_HIT);
            if run.acts
                && run.mouse.pressed
                && hit.contains(run.mouse.at.x, run.mouse.at.y)
                && run.outcome.is_none()
            {
                let on = stock.row(item).setting.contains(self.editing);
                run.answer(Outcome::Command(Command::SetStock {
                    node: view.node,
                    item,
                    setting: self.editing,
                    on: !on,
                }));
            }
        }
    }

    /// The headquarters' reserve page: a minus and a plus button per rank (`0x56caa`).
    fn reserve(run: &mut Run<'_>, view: &BuildingView) {
        let (fewer, more, top, size) = RESERVE_BUTTONS;
        for (index, (&rank, reserve)) in Job::RANKS.iter().zip(view.reserve).enumerate() {
            let row = i32::try_from(index).unwrap_or_default();
            let y = run.area.y + top + RESERVE_STEP * row;
            let help = Label::Named(PERSON_NAMES + u16::from(rank.0));
            let offset_slot = u16::try_from(index).unwrap_or_default();
            let down = Rect::new(run.area.x + fewer, y, size, size);
            if run.worded(widget::FEWER + offset_slot, down, b"-", help) {
                run.answer(Outcome::Command(Command::SetReserve {
                    node: view.node,
                    rank,
                    wanted: reserve.wanted.saturating_sub(1),
                }));
            }
            let up = Rect::new(run.area.x + more, y, size, size);
            if run.worded(widget::MORE + offset_slot, up, b"+", help) {
                run.answer(Outcome::Command(Command::SetReserve {
                    node: view.node,
                    rank,
                    wanted: (reserve.wanted + 1).min(MOST_RESERVED),
                }));
            }
        }
    }

    /// The places and the wares or jobs of the page shown.
    fn items(&self) -> Vec<(u32, StockItem)> {
        match self.shelf {
            Shelf::Wares => (0..WARE_KINDS)
                .map(|ware| (u32::from(ware), StockItem::Ware(WareKind(ware))))
                .collect(),
            Shelf::People => PEOPLE_ORDER
                .iter()
                .zip(0..)
                .map(|(&job, place)| (place, StockItem::Job(Job(job))))
                .collect(),
            Shelf::Reserve => Vec::new(),
        }
    }

    /// Draws a storehouse's page (`0x57805`).
    fn draw_storehouse(&self, run: &mut Run<'_>, panel: Rect, view: &BuildingView) {
        match self.shelf {
            Shelf::Wares | Shelf::People => {
                let Some(stock) = &view.stock else {
                    return;
                };
                let origin = Point::new(run.area.x, run.area.y);
                for (place, item) in self.items() {
                    let at = StockWindow::cell(place, origin);
                    draw_cell(run, Point::new(at.x, at.y), item, stock.row(item));
                }
            }
            Shelf::Reserve => {
                for (index, (&rank, reserve)) in Job::RANKS.iter().zip(view.reserve).enumerate() {
                    let row = RESERVE_STEP * i32::try_from(index).unwrap_or_default();
                    run.line(panel, RESERVE_ROW, Label::Slot(slot::RESERVE));
                    run.painter.text_aligned(
                        Point::new(panel.x + RESERVE_COUNT.0, panel.y + RESERVE_COUNT.1 + row),
                        FONT,
                        TEXT,
                        Label::Pair(i32::from(reserve.held), i32::from(reserve.wanted)),
                        Pen::TIGHT,
                        Align::Right,
                    );
                    let icon = Point::new(panel.x + RESERVE_ICON.0, panel.y + RESERVE_ICON.1 + row);
                    run.painter.game(Sheet::Landscape, icons::CELL_TOP, icon);
                    if let Some(picture) = icons::person(rank) {
                        run.painter.game(Sheet::Landscape, picture, icon);
                    }
                }
            }
        }
    }
}

/// How many wares the stock pages list.
const WARE_KINDS: u8 = 31;

/// The x of the first button after the demolish button.
const fn first_after_demolish() -> i32 {
    50 + 34 + AFTER_DEMOLISH
}

/// The panel's rectangle in a window of `area`.
fn panel(area: Rect) -> Rect {
    Rect::new(
        area.x + PANEL.0,
        area.y + PANEL.1,
        area.width.saturating_sub(PANEL_NARROWER),
        area.height,
    )
}

/// `rect` laid out from `at` by `(dx, dy, width, height)`.
const fn offset(at: Rect, (dx, dy, width, height): (i32, i32, u32, u32)) -> Rect {
    Rect::new(at.x + dx, at.y + dy, width, height)
}

/// The line of help a cell reports, which is the thing's name in the names table.
fn names(item: StockItem) -> Label {
    match item {
        StockItem::Ware(ware) => Label::Named(u16::from(ware.0)),
        StockItem::Job(job) => Label::Named(PERSON_NAMES + u16::from(job.0)),
    }
}

/// The landscape item of a ware's icon.
fn ware_icon(ware: WareKind) -> Option<u16> {
    icons::ware(ware)
}

/// Draws one input row, or a mine's three on one line (`0x570c4`, `0x5797c`): every place's
/// icon, the box darkened over them, then the ones held again at full brightness.
fn draw_inputs(run: &mut Run<'_>, panel: Rect, inputs: &[Input], step: i32, mine: bool) {
    for (row, input) in inputs.iter().enumerate() {
        let row = i32::try_from(row).unwrap_or_default();
        let count = i32::from(input.capacity);
        let (start, top, first) = if mine {
            (ROW_MIDDLE - count * 3 * step / 2, INPUT_TOP, 2 * row)
        } else {
            (ROW_MIDDLE - count * step / 2, INPUT_TOP + INPUT_STEP * row, 0)
        };
        let icon = |place: i32| Point::new(panel.x + start + step * (first + place), panel.y + top);
        let Some(picture) = ware_icon(input.ware) else {
            continue;
        };
        for place in 0..count {
            run.painter.game(Sheet::Landscape, picture, icon(place));
        }
        let left = icon(0);
        let shade = Rect::new(
            left.x - BOX_LEFT,
            left.y - BOX_UP,
            u32::try_from(step * count).unwrap_or_default(),
            BOX_TALL,
        );
        for _ in 0..DARKENINGS {
            run.painter.darken(shade);
        }
        for place in 0..i32::from(input.held) {
            run.painter.game(Sheet::Landscape, picture, icon(place));
        }
    }
}

/// Draws a workplace's page (`0x57076`).
fn draw_workplace(run: &mut Run<'_>, panel: Rect, view: &BuildingView) {
    draw_inputs(run, panel, &view.inputs, WORK_STEP, view.kind.is_mine());
    if let Some(worker) = view.worker {
        run.boxed(panel, LEFT_ICON, icons::person(worker));
        if !view.state.contains(BuildingState::MANNED) {
            run.line(panel, UNOCCUPIED_ROW, Label::Slot(slot::UNOCCUPIED));
        }
    }
    let right = (right_icon(panel), LEFT_ICON.1);
    if view.kind == BuildingKind::DONKEY_BREEDER {
        run.boxed(panel, right, icons::person(Job::PACK_DONKEY));
    } else if let Some(output) = view.output {
        run.boxed(panel, right, ware_icon(output));
    }
}

/// Draws a military building's page (`0x57878`).
fn draw_military(run: &mut Run<'_>, panel: Rect, view: &BuildingView) {
    run.boxed(
        panel,
        (right_icon(panel), LEFT_ICON.1),
        ware_icon(WareKind::SHIELD),
    );
    run.boxed(panel, LEFT_ICON, ware_icon(WareKind::SWORD));
    draw_inputs(run, panel, &view.inputs, MILITARY_STEP, false);
    let capacity = i32::from(view.capacity);
    let start = ROW_MIDDLE - capacity * MILITARY_STEP / 2;
    let shade = Rect::new(
        panel.x + start - BOX_LEFT,
        panel.y + SOLDIER_ROW - BOX_UP,
        u32::try_from(capacity * MILITARY_STEP).unwrap_or_default(),
        BOX_TALL,
    );
    for _ in 0..DARKENINGS {
        run.painter.darken(shade);
    }
    for (place, &rank) in view.soldiers.iter().enumerate() {
        let place = i32::try_from(place).unwrap_or_default();
        if let Some(picture) = icons::person(rank) {
            let at = Point::new(panel.x + start + MILITARY_STEP * place, panel.y + SOLDIER_ROW);
            run.painter.game(Sheet::Landscape, picture, at);
        }
    }
}

/// The x of the icon at the panel's top right.
fn right_icon(panel: Rect) -> i32 {
    i32::try_from(panel.width).unwrap_or_default() - RIGHT_ICON
}

/// Draws one cell of a storehouse's page, with its icon's centre at `at`.
fn draw_cell(run: &mut Run<'_>, at: Point, item: StockItem, row: StockRow) {
    let painter = &mut *run.painter;
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
    for (setting, picture, (dx, dy)) in [
        (StockSetting::STOP, Picture::NOT_STORING, STOP_MARK),
        (StockSetting::SEND, Picture::SENDING_OUT, SEND_MARK),
    ] {
        if row.setting.contains(setting)
            && let Some(item) = run.skin.picture(picture)
        {
            painter.item(item, Point::new(at.x + dx, at.y + dy));
        }
    }
    painter.text_aligned(
        Point::new(at.x + COUNT_AT.0, at.y + COUNT_AT.1),
        FONT,
        if row.count == 0 { NONE_LEFT } else { TEXT },
        Label::Number(i32::from(row.count)),
        Pen::TIGHT,
        Align::Right,
    );
}

/// What one frame of the window works with.
struct Run<'a> {
    id: WindowId,
    acts: bool,
    ui: &'a mut Ui,
    painter: &'a mut Painter,
    skin: &'a Skin,
    mouse: Pointer,
    area: Rect,
    outcome: Option<Outcome>,
}

impl Run<'_> {
    /// Keeps `outcome` unless one is already kept.
    fn answer(&mut self, outcome: Outcome) {
        if self.outcome.is_none() {
            self.outcome = Some(outcome);
        }
    }

    /// Draws an icon button and says whether a release acted on it. One that is not `enabled`
    /// draws greyed and swallows the click (`+0x1d`, `0x4d8c0`); none acts while the game is
    /// paused.
    fn icon(&mut self, slot: u16, rect: Rect, picture: Picture, help: u16, enabled: bool) -> bool {
        self.marked_icon(slot, rect, picture, help, (enabled, false))
    }

    /// [`Self::icon`] for a button that can be marked, whose inside goes through the lightening
    /// table between its face and its picture (`0x51898`).
    fn marked_icon(
        &mut self,
        slot: u16,
        rect: Rect,
        picture: Picture,
        help: u16,
        (enabled, marked): (bool, bool),
    ) -> bool {
        let widget = WidgetId::new(self.id, slot);
        self.ui.note_help(rect, self.mouse, Label::Slot(help));
        let acted = self.ui.press(widget, rect, self.mouse, enabled && self.acts);
        let look = self.ui.button(widget).look(true, false);
        if let Some(item) = self.skin.face(STYLE, look) {
            self.painter
                .frame_edges(item, self.skin.edge(STYLE), rect, look.edges());
        }
        if marked {
            self.painter.lighten(rect.shrink(paint::EDGE));
        }
        if let Some(item) = self.skin.picture(picture) {
            self.painter.item_centred(item, rect);
        }
        if !enabled {
            self.painter.disable(rect);
        }
        acted
    }

    /// Draws a text button showing `text` and says whether a release acted on it.
    fn worded(&mut self, slot: u16, rect: Rect, text: &[u8], help: Label) -> bool {
        let widget = WidgetId::new(self.id, slot);
        self.ui.note_help(rect, self.mouse, help);
        let acted = self.ui.press(widget, rect, self.mouse, self.acts);
        let look = self.ui.button(widget).look(true, false);
        if let Some(item) = self.skin.face(STYLE, look) {
            self.painter
                .frame_edges(item, self.skin.edge(STYLE), rect, look.edges());
        }
        let label = self.painter.run(text);
        let style = self
            .painter
            .lettering(STYLE)
            .unwrap_or_else(|| self.painter.first_lettering());
        self.painter.text_aligned(
            Point::new(rect.x, rect.y),
            style.font,
            style.colour,
            label,
            Pen::TIGHT,
            Align::CentredInBox(rect.width, rect.height),
        );
        acted
    }

    /// Writes `label` centred across `panel`, `down` from its top (`0x50444`).
    fn line(&mut self, panel: Rect, down: i32, label: Label) {
        self.painter.text_aligned(
            Point::new(panel.x, panel.y + down),
            FONT,
            TEXT,
            label,
            Pen::TIGHT,
            Align::Across(panel.width),
        );
    }

    /// Draws the cell box with `picture` over it, both centred on `at` in `panel`.
    fn boxed(&mut self, panel: Rect, (x, y): (i32, i32), picture: Option<u16>) {
        let at = Point::new(panel.x + x, panel.y + y);
        self.painter.game(Sheet::Landscape, icons::CELL_TOP, at);
        if let Some(picture) = picture {
            self.painter.game(Sheet::Landscape, picture, at);
        }
    }
}

#[cfg(test)]
mod tests;
