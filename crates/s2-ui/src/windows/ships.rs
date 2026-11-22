//! The ship register: one ship at a time, the buttons that walk the player's list, and the
//! expedition buttons.

use s2_render::terrain::Point;
use s2_sim::{Command, Node, SECTORS, ShipMode, World};
use s2_systems::player::Player;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{self, Outcome, kind};

/// The help page the window's own button opens, which command 10938 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1004;

/// The text slot the button that moves the view to the ship reports.
const SHOW_HELP: u16 = 532;

/// The text slot the list shows with no ship in it (`0x6769d`).
const NO_SHIPS: u16 = 533;

/// The palette index that line draws in, which `0x508fc` leaves on the canvas.
const LIST_TEXT: u8 = 26;

/// Which way one of the four buttons walks the player's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walk {
    /// To the first ship.
    First,
    /// To the ship before.
    Back,
    /// To the ship after.
    On,
    /// To the last ship.
    Last,
}

impl Walk {
    /// The four, left to right.
    pub const ALL: [Self; 4] = [Self::First, Self::Back, Self::On, Self::Last];

    /// The text slot it reports as its line of help.
    #[must_use]
    pub const fn help(self) -> u16 {
        match self {
            Self::First => 528,
            Self::Back => 529,
            Self::On => 530,
            Self::Last => 531,
        }
    }

    /// The `IO.DAT` item that draws it.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::First => Picture::FIRST,
            Self::Back => Picture::PREVIOUS,
            Self::On => Picture::NEXT,
            Self::Last => Picture::LAST,
        }
    }

    /// Where it lands from `place` in a list of `count` ships.
    #[must_use]
    pub const fn landing(self, place: usize, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let last = count - 1;
        let wanted = match self {
            Self::First => 0,
            Self::Back if place == 0 => return None,
            Self::Back => place - 1,
            Self::On if place >= last => return None,
            Self::On => place + 1,
            Self::Last => last,
        };
        if wanted == place { None } else { Some(wanted) }
    }
}

/// Where a sector's button sits and what draws it, from the records at `0x1fd5f4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectorButton {
    /// Where it sits in the window.
    pub at: (i32, i32),
    /// The picture that draws it.
    pub picture: Picture,
}

/// The six sector buttons, in sector order clockwise from the north.
pub const SECTOR_BUTTONS: [SectorButton; SECTORS] = [
    SectorButton {
        at: (60, 61),
        picture: Picture::SECTOR_NORTH,
    },
    SectorButton {
        at: (80, 70),
        picture: Picture::SECTOR_NORTH_EAST,
    },
    SectorButton {
        at: (80, 90),
        picture: Picture::SECTOR_SOUTH_EAST,
    },
    SectorButton {
        at: (60, 100),
        picture: Picture::SECTOR_SOUTH,
    },
    SectorButton {
        at: (40, 90),
        picture: Picture::SECTOR_SOUTH_WEST,
    },
    SectorButton {
        at: (40, 70),
        picture: Picture::SECTOR_NORTH_WEST,
    },
];

/// What an expedition ship offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expedition {
    /// Whether the ship rests, which is the original's phase 3.
    pub resting: bool,
    /// The sectors with a harbour site.
    pub sectors: [bool; SECTORS],
    /// Whether the land the ship stopped at belongs to nobody.
    pub unclaimed: bool,
}

/// What the ship register shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipsView {
    /// The player's ships, in the order their numbers run.
    pub ships: Vec<u16>,
    /// Which of them the window shows.
    pub place: Option<usize>,
    /// Where that ship is.
    pub at: Option<Node>,
    /// Where the harbour it serves lands its boats, for a transport ship.
    pub harbour: Option<Node>,
    /// What it offers while it carries an expedition.
    pub expedition: Option<Expedition>,
}

impl ShipsView {
    /// `player`'s ships with `chosen` shown, falling back to the first ship.
    #[must_use]
    pub fn read(world: &World, player: Player, chosen: Option<u16>) -> Self {
        let mut ships: Vec<(u16, u16)> = world
            .ships()
            .filter(|(_, ship)| ship.owner == player)
            .map(|(record, ship)| (ship.number, record))
            .collect();
        ships.sort_unstable();
        let ships: Vec<u16> = ships.into_iter().map(|(_, record)| record).collect();
        let place = chosen
            .and_then(|record| ships.iter().position(|&other| other == record))
            .or(if ships.is_empty() { None } else { Some(0) });
        let ship = place
            .and_then(|place| ships.get(place))
            .and_then(|&record| world.ship(record));
        Self {
            ships,
            place,
            at: ship
                .and_then(|ship| world.figure(ship.figure))
                .map(s2_sim::Figure::node),
            harbour: ship.and_then(|ship| match ship.mode {
                ShipMode::Transport { harbour } | ShipMode::Anchoring { harbour } => {
                    harbour.and_then(|harbour| world.landing(harbour, ship.sea))
                }
                ShipMode::Expedition { .. } => None,
            }),
            expedition: ship.and_then(|ship| match ship.mode {
                ShipMode::Expedition { .. } => Some(Expedition {
                    resting: !ship.sailing,
                    sectors: ship.sectors,
                    unclaimed: ship.unclaimed,
                }),
                ShipMode::Transport { .. } | ShipMode::Anchoring { .. } => None,
            }),
        }
    }

    /// The record the window shows.
    #[must_use]
    pub fn chosen(&self) -> Option<u16> {
        self.place.and_then(|place| self.ships.get(place)).copied()
    }
}

/// The ship register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipsWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The ship it shows, which the four walking buttons move.
    pub chosen: Option<u16>,
}

impl ShipsWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 252;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 238;
    /// Where the list sits and how big it is.
    const LIST: (i32, i32, u32, u32) = (11, 21, 230, 162);
    /// How far down the row of buttons sits, and how big each one is.
    const ROW: (i32, u32, u32) = (194, 28, 24);
    /// Where the first walking button sits, and how far apart they are.
    const WALK_AT: (i32, i32) = (55, 28);
    /// Where the button that moves the view to the ship sits.
    const SHOW_AT: i32 = 171;
    /// Where the button the ship's mode offers sits.
    const MODE_AT: i32 = 199;
    /// Where the button that founds a colony sits, and how big it is.
    const FOUND: (i32, i32, u32) = (60, 81, 18);
    /// How big a sector button is.
    const SECTOR: u32 = 18;
    /// The help button: how far in from the left, how far up from the bottom, and its size.
    const HELP: (i32, i32, u32, u32) = (18, 48, 30, 32);

    /// The window at `id` with nothing chosen yet.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self { id, chosen: None }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the list sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn list(at: Point) -> Rect {
        let (x, y, width, height) = Self::LIST;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where the button that walks the list `how` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn walk(how: Walk, at: Point) -> Rect {
        let step = match how {
            Walk::First => 0,
            Walk::Back => 1,
            Walk::On => 2,
            Walk::Last => 3,
        };
        let (y, width, height) = Self::ROW;
        let (first, apart) = Self::WALK_AT;
        Rect::new(at.x + first + apart * step, at.y + y, width, height)
    }

    /// Where the button that moves the view to the ship sits.
    #[must_use]
    pub fn show(at: Point) -> Rect {
        let (y, width, height) = Self::ROW;
        Rect::new(at.x + Self::SHOW_AT, at.y + y, width, height)
    }

    /// Where the button the ship's mode offers sits.
    #[must_use]
    pub fn mode(at: Point) -> Rect {
        let (y, width, height) = Self::ROW;
        Rect::new(at.x + Self::MODE_AT, at.y + y, width, height)
    }

    /// Where the button that founds a colony sits.
    #[must_use]
    pub fn found(at: Point) -> Rect {
        let (x, y, size) = Self::FOUND;
        Rect::new(at.x + x, at.y + y, size, size)
    }

    /// Where sector `sector`'s button sits.
    #[must_use]
    pub fn sector(sector: usize, at: Point) -> Option<Rect> {
        SECTOR_BUTTONS
            .get(sector)
            .map(|button| Rect::new(at.x + button.at.0, at.y + button.at.1, Self::SECTOR, Self::SECTOR))
    }

    /// Where the button that opens the help page sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (left, up, width, height) = Self::HELP;
        Rect::new(area.x + left, area.bottom() - up, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &ShipsView,
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
        let list = Self::list(at);
        if let Some(item) = skin.picture(Picture::SCROLL) {
            painter.item_centred(item, list);
        }
        if view.ships.is_empty() {
            painter.text_aligned(
                Point::new(list.x, list.y),
                FontId::Medium,
                LIST_TEXT,
                Label::Slot(NO_SHIPS),
                Pen::TIGHT,
                Align::CentredInBox(list.width, list.height),
            );
        }
        let mut outcome = None;
        let mut walked = None;
        for how in Walk::ALL {
            let place = Self::walk(how, at);
            let landing = view.place.and_then(|place| how.landing(place, view.ships.len()));
            let widget = WidgetId::new(self.id, Self::walk_slot(how));
            ui.note_help(place, mouse, Label::Slot(how.help()));
            let acted = ui.press(widget, place, mouse, landing.is_some());
            let look = ui.button(widget).look(landing.is_some(), false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if let Some(item) = skin.picture(how.picture()) {
                painter.item_centred(item, place);
            }
            if acted && walked.is_none() {
                walked = landing.and_then(|landing| view.ships.get(landing)).copied();
            }
        }
        let show = Self::show(at);
        let widget = WidgetId::new(self.id, Self::SHOW_SLOT);
        ui.note_help(show, mouse, Label::Slot(SHOW_HELP));
        let acted = ui.press(widget, show, mouse, view.at.is_some());
        let look = ui.button(widget).look(view.at.is_some(), false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), show, look.edges());
        }
        if let Some(item) = skin.picture(Picture::GO_TO) {
            painter.item_centred(item, show);
        }
        if acted && let Some(node) = view.at {
            outcome = Some(Outcome::Centre(node));
        }
        outcome = self.mode_button(ui, painter, skin, view, mouse, at).or(outcome);
        outcome = self.expedition(ui, painter, skin, view, mouse, at).or(outcome);
        let help = Self::help(area);
        let widget = WidgetId::new(self.id, Self::HELP_SLOT);
        ui.note_help(help, mouse, Label::Slot(windows::HELP_BUTTON));
        let acted = ui.press(widget, help, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), help, look.edges());
        }
        if let Some(item) = skin.picture(Picture::HELP) {
            painter.item_centred(item, help);
        }
        if acted && outcome.is_none() {
            outcome = Some(Outcome::Help(HELP_PAGE));
        }
        if let Some(record) = walked {
            self.chosen = Some(record);
        }
        painter.pop_clip();
        outcome
    }

    /// The button the ship's mode offers: the harbour for a transport ship, calling the
    /// expedition off for an expedition ship.
    fn mode_button(
        self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &ShipsView,
        mouse: Pointer,
        at: Point,
    ) -> Option<Outcome> {
        let chosen = view.chosen()?;
        let place = Self::mode(at);
        let offered = view.expedition.is_some() || view.harbour.is_some();
        let widget = WidgetId::new(self.id, Self::MODE_SLOT);
        let acted = ui.press(widget, place, mouse, offered);
        if offered && let Some(item) = skin.item(Control::Button, ui.button(widget).look(true, false)) {
            painter.frame(item, skin.item(Control::Edge, Look::Idle), place);
        }
        if !acted {
            return None;
        }
        if view.expedition.is_some() {
            return Some(Outcome::Command(Command::CancelExpedition { ship: chosen }));
        }
        view.harbour.map(Outcome::Centre)
    }

    /// The buttons an expedition ship at rest offers: founding a colony and the sectors with a
    /// harbour site.
    fn expedition(
        self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &ShipsView,
        mouse: Pointer,
        at: Point,
    ) -> Option<Outcome> {
        let chosen = view.chosen()?;
        let expedition = view.expedition?;
        if !expedition.resting {
            return None;
        }
        let mut outcome = None;
        if expedition.unclaimed {
            let place = Self::found(at);
            let widget = WidgetId::new(self.id, Self::FOUND_SLOT);
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if acted {
                outcome = Some(Outcome::Command(Command::FoundHarbour { ship: chosen }));
            }
        }
        for (sector, &offered) in expedition.sectors.iter().enumerate() {
            if !offered {
                continue;
            }
            let Some(place) = Self::sector(sector, at) else {
                continue;
            };
            let widget = WidgetId::new(
                self.id,
                Self::SECTOR_SLOT + u16::try_from(sector).unwrap_or_default(),
            );
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if let Some(button) = SECTOR_BUTTONS.get(sector)
                && let Some(item) = skin.picture(button.picture)
            {
                painter.item_centred(item, place);
            }
            if acted && outcome.is_none() {
                outcome = Some(Outcome::Command(Command::SailExpedition {
                    ship: chosen,
                    sector: u8::try_from(sector).unwrap_or_default(),
                }));
            }
        }
        outcome
    }

    /// The widget slot the button that moves the view to the ship takes.
    const SHOW_SLOT: u16 = 4;
    /// The widget slot the button the mode offers takes.
    const MODE_SLOT: u16 = 5;
    /// The widget slot the button that founds a colony takes.
    const FOUND_SLOT: u16 = 6;
    /// The widget slot the help button takes.
    const HELP_SLOT: u16 = 7;
    /// The first of the six widget slots the sector buttons take.
    const SECTOR_SLOT: u16 = 8;

    /// The widget slot one of the four walking buttons takes.
    const fn walk_slot(how: Walk) -> u16 {
        match how {
            Walk::First => 0,
            Walk::Back => 1,
            Walk::On => 2,
            Walk::Last => 3,
        }
    }
}

/// The kind the ship register opens as.
pub const KIND: WindowId = kind::SHIPS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;

    const SHIPS: [u16; 3] = [4, 1, 9];
    const AT: Node = Node { x: 7, y: 5 };
    const LANDING: Node = Node { x: 3, y: 4 };

    fn player() -> Player {
        Player::new(0).expect("player 0")
    }

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Button, Look::Disabled, ItemId(3));
        skin.set(Control::Title, Look::Idle, ItemId(4));
        skin
    }

    /// A transport ship at `AT` serving a harbour that lands boats at `LANDING`.
    fn carrier(place: usize) -> ShipsView {
        ShipsView {
            ships: SHIPS.to_vec(),
            place: Some(place),
            at: Some(AT),
            harbour: Some(LANDING),
            expedition: None,
        }
    }

    /// An expedition ship at rest with a site in sector 2 on land that belongs to nobody.
    fn expedition() -> (ShipsView, [bool; SECTORS]) {
        let mut sectors = [false; SECTORS];
        sectors[2] = true;
        (
            ShipsView {
                ships: SHIPS.to_vec(),
                place: Some(0),
                at: Some(AT),
                harbour: None,
                expedition: Some(Expedition {
                    resting: true,
                    sectors,
                    unclaimed: true,
                }),
            },
            sectors,
        )
    }

    fn open() -> (ShipsWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = ShipsWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (
            ShipsWindow::new(KIND),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(
        window: &mut ShipsWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: &ShipsView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), view, on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), view, on.releasing())
    }

    #[test]
    fn the_parts_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(ShipsWindow::list(at), Rect::new(11, 21, 230, 162));
        let walks: Vec<_> = Walk::ALL
            .iter()
            .map(|&how| ShipsWindow::walk(how, at).x)
            .collect();
        assert_eq!(walks, [55, 83, 111, 139]);
        assert_eq!(ShipsWindow::walk(Walk::First, at), Rect::new(55, 194, 28, 24));
        assert_eq!(ShipsWindow::show(at), Rect::new(171, 194, 28, 24));
        assert_eq!(ShipsWindow::mode(at), Rect::new(199, 194, 28, 24));
        assert_eq!(ShipsWindow::found(at), Rect::new(60, 81, 18, 18));
        assert_eq!(ShipsWindow::sector(0, at), Some(Rect::new(60, 61, 18, 18)));
        assert_eq!(ShipsWindow::sector(SECTORS, at), None);
        let area = Rect::new(0, 0, ShipsWindow::WIDTH, ShipsWindow::HEIGHT);
        assert_eq!(ShipsWindow::help(area), Rect::new(18, 190, 30, 32));
        let width = i32::try_from(ShipsWindow::WIDTH).expect("fits");
        assert!(ShipsWindow::mode(at).right() <= width);
    }

    #[test]
    fn the_sector_buttons_ring_a_centre() {
        let at = Point::new(0, 0);
        let middles: Vec<_> = (0..SECTORS)
            .filter_map(|sector| ShipsWindow::sector(sector, at))
            .map(|place| (place.x + 9, place.y + 9))
            .collect();
        assert_eq!(
            middles,
            [(69, 70), (89, 79), (89, 99), (69, 109), (49, 99), (49, 79)]
        );
    }

    #[test]
    fn a_world_with_no_ships_leaves_the_window_empty() {
        let view = ShipsView::read(&World::empty(32, 32), player(), None);
        assert!(view.ships.is_empty(), "{:?}", view.ships);
        assert_eq!(view.place, None);
        assert_eq!(view.chosen(), None);
        assert_eq!(view.at, None);
        assert_eq!(view.expedition, None);
    }

    #[test]
    fn a_walking_button_stops_at_the_ends() {
        assert_eq!(Walk::First.landing(0, 3), None);
        assert_eq!(Walk::Back.landing(0, 3), None);
        assert_eq!(Walk::On.landing(2, 3), None);
        assert_eq!(Walk::Last.landing(2, 3), None);
        assert_eq!(Walk::On.landing(0, 3), Some(1));
        assert_eq!(Walk::Last.landing(0, 3), Some(2));
        assert_eq!(Walk::First.landing(2, 3), Some(0));
        assert_eq!(Walk::Back.landing(2, 3), Some(1));
        assert_eq!(Walk::On.landing(0, 0), None);
        assert_eq!(Walk::First.landing(0, 1), None);
    }

    #[test]
    fn a_walking_button_moves_the_choice() {
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        let view = carrier(0);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &view,
                ShipsWindow::walk(Walk::On, at)
            ),
            None,
            "walking the list is the window's own business"
        );
        assert_eq!(window.chosen, Some(SHIPS[1]));
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &carrier(2),
                ShipsWindow::walk(Walk::First, at)
            ),
            None
        );
        assert_eq!(window.chosen, Some(SHIPS[0]));
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &carrier(0),
                ShipsWindow::walk(Walk::Back, at)
            ),
            None,
            "the first ship has nothing before it"
        );
        assert_eq!(window.chosen, Some(SHIPS[0]));
    }

    #[test]
    fn the_two_buttons_that_move_the_view_carry_their_nodes() {
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        let view = carrier(0);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, ShipsWindow::show(at)),
            Some(Outcome::Centre(AT))
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, ShipsWindow::mode(at)),
            Some(Outcome::Centre(LANDING))
        );
    }

    #[test]
    fn an_expedition_offers_its_sectors_and_a_colony() {
        let (view, sectors) = expedition();
        let ship = SHIPS[0];
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &view,
                ShipsWindow::sector(2, at).expect("a sector")
            ),
            Some(Outcome::Command(Command::SailExpedition { ship, sector: 2 }))
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, ShipsWindow::found(at)),
            Some(Outcome::Command(Command::FoundHarbour { ship }))
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, ShipsWindow::mode(at)),
            Some(Outcome::Command(Command::CancelExpedition { ship }))
        );
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &view,
                ShipsWindow::sector(0, at).expect("a sector")
            ),
            None,
            "a sector with no site takes no press"
        );
        assert!(sectors[2]);
    }

    #[test]
    fn a_sailing_expedition_offers_nothing_but_calling_it_off() {
        let (mut view, _) = expedition();
        if let Some(expedition) = view.expedition.as_mut() {
            expedition.resting = false;
        }
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &view,
                ShipsWindow::sector(2, at).expect("a sector")
            ),
            None
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, ShipsWindow::found(at)),
            None
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, ShipsWindow::mode(at)),
            Some(Outcome::Command(Command::CancelExpedition { ship: SHIPS[0] }))
        );
    }

    #[test]
    fn the_help_button_opens_the_page() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, ShipsWindow::WIDTH, ShipsWindow::HEIGHT);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                &carrier(0),
                ShipsWindow::help(area)
            ),
            Some(Outcome::Help(HELP_PAGE))
        );
    }

    #[test]
    fn a_rolled_up_window_draws_nothing_and_takes_no_press() {
        let (mut window, mut ui, mut painter) = open();
        ui.roll(KIND);
        let place = ShipsWindow::show(Point::new(0, 0));
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &carrier(0), place),
            None
        );
        assert!(
            painter.ops().is_empty(),
            "a rolled window draws nothing of its own"
        );
    }
}
