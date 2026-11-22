//! The action menu a click on a node opens.
//!
//! The 28 records of the original's table carry a picture, a command, a text slot, the next
//! candidate in their row and what pressing them opens. A row keeps the first five candidates of
//! its chain whose test passes, so what the menu offers depends on what stands on the node.
//!
//! The third row holds the building types of one size, which [`types`] gathers from the game
//! rather than from the table. What draws those buttons in the original is the item named
//! `ho<nation><group>`, which no shipped index holds, so they carry no picture here yet.

use s2_render::terrain::Point;
use s2_sim::{BuildingKind, BuildingTypes, Direction, Game, Node, Player, SiteSize};

use crate::geometry::Rect;
use crate::paint::{EDGE, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{BUILDING_NAMES, Outcome, kind};

/// How many buttons a row shows.
pub const PLACES: usize = 5;

/// How many rows the menu builds.
pub const ROWS: usize = 3;

/// The record the first row starts from.
const FIRST: u8 = 0;

/// The record it starts from instead while a road is being drawn.
const DRAWING: u8 = 19;

/// Where the rows sit: x, the three ys, and how wide a box is.
const BOX: (i32, [i32; ROWS], u32) = (11, [21, 65, 109], 36);

/// How tall a tab's face is and how tall the box it sits in is. `0x5b386` gives a row holding a
/// tab boxes 44 tall, and `0x5bffb` draws the strip the row below runs on from over the
/// bottom 12 of every tab but the chosen one.
const TAB: (u32, u32) = (32, 44);

/// The black the strip under a row of tabs draws its own right and bottom in, which is the
/// palette's first index and not the one a control's frame lays under itself.
const STRIP_BLACK: u8 = 0;

/// The pieces of the strip under one tab or under the block that fills a row out: two rows of
/// edge, nine of face, and a row of black, with the two columns at either side that the first
/// and last places draw over the face.
#[derive(Debug, Clone, Copy)]
struct Strip {
    top: Rect,
    face: Rect,
    bottom: Rect,
    left: Rect,
    right: Rect,
}

impl Strip {
    /// The pieces of the strip that covers `foot`, the bottom 12 rows of a box.
    fn of(foot: Rect) -> Self {
        let thick = i32::try_from(EDGE).unwrap_or_default();
        let face = Rect::new(
            foot.x,
            foot.y + thick,
            foot.width,
            foot.height.saturating_sub(EDGE + 1),
        );
        Self {
            top: foot.with_height(EDGE),
            face,
            bottom: Rect::new(foot.x, foot.bottom() - 1, foot.width, 1),
            left: Rect::new(foot.x, foot.y, EDGE, foot.height.saturating_sub(1)),
            right: Rect::new(foot.right() - thick, face.y, EDGE, face.height),
        }
    }
}

/// How wide the boxes of a second row with no tab in it share between them (`0x5b5e5`).
const SHARED: u32 = 180;

/// The row whose boxes can share [`SHARED`]: the second, which `0x5b4c2` builds. The first row's
/// boxes are always square (`0x5b29b`).
const SHARING_ROW: usize = 1;

/// What a record's test asks of the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// Nothing: the button is always offered.
    Always,
    /// A headquarters may be placed, which only the cheat word allows.
    Headquarters,
    /// A building of some size fits.
    Building,
    /// A building of one size fits.
    Fits(SiteSize),
    /// A flag fits, and the node has a water edge or has not.
    FlagFits(bool),
    /// A flag fits either way.
    Flag,
    /// A building other than the headquarters stands here.
    BuildingHere,
    /// A road runs through the node.
    RoadHere,
    /// A flag stands here, and the node has a water edge or has not.
    FlagHere(bool),
    /// A flag stands here that may be pulled down: any flag but the headquarters' own.
    FlagDown,
    /// A flag stands here that a geologist or a scout may leave from.
    FlagSends,
    /// Soldiers could be sent against the building here.
    Attack,
}

/// One of the 28 records at `0x1fc8b4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    /// The picture the button draws.
    pub picture: Picture,
    /// The command it issues.
    pub command: u16,
    /// The text slot of its label.
    pub slot: u16,
    /// The next candidate in this row.
    pub next: Option<u8>,
    /// The first record of the row it opens.
    pub opens: Option<u8>,
    /// The size of building whose types it lists, which is what its routine builds.
    pub lists: Option<SiteSize>,
    /// What the node must offer for it to show.
    pub shown: Shown,
}

/// The window style a button that opens no row draws in, which is the grey of the window's own
/// background. Every record carries its style in its `+0x10` word, and the records that open
/// nothing carry this one.
const PLAIN_STYLE: u16 = 0;

/// The style a tab that is not the chosen one draws in. Its `back_03` is the dark red the
/// original fills the row with, and `0x5c111` pads the places a row leaves empty out in it.
const ROW_STYLE: u16 = 1;

/// The style the chosen tab draws in, and the strip the row below runs on from. `0x5be8b` picks
/// between this style and [`ROW_STYLE`] by comparing the button's own record with the one the
/// window keeps as the row's head, and `0x170e70` names this style's `rahm_02` and `back_05` for
/// the pieces that join the chosen tab to the row.
const TAB_STYLE: u16 = 2;

/// The commands the records carry.
pub mod command {
    /// Place a headquarters.
    pub const HEADQUARTERS: u16 = 11101;
    /// Demolish the building.
    pub const DEMOLISH: u16 = 11102;
    /// Pull the flag down.
    pub const FLAG_DOWN: u16 = 11103;
    /// Dig up the road.
    pub const DIG_ROAD: u16 = 11104;
    /// Open the second row.
    pub const SECOND_ROW: u16 = 11105;
    /// Open the third row.
    pub const THIRD_ROW: u16 = 11106;
    /// Open the attack window.
    pub const ATTACK: u16 = 11107;
    /// Place an ordinary flag.
    pub const FLAG: u16 = 11108;
    /// Place a landing flag.
    pub const LANDING_FLAG: u16 = 11109;
    /// Start a road.
    pub const ROAD: u16 = 11110;
    /// Start a waterway.
    pub const WATERWAY: u16 = 11111;
    /// Open the observation window.
    pub const OBSERVE: u16 = 11115;
    /// Order a geologist.
    pub const GEOLOGIST: u16 = 11116;
    /// Order a scout.
    pub const SCOUT: u16 = 11117;
    /// Turn the house names on and off.
    pub const HOUSE_NAMES: u16 = 11118;
    /// Move the view to the headquarters.
    pub const GO_HOME: u16 = 11119;
    /// Stop drawing the road.
    pub const STOP_ROAD: u16 = 11120;
    /// The first of the 32 commands that place a building of one type.
    pub const FIRST_BUILDING: u16 = 11121;
}

/// The 28 records, in the order the original's table holds them.
pub const RECORDS: [Record; 28] = [
    record(
        Picture::HEADQUARTERS,
        command::SECOND_ROW,
        249,
        Some(2),
        Some(1),
        Shown::Headquarters,
    ),
    record(
        Picture::HEADQUARTERS,
        command::HEADQUARTERS,
        249,
        None,
        None,
        Shown::Headquarters,
    ),
    record(
        Picture::BUILD,
        command::SECOND_ROW,
        230,
        Some(3),
        Some(12),
        Shown::Building,
    ),
    record(
        Picture::PLACE_FLAG,
        command::SECOND_ROW,
        231,
        Some(8),
        Some(10),
        Shown::Flag,
    ),
    record(
        Picture::DEMOLISH,
        command::SECOND_ROW,
        233,
        Some(22),
        Some(5),
        Shown::BuildingHere,
    ),
    record(
        Picture::CONFIRM,
        command::DEMOLISH,
        233,
        None,
        None,
        Shown::BuildingHere,
    ),
    record(
        Picture::DIG_ROAD,
        command::SECOND_ROW,
        235,
        Some(9),
        Some(7),
        Shown::RoadHere,
    ),
    record(
        Picture::CONFIRM,
        command::DIG_ROAD,
        235,
        None,
        None,
        Shown::RoadHere,
    ),
    record(
        Picture::ATTACK,
        command::ATTACK,
        236,
        Some(4),
        None,
        Shown::Attack,
    ),
    record(
        Picture::WINDOW_MENU,
        command::SECOND_ROW,
        252,
        None,
        Some(16),
        Shown::Always,
    ),
    record(
        Picture::FLAG,
        command::FLAG,
        243,
        Some(11),
        None,
        Shown::FlagFits(false),
    ),
    record(
        Picture::LANDING_FLAG,
        command::LANDING_FLAG,
        245,
        None,
        None,
        Shown::FlagFits(true),
    ),
    lists(Picture::SMALL_BUILDING, 239, Some(13), SiteSize::Small),
    lists(Picture::MEDIUM_BUILDING, 240, Some(14), SiteSize::Medium),
    lists(Picture::LARGE_BUILDING, 241, Some(15), SiteSize::Large),
    lists(Picture::MINE, 242, None, SiteSize::Mine),
    record(
        Picture::OBSERVE,
        command::OBSERVE,
        158,
        Some(17),
        None,
        Shown::Always,
    ),
    record(
        Picture::HOUSE_NAMES,
        command::HOUSE_NAMES,
        257,
        Some(18),
        None,
        Shown::Always,
    ),
    record(Picture::GO_HOME, command::GO_HOME, 258, None, None, Shown::Always),
    record(
        Picture::FLAG,
        command::FLAG,
        243,
        Some(20),
        None,
        Shown::FlagFits(false),
    ),
    record(
        Picture::LANDING_FLAG,
        command::LANDING_FLAG,
        245,
        Some(21),
        None,
        Shown::FlagFits(true),
    ),
    record(
        Picture::STOP_ROAD,
        command::STOP_ROAD,
        259,
        None,
        None,
        Shown::Always,
    ),
    record(
        Picture::FLAG_MENU,
        command::SECOND_ROW,
        234,
        Some(6),
        Some(23),
        Shown::FlagHere(false),
    ),
    record(
        Picture::ROAD,
        command::ROAD,
        246,
        Some(24),
        None,
        Shown::FlagHere(false),
    ),
    record(
        Picture::WATERWAY,
        command::WATERWAY,
        248,
        Some(25),
        None,
        Shown::FlagHere(true),
    ),
    record(
        Picture::FLAG_DOWN,
        command::FLAG_DOWN,
        234,
        Some(26),
        None,
        Shown::FlagDown,
    ),
    record(
        Picture::GEOLOGIST,
        command::GEOLOGIST,
        254,
        Some(27),
        None,
        Shown::FlagSends,
    ),
    record(Picture::SCOUT, command::SCOUT, 255, None, None, Shown::FlagSends),
];

/// A record that carries no building list.
const fn record(
    picture: Picture,
    command: u16,
    slot: u16,
    next: Option<u8>,
    opens: Option<u8>,
    shown: Shown,
) -> Record {
    Record {
        picture,
        command,
        slot,
        next,
        opens,
        lists: None,
        shown,
    }
}

/// One of the four size buttons, which lists the types of that size.
const fn lists(picture: Picture, slot: u16, next: Option<u8>, size: SiteSize) -> Record {
    Record {
        picture,
        command: command::THIRD_ROW,
        slot,
        next,
        opens: None,
        lists: Some(size),
        shown: Shown::Fits(size),
    }
}

/// What the records ask about the node the menu was opened on, read without a renderer so a test
/// can assert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one yes or no for each test the records make"
)]
pub struct ActionView {
    /// The node itself.
    pub node: Node,
    /// Whether it belongs to the player.
    pub own: bool,
    /// The largest thing that fits here.
    pub fits: SiteSize,
    /// Whether a mine fits, which the size alone does not say.
    pub mine: bool,
    /// Whether a flag fits.
    pub flag_fits: bool,
    /// Whether one of the node's edges has water on both sides.
    pub water: bool,
    /// Whether a flag stands here.
    pub flag: bool,
    /// Whether a building other than the headquarters stands here.
    pub building: bool,
    /// Whether the flag here serves a building other than the headquarters, which makes
    /// pulling it down ask about the building first.
    pub flag_building: bool,
    /// Whether the flag here serves the headquarters. `0x5a4eb` keeps that one flag standing.
    pub headquarters_flag: bool,
    /// Whether a geologist or a scout may leave the flag here (`0x199da`): any flag but a finished
    /// storehouse's, harbour's or headquarters'.
    pub sends: bool,
    /// Whether a road runs through the node.
    pub road: bool,
    /// How many soldiers the player could send against the building here.
    pub attackers: u8,
    /// Whether a road is being drawn, which changes the first row.
    pub drawing: bool,
    /// Whether the cheat word is on, which is what offers a headquarters.
    pub cheat: bool,
}

impl ActionView {
    /// What `node` offers `player`.
    #[must_use]
    pub fn read(game: &Game, player: Player, node: Node, drawing: bool) -> Self {
        use s2_sim::{MapObject, SiteFlags};

        let (world, nodes) = (game.world(), game.nodes());
        let data = nodes.get(node);
        let own = data.owner == Some(player);
        let fits = data.sites.size;
        let flag_fits = own && !data.sites.flags.contains(SiteFlags::NO_FLAG) && fits >= SiteSize::Flag;
        let water = Direction::ALL
            .into_iter()
            .any(|direction| nodes.edge_water(node, direction));
        let flag = matches!(world.object(node), MapObject::Flag(_));
        let served = match world.object(node) {
            MapObject::Flag(record) => world
                .building_at_flag(record)
                .and_then(|building| world.building(building)),
            _ => None,
        };
        let standing = match world.object(node) {
            MapObject::Building(record) => Some(record),
            _ => None,
        };
        let here = standing.and_then(|record| world.building(record));
        let headquarters = |kind| kind == s2_sim::BuildingKind::HEADQUARTERS;
        Self {
            node,
            own,
            fits,
            mine: fits == SiteSize::Mine,
            flag_fits,
            water,
            flag,
            building: here.is_some_and(|building| !headquarters(building.kind)),
            flag_building: served.is_some_and(|building| !headquarters(building.kind)),
            headquarters_flag: served.is_some_and(|building| headquarters(building.kind)),
            sends: flag
                && served.is_none_or(|building| {
                    let category = world.building_types().get(building.kind).map(|row| row.category);
                    sends_from(category, building.state)
                }),
            road: world.road_through(nodes, node).is_some(),
            attackers: standing.map_or(0, |record| game.attack_count(player, record)),
            drawing,
            cheat: false,
        }
    }
}

/// Whether a geologist or a scout may leave the flag of a building of `category` in `state`
/// (`0x199da`): unless it is a finished storehouse, harbour or headquarters.
fn sends_from(category: Option<s2_sim::Category>, state: s2_sim::BuildingState) -> bool {
    category != Some(s2_sim::Category::Storehouse) || !state.contains(s2_sim::BuildingState::FINISHED)
}

impl Shown {
    /// Whether a record carrying this test shows for `view`.
    #[must_use]
    pub fn passes(self, view: &ActionView) -> bool {
        match self {
            Self::Always => true,
            Self::Headquarters => view.cheat,
            Self::Building => view.own && (view.fits >= SiteSize::Small || view.mine),
            Self::Fits(SiteSize::Mine) => view.mine,
            Self::Fits(size) => view.own && view.fits >= size && view.fits != SiteSize::Mine,
            Self::Flag => view.flag_fits,
            Self::FlagFits(water) => view.flag_fits && view.water == water,
            Self::BuildingHere => view.own && view.building,
            Self::RoadHere => view.own && view.road,
            Self::FlagHere(water) => view.own && view.flag && (!water || view.water),
            Self::FlagDown => view.own && view.flag && !view.headquarters_flag,
            Self::FlagSends => view.own && view.flag && view.sends,
            Self::Attack => view.attackers > 0,
        }
    }
}

/// What the node allows beyond its room, which two of the building types ask about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Room {
    /// Whether a military building fits here (`0x197a6`).
    pub military: bool,
    /// Whether the map marks a harbour site here (`0x1988d`).
    pub harbour: bool,
}

impl Room {
    /// What `node` allows `player`.
    #[must_use]
    pub fn read(game: &Game, player: Player, node: Node) -> Self {
        Self {
            military: game.military_room(player, node),
            harbour: game.nodes().get(node).flags.contains(s2_sim::NodeFlags::HARBOUR),
        }
    }
}

/// The building types the third row offers for `size`, in type order, as `0x5a653` gathers them.
///
/// The headquarters is left out, a military type where one does not fit, a harbour where the
/// map marks no site, and a type `allowed` refuses, which is how a campaign chapter keeps a type
/// from the player. The original also leaves out a type that needs the cheat word, which the
/// type table here does not carry.
#[must_use]
pub fn types(
    table: &BuildingTypes,
    size: SiteSize,
    room: Room,
    allowed: impl Fn(BuildingKind) -> bool,
) -> Vec<Offer> {
    use s2_sim::Category;

    let Some(wanted) = size.building() else {
        return Vec::new();
    };
    table
        .types()
        .iter()
        .enumerate()
        .filter_map(|(place, row)| {
            let kind = BuildingKind(u8::try_from(place).ok()?);
            if row.size != wanted || kind == BuildingKind::HEADQUARTERS || !allowed(kind) {
                return None;
            }
            let offered = match (row.category, kind) {
                (Category::Military, _) => room.military,
                (_, BuildingKind::HARBOUR) => room.harbour,
                _ => true,
            };
            offered.then_some(Offer {
                kind,
                group: row.group,
            })
        })
        .collect()
}

/// How a row of buttons lays itself out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// How many buttons the row holds.
    pub places: usize,
    /// Whether any of them opens another row, which keeps the boxes square and pads the rest of
    /// the row out in red.
    pub tabs: bool,
}

impl Shape {
    /// The shape of a row holding `records`.
    #[must_use]
    pub fn of(records: &[u8]) -> Self {
        Self {
            places: records.len(),
            tabs: records.iter().any(|&index| opens_a_row(index)),
        }
    }

    /// How wide one of its boxes is.
    #[must_use]
    pub fn width(self) -> u32 {
        let (_, _, size) = BOX;
        if self.tabs || self.places == 0 {
            size
        } else {
            SHARED / u32::try_from(self.places).unwrap_or(1)
        }
    }
}

/// Whether record `index` opens a row of its own, which is what makes its button a tab.
///
/// The three commands that open a row are the records the original's table gives style 1 to,
/// and `0x5be8b` draws those in [`ROW_STYLE`] or [`TAB_STYLE`]. Every other record carries
/// style 0 and draws as a plain button.
#[must_use]
pub fn opens_a_row(index: u8) -> bool {
    RECORDS.get(usize::from(index)).is_some_and(|record| {
        matches!(
            record.command,
            command::SECOND_ROW | command::THIRD_ROW | command::ATTACK
        )
    })
}

/// A building type the third row offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    /// Which type a press places.
    pub kind: BuildingKind,
    /// Its graphics group, which is the item its icon comes from in the nation's icon archive.
    pub group: u16,
}

/// The records a row shows: the first [`PLACES`] of the chain from `head` whose test passes.
#[must_use]
pub fn row(head: u8, view: &ActionView) -> Vec<u8> {
    let mut shown = Vec::with_capacity(PLACES);
    let mut next = Some(head);
    while let Some(place) = next {
        let Some(record) = RECORDS.get(usize::from(place)) else {
            break;
        };
        if record.shown.passes(view) {
            shown.push(place);
            if shown.len() == PLACES {
                break;
            }
        }
        next = record.next;
    }
    shown
}

/// The record whose button opening the menu never puts the pointer on: the one under the road
/// tab that digs the road up. A second row that starts with it leaves the pointer on the first
/// row (`0x5b5c0`).
pub const KEEPS_POINTER_OFF: u8 = 7;

/// The action menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The record the second row was opened from.
    pub second: Option<u8>,
    /// The record the third row was opened from.
    pub third: Option<u8>,
}

impl ActionWindow {
    /// How wide the original's window is (`0x5ae00`).
    pub const WIDTH: u32 = 202;
    /// How tall the rows leave it: the first row alone, and both rows (`0x5b121`).
    pub const HEIGHT: (u32, u32) = (78, 118);
    /// What the line of help and the margin under the rows add (`0x5b121`).
    const UNDER_ROWS: u32 = 20;
    /// What a line of building types adds, and what the row adds on top of its lines
    /// (`0x5a653`).
    const LISTED: (u32, u32) = (36, 8);

    /// A menu at `id` with one row.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self {
            id,
            second: None,
            third: None,
        }
    }

    /// Chooses the first tab of each row the menu opens, which is what the original's menu
    /// shows the moment it opens (`0x5b2c4`).
    pub fn open_first(&mut self, view: &ActionView) {
        if self.second.is_none() {
            self.second = row(Self::head(view), view).first().copied();
        }
        if self.third.is_none()
            && let Some(head) = self.second.and_then(opens)
        {
            self.third = row(head, view).first().copied();
        }
    }

    /// How big the window is with the rows it has open, `listed` building types in the third.
    #[must_use]
    pub fn size(self, listed: usize) -> (u32, u32) {
        let (one, two) = Self::HEIGHT;
        let rows = if self.second.and_then(opens).is_some() {
            two
        } else {
            one
        };
        (Self::WIDTH, rows + self.listing_height(listed) + Self::UNDER_ROWS)
    }

    /// What the row of building types adds to the height. The original keeps the answer of the
    /// routine that built the row, which is 0 for a row that placed nothing.
    fn listing_height(self, listed: usize) -> u32 {
        let (line, extra) = Self::LISTED;
        let places = u32::try_from(PLACES).unwrap_or(1);
        let listed = u32::try_from(listed).unwrap_or_default();
        if self.third.is_none() || listed == 0 {
            return 0;
        }
        line * listed.div_ceil(places) + extra
    }

    /// The band of tabs a row covers, which the places it leaves empty fill out (`0x5c111`).
    #[must_use]
    pub fn band(row: usize, at: Point) -> Option<Rect> {
        let (x, rows, size) = BOX;
        let down = rows.get(row)?;
        let width = size * u32::try_from(PLACES).unwrap_or(1);
        Some(Rect::new(at.x + x, at.y + down, width, TAB.0))
    }

    /// The strip under the tabs, which takes the open tab's own colour so that the tab runs on
    /// into the row it opened.
    #[must_use]
    pub fn page(row: usize, at: Point) -> Option<Rect> {
        let (x, rows, size) = BOX;
        let down = rows.get(row)?;
        let width = size * u32::try_from(PLACES).unwrap_or(1);
        let (band, whole) = TAB;
        Some(Rect::new(
            at.x + x,
            at.y + down + i32::try_from(band).unwrap_or_default(),
            width,
            whole.saturating_sub(band),
        ))
    }

    /// Where the box for `place` of `row` sits in a window whose top-left corner is `at`.
    ///
    /// The row of building types wraps every [`PLACES`] boxes into a line of its own
    /// (`0x5a801`). A row of tabs keeps square boxes and leaves the places it does not fill to
    /// the red block that pads them out; a second row holding no tab shares [`SHARED`] pixels
    /// between the boxes it does hold instead (`0x5b5e5`). The first row's boxes are square
    /// whatever it holds.
    #[must_use]
    pub fn place(row: usize, place: usize, at: Point, shape: Shape) -> Option<Rect> {
        let (x, rows, size) = BOX;
        let down = *rows.get(row)?;
        let places = u32::try_from(PLACES).unwrap_or(1);
        if row + 1 == ROWS {
            let line = u32::try_from(place).ok()? / places;
            let column = u32::try_from(place).ok()? % places;
            let across = i32::try_from(column * size).ok()?;
            let under = i32::try_from(line * size).ok()?;
            return Some(Rect::new(at.x + x + across, at.y + down + under, size, size));
        }
        if place >= shape.places.min(PLACES) {
            return None;
        }
        let width = if row == SHARING_ROW { shape.width() } else { size };
        let height = if shape.tabs { TAB.1 } else { size };
        let across = i32::try_from(place).ok()? * i32::try_from(width).ok()?;
        Some(Rect::new(at.x + x + across, at.y + down, width, height))
    }

    /// The record the row below `row` was opened from, which is the tab drawn as open.
    #[must_use]
    const fn open(self, row: usize) -> Option<u8> {
        match row {
            0 => self.second,
            1 => self.third,
            _ => None,
        }
    }

    /// Fills the places a row of tabs leaves empty, as one block from the first of them to the
    /// row's end (`0x5c17b`): the row's red over the band, and under it the strip's edge, the
    /// open style's second face, two columns of black at the right and a row of black below. A
    /// row holding no tab shows the window's own background instead.
    fn filler(row: usize, shape: Shape, painter: &mut Painter, skin: &Skin, at: Point) {
        if !shape.tabs {
            return;
        }
        let (Some(band), Some(page)) = (Self::band(row, at), Self::page(row, at)) else {
            return;
        };
        let held = u32::try_from(shape.places.min(PLACES)).unwrap_or_default() * BOX.2;
        let Some(width) = band.width.checked_sub(held).filter(|&width| width > 0) else {
            return;
        };
        let x = band.x + i32::try_from(held).unwrap_or_default();
        let strip = Strip::of(Rect::new(x, page.y, width, page.height));
        if let Some(item) = skin.face(ROW_STYLE, Look::Idle) {
            painter.tile(item, Rect::new(x, band.y, width, band.height));
        }
        if let Some(edge) = skin.edge(TAB_STYLE) {
            painter.tile(edge, strip.top);
        }
        painter.fill(strip.bottom, STRIP_BLACK);
        painter.fill(strip.right, STRIP_BLACK);
        if let Some(face) = skin.face(TAB_STYLE, Look::On) {
            painter.tile(face, strip.face.with_width(width.saturating_sub(EDGE)));
        }
    }

    /// Draws the strip over the bottom of a tab that is not the chosen one (`0x5bffb`): the open
    /// style's edge, its face and a row of black. The row's first place takes the edge down its
    /// left as well, and its fifth two columns of black at its right.
    fn strip_under(painter: &mut Painter, skin: &Skin, tab: Rect, place: usize) {
        let strip = Strip::of(Self::foot(tab));
        let edge = skin.edge(TAB_STYLE);
        if let Some(edge) = edge {
            painter.tile(edge, strip.top);
        }
        if let Some(face) = skin.face(TAB_STYLE, Look::Idle) {
            painter.tile(face, strip.face);
        }
        painter.fill(strip.bottom, STRIP_BLACK);
        if place == 0
            && let Some(edge) = edge
        {
            painter.tile(edge, strip.left);
        }
        if place + 1 == PLACES {
            painter.fill(
                Rect {
                    x: strip.right.x,
                    ..strip.left
                },
                STRIP_BLACK,
            );
        }
    }

    /// Runs the chosen tab on into the strip under it (`0x5bf03`), over the frame the tab has
    /// already drawn: its face down its last two columns and along the row its frame left
    /// black, then a row of black under that. The row's first place keeps its edge down the left
    /// and every other place takes the face there. The fifth place has black down its right.
    fn run_on(painter: &mut Painter, skin: &Skin, tab: Rect, place: usize) {
        let strip = Strip::of(Self::foot(tab));
        let thick = i32::try_from(EDGE).unwrap_or_default();
        let (first, fifth) = (place == 0, place + 1 == PLACES);
        let face = skin.face(TAB_STYLE, Look::Idle);
        let below = Rect::new(tab.x, strip.face.y, EDGE, strip.face.height + 1);
        if first {
            if let Some(edge) = skin.edge(TAB_STYLE) {
                painter.tile(
                    edge,
                    Rect::new(tab.x, strip.top.y, EDGE, strip.top.height + below.height),
                );
            }
        } else if let Some(face) = face {
            painter.tile(face, below);
        }
        if fifth {
            painter.fill(
                Rect::new(
                    strip.right.x,
                    tab.y + thick,
                    EDGE,
                    tab.height.saturating_sub(EDGE),
                ),
                STRIP_BLACK,
            );
        } else if let Some(face) = face {
            painter.tile(
                face,
                Rect {
                    x: strip.right.x,
                    ..below
                },
            );
        }
        if let Some(face) = face {
            let (x, width) = match (first, fifth) {
                (true, _) => (tab.x + thick, tab.width.saturating_sub(EDGE)),
                (false, true) => (tab.x, tab.width.saturating_sub(EDGE)),
                (false, false) => (tab.x, tab.width),
            };
            painter.tile(face, Rect::new(x, strip.bottom.y - 1, width, 1));
        }
        painter.fill(strip.bottom, STRIP_BLACK);
    }

    /// The part of a tab's box the strip covers.
    fn foot(tab: Rect) -> Rect {
        let down = i32::try_from(TAB.0).unwrap_or_default();
        Rect::new(tab.x, tab.y + down, tab.width, tab.height.saturating_sub(TAB.0))
    }

    /// Where opening the menu puts the pointer, for a menu whose top-left corner is `at`
    /// (`0x5ae3b`): the middle of the second row's first button. The first row's first button
    /// takes it instead when no second row is open, when that row is empty, and when it starts
    /// with [`KEEPS_POINTER_OFF`]. A menu with no button leaves the pointer alone.
    #[must_use]
    pub fn pointer_to(self, view: &ActionView, at: Point) -> Option<Point> {
        let rows = self.rows(view);
        let row = match rows.get(1) {
            Some(second) if second.first().is_some_and(|&first| first != KEEPS_POINTER_OFF) => 1,
            _ => 0,
        };
        let records = rows.get(row)?;
        records.first()?;
        let button = Self::place(row, 0, at, Shape::of(records))?;
        let half = |length: u32| i32::try_from(length / 2).unwrap_or_default();
        Some(Point::new(
            button.x + half(button.width),
            button.y + half(button.height),
        ))
    }

    /// The record the first row starts from.
    #[must_use]
    pub const fn head(view: &ActionView) -> u8 {
        if view.drawing { DRAWING } else { FIRST }
    }

    /// The records each row of buttons from the table shows, which is the first row and the
    /// second.
    #[must_use]
    pub fn rows(self, view: &ActionView) -> Vec<Vec<u8>> {
        let mut rows = vec![row(Self::head(view), view)];
        if let Some(opens) = self.second.and_then(opens) {
            rows.push(row(opens, view));
        }
        rows
    }

    /// The size of building the third row lists, where one is open.
    #[must_use]
    pub fn listing(self) -> Option<SiteSize> {
        let record = RECORDS.get(usize::from(self.third?))?;
        record.lists
    }

    /// Takes `mouse`, draws the menu into `painter` and gives back what a release does. `types`
    /// holds what the third row lists, which [`types`] gathers.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &ActionView,
        types: &[Offer],
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
        let mut pressed = None;
        let rows = self.rows(view);
        for (number, row) in rows.iter().enumerate() {
            let open = self
                .open(number)
                .and_then(|index| row.iter().position(|&held| held == index));
            let shape = Shape::of(row);
            Self::filler(number, shape, painter, skin, at);
            for (place, &index) in row.iter().enumerate() {
                let Some(box_) = Self::place(number, place, at, shape) else {
                    continue;
                };
                let tab = opens_a_row(index);
                let rect = box_;
                let slot = u16::try_from(number * PLACES + place).unwrap_or_default();
                let widget = WidgetId::new(self.id, slot);
                if let Some(record) = RECORDS.get(usize::from(index)) {
                    ui.note_help(rect, mouse, Label::Slot(record.slot));
                }
                let acted = ui.press(widget, rect, mouse, true);
                let chosen = tab && open == Some(place);
                // The chosen tab shows in the open style. The pointer over a button picks the
                // style's second face (`0x51558`) and a press lights the other pair of edges.
                let look = ui.button(widget).look(true, false);
                let style = match (tab, chosen) {
                    (false, _) => PLAIN_STYLE,
                    (true, false) => ROW_STYLE,
                    (true, true) => TAB_STYLE,
                };
                let face = skin.face(style, look);
                if let Some(item) = face {
                    painter.frame_edges(item, skin.edge(style), box_, look.edges());
                }
                // Every button centres its picture on its whole box, as `0x4e3d8` does, and a
                // tab draws its strip afterwards, over the foot of the picture.
                if let Some(record) = RECORDS.get(usize::from(index))
                    && let Some(item) = skin.picture(record.picture)
                {
                    painter.item_centred(item, box_);
                }
                // Every tab carries its piece of the strip over its bottom 12 rows. The chosen
                // one runs on into it instead.
                if face.is_some() && chosen {
                    Self::run_on(painter, skin, box_, place);
                } else if face.is_some() && tab {
                    Self::strip_under(painter, skin, box_, place);
                }
                if acted && pressed.is_none() {
                    pressed = Some(index);
                }
            }
        }
        let placed = self.list(ui, painter, skin, types, mouse);
        painter.pop_clip();
        if let Some(kind) = placed {
            self.second = None;
            self.third = None;
            ui.close(self.id);
            return Some(Outcome::Command(s2_sim::Command::PlaceBuilding {
                node: view.node,
                kind,
            }));
        }
        let index = pressed?;
        let act = self.acts(index, view);
        if act.closes {
            ui.close(self.id);
        }
        act.outcome
    }

    /// Draws the third row, the building types, and gives back the one a release places.
    fn list(
        self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        types: &[Offer],
        mouse: Pointer,
    ) -> Option<BuildingKind> {
        self.third?;
        let window = *ui.window(self.id)?;
        let at = Point::new(window.area.x, window.area.y);
        let mut placed = None;
        for (place, &offer) in types.iter().enumerate() {
            let Some(rect) = Self::place(ROWS - 1, place, at, Shape::of(&[])) else {
                continue;
            };
            let slot = u16::try_from((ROWS - 1) * PLACES + place).unwrap_or_default();
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(
                rect,
                mouse,
                Label::Named(BUILDING_NAMES + u16::from(offer.kind.0)),
            );
            let acted = ui.press(widget, rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if look == Look::Pressed
                && let Some(item) = skin.item(Control::Button, Look::On)
            {
                painter.tile(item, rect);
            }
            painter.building(offer.group, rect.middle());
            if acted && placed.is_none() {
                placed = Some(offer.kind);
            }
        }
        placed
    }

    /// What a press on record `index` does.
    fn acts(&mut self, index: u8, view: &ActionView) -> Act {
        let Some(record) = RECORDS.get(usize::from(index)) else {
            return Act::stays(None);
        };
        match record.command {
            command::SECOND_ROW => {
                self.second = Some(index);
                self.third = None;
                Act::stays(None)
            }
            command::THIRD_ROW => {
                self.third = Some(index);
                Act::stays(None)
            }
            command::ATTACK => {
                self.second = Some(index);
                Act::stays(Some(Outcome::Open(kind::MILITARY)))
            }
            other => Act {
                outcome: Some(outcome(other, view)),
                closes: true,
            },
        }
    }
}

/// What a press on a button does: what it asks for, and whether the menu closes behind it.
/// `0x5b949` closes the menu for every command but the three that open a row of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Act {
    outcome: Option<Outcome>,
    closes: bool,
}

impl Act {
    /// An act that leaves the menu open.
    const fn stays(outcome: Option<Outcome>) -> Self {
        Self {
            outcome,
            closes: false,
        }
    }
}

/// The record a row opens from the one at `index`.
fn opens(index: u8) -> Option<u8> {
    RECORDS.get(usize::from(index))?.opens
}

/// What a command other than the three that open a row asks for.
fn outcome(id: u16, view: &ActionView) -> Outcome {
    use s2_sim::{BuildingKind, Command};

    let node = view.node;
    match id {
        command::HEADQUARTERS => Outcome::Command(Command::PlaceBuilding {
            node,
            kind: BuildingKind::HEADQUARTERS,
        }),
        command::DEMOLISH => Outcome::Command(Command::Demolish { node }),
        command::FLAG_DOWN if view.flag_building => Outcome::Open(kind::DEMOLISH),
        command::FLAG_DOWN => Outcome::Command(Command::RemoveFlag { node }),
        command::DIG_ROAD if view.drawing => Outcome::Command(Command::CancelRoad),
        command::DIG_ROAD => Outcome::Command(Command::RemoveRoad { node }),
        command::FLAG => Outcome::Command(Command::PlaceFlag { node, water: false }),
        command::LANDING_FLAG => Outcome::Command(Command::PlaceFlag { node, water: true }),
        command::ROAD => Outcome::Command(Command::StartRoad { node, water: false }),
        command::WATERWAY => Outcome::Command(Command::StartRoad { node, water: true }),
        command::STOP_ROAD => Outcome::Command(Command::CancelRoad),
        command::GEOLOGIST => Outcome::Command(Command::SendGeologist { node }),
        command::SCOUT => Outcome::Command(Command::SendScout { node }),
        command::GO_HOME => Outcome::Headquarters,
        command::HOUSE_NAMES => Outcome::HouseNames,
        command::OBSERVE => Outcome::Open(kind::OBSERVE),
        other => Outcome::Command(Command::PlaceBuilding {
            node,
            kind: BuildingKind(u8::try_from(other - command::FIRST_BUILDING).unwrap_or_default()),
        }),
    }
}

/// The kind the menu opens as.
pub const KIND: WindowId = kind::ACTION;

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> ActionView {
        ActionView {
            node: Node { x: 4, y: 6 },
            own: true,
            fits: SiteSize::Large,
            flag_fits: true,
            ..ActionView::default()
        }
    }

    #[test]
    fn a_node_with_room_for_a_flag_alone_offers_the_flag_first() {
        let view = ActionView {
            fits: SiteSize::Flag,
            ..view()
        };
        let row = row(FIRST, &view);
        assert_eq!(row.first(), Some(&3), "the flag, not the trowel");
    }

    fn middle(rect: Rect) -> Point {
        let half = |length: u32| i32::try_from(length / 2).unwrap();
        Point::new(rect.x + half(rect.width), rect.y + half(rect.height))
    }

    #[test]
    fn opening_puts_the_pointer_on_the_second_rows_first_button() {
        let view = view();
        let mut menu = ActionWindow::new(kind::ACTION);
        menu.open_first(&view);
        let rows = menu.rows(&view);
        let at = Point::new(100, 50);
        let first = ActionWindow::place(1, 0, at, Shape::of(&rows[1])).unwrap();
        assert_eq!(menu.pointer_to(&view, at), Some(middle(first)));
    }

    #[test]
    fn a_second_row_that_digs_a_road_up_leaves_the_pointer_on_the_first() {
        let view = ActionView {
            fits: SiteSize::Nothing,
            flag_fits: false,
            road: true,
            ..view()
        };
        let mut menu = ActionWindow::new(kind::ACTION);
        menu.open_first(&view);
        let rows = menu.rows(&view);
        assert_eq!(rows[1].first(), Some(&KEEPS_POINTER_OFF));
        let at = Point::new(100, 50);
        let first = ActionWindow::place(0, 0, at, Shape::of(&rows[0])).unwrap();
        assert_eq!(menu.pointer_to(&view, at), Some(middle(first)));
    }

    #[test]
    fn every_record_names_a_chain_inside_the_table() {
        for (place, record) in RECORDS.iter().enumerate() {
            for index in [record.next, record.opens].into_iter().flatten() {
                assert!(usize::from(index) < RECORDS.len(), "record {place} names {index}");
            }
        }
    }

    #[test]
    fn a_row_keeps_the_first_five_that_pass() {
        let mut view = view();
        view.building = true;
        view.road = true;
        view.cheat = true;
        let shown = row(FIRST, &view);
        assert_eq!(shown.len(), PLACES);
        assert_eq!(shown[0], 0, "the headquarters comes first with the cheat word");
        view.cheat = false;
        let plain = row(FIRST, &view);
        assert!(!plain.contains(&0), "no headquarters without the cheat word");
        assert!(plain.contains(&9), "the window menu always shows");
    }

    #[test]
    fn drawing_a_road_offers_its_own_row() {
        let mut view = view();
        view.drawing = true;
        assert_eq!(ActionWindow::head(&view), DRAWING);
        let shown = row(ActionWindow::head(&view), &view);
        assert_eq!(shown, vec![19, 21], "an ordinary flag and the way out");
    }

    #[test]
    fn a_flag_with_water_offers_the_landing_one() {
        let mut view = view();
        view.water = true;
        let shown = row(10, &view);
        assert_eq!(shown, vec![11], "the landing flag alone");
    }

    #[test]
    fn the_sizes_offer_what_the_node_takes() {
        let mut view = view();
        view.fits = SiteSize::Small;
        assert_eq!(row(12, &view), vec![12], "a small building alone");
        view.fits = SiteSize::Large;
        assert_eq!(row(12, &view), vec![12, 13, 14]);
        view.fits = SiteSize::Mine;
        view.mine = true;
        assert_eq!(row(12, &view), vec![15], "a mine takes no other size");
    }

    #[test]
    fn the_types_the_third_row_lists_follow_the_size() {
        use s2_sim::{BuildingKind, Category};

        let table = &BuildingTypes::ORIGINAL;
        let room = Room {
            military: true,
            harbour: true,
        };
        for (size, class) in [
            (SiteSize::Small, s2_sim::Size::Small),
            (SiteSize::Medium, s2_sim::Size::Medium),
            (SiteSize::Large, s2_sim::Size::Large),
            (SiteSize::Mine, s2_sim::Size::Mine),
        ] {
            let offered = types(table, size, room, |_| true);
            assert!(!offered.is_empty(), "{size:?}");
            assert!(
                !offered
                    .iter()
                    .any(|offer| offer.kind == BuildingKind::HEADQUARTERS)
            );
            for offer in offered {
                let row = table.get(offer.kind).expect("a row");
                assert_eq!(row.size, class, "{:?}", offer.kind);
                assert_eq!(row.group, offer.group);
            }
        }
        let found = types(table, SiteSize::Flag, room, |_| true);
        assert!(found.is_empty(), "{found:?}");

        let cramped = Room::default();
        let large = types(table, SiteSize::Large, cramped, |_| true);
        assert!(
            !large.iter().any(|offer| offer.kind == BuildingKind::HARBOUR),
            "no harbour site"
        );
        for offer in large {
            let row = table.get(offer.kind).expect("a row");
            assert_ne!(row.category, Category::Military, "no room for one");
        }
    }

    #[test]
    fn the_third_row_leaves_out_a_type_the_chapter_has_not_given() {
        use s2_sim::BuildingKind;

        let room = Room {
            military: true,
            harbour: true,
        };
        let given = [BuildingKind::WOODCUTTER, BuildingKind::QUARRY];
        let small = types(&BuildingTypes::ORIGINAL, SiteSize::Small, room, |kind| {
            given.contains(&kind)
        });
        let kinds: Vec<_> = small.iter().map(|offer| offer.kind).collect();
        assert_eq!(kinds, given);
    }

    #[test]
    fn the_boxes_follow_the_rows_the_original_draws() {
        let at = Point::new(0, 0);
        let tabs = Shape {
            places: PLACES,
            tabs: true,
        };
        assert_eq!(
            ActionWindow::place(0, 0, at, tabs),
            Some(Rect::new(11, 21, 36, 44))
        );
        assert_eq!(
            ActionWindow::place(0, 4, at, tabs),
            Some(Rect::new(11 + 4 * 36, 21, 36, 44))
        );
        assert_eq!(
            ActionWindow::place(1, 0, at, tabs),
            Some(Rect::new(11, 65, 36, 44))
        );
        assert_eq!(
            ActionWindow::place(2, 0, at, tabs),
            Some(Rect::new(11, 109, 36, 36))
        );
        assert_eq!(ActionWindow::place(0, PLACES, at, tabs), None);
        assert_eq!(ActionWindow::place(ROWS, 0, at, tabs), None);
        let width = i32::try_from(ActionWindow::WIDTH).expect("fits");
        let last = ActionWindow::place(0, PLACES - 1, at, tabs).expect("a box");
        assert!(last.right() <= width, "{last:?}");
    }

    #[test]
    fn the_first_row_keeps_square_boxes_when_it_holds_no_tab() {
        let drawing = Shape {
            places: 2,
            tabs: false,
        };
        let at = Point::new(0, 0);
        assert_eq!(
            ActionWindow::place(0, 0, at, drawing),
            Some(Rect::new(11, 21, 36, 36))
        );
        assert_eq!(
            ActionWindow::place(0, 1, at, drawing),
            Some(Rect::new(47, 21, 36, 36))
        );
    }

    #[test]
    fn a_row_holding_no_tab_shares_its_width_between_the_boxes_it_holds() {
        let at = Point::new(0, 0);
        let three = Shape {
            places: 3,
            tabs: false,
        };
        assert_eq!(three.width(), 60);
        assert_eq!(
            ActionWindow::place(1, 0, at, three),
            Some(Rect::new(11, 65, 60, 36))
        );
        assert_eq!(
            ActionWindow::place(1, 2, at, three),
            Some(Rect::new(11 + 2 * 60, 65, 60, 36))
        );
        assert_eq!(
            ActionWindow::place(1, 3, at, three),
            None,
            "three is all it holds"
        );
    }

    #[test]
    fn the_list_of_building_types_wraps_into_lines_of_five() {
        let at = Point::new(0, 0);
        let none = Shape {
            places: 0,
            tabs: false,
        };
        let first = ActionWindow::place(2, 0, at, none).expect("a box");
        let sixth = ActionWindow::place(2, 5, at, none).expect("a box");
        assert_eq!(sixth, Rect::new(first.x, first.y + 36, 36, 36));
        let last = ActionWindow::place(2, 9, at, none).expect("a box");
        assert_eq!(last.x, first.x + 4 * 36);
        assert_eq!(last.y, first.y + 36);
    }

    #[test]
    fn a_second_row_makes_the_window_taller() {
        let mut window = ActionWindow::new(KIND);
        assert_eq!(window.size(0), (ActionWindow::WIDTH, 78 + 20));
        window.second = Some(2);
        assert_eq!(window.size(0), (ActionWindow::WIDTH, 118 + 20));
    }

    #[test]
    fn the_building_types_add_a_line_at_a_time() {
        let mut window = ActionWindow::new(KIND);
        window.second = Some(2);
        window.third = Some(12);
        assert_eq!(
            window.size(0),
            (ActionWindow::WIDTH, 118 + 20),
            "none placed adds nothing"
        );
        assert_eq!(window.size(5), (ActionWindow::WIDTH, 118 + 36 + 8 + 20));
        assert_eq!(window.size(6), (ActionWindow::WIDTH, 118 + 72 + 8 + 20));
    }

    #[test]
    fn a_command_asks_for_what_its_record_carries() {
        use s2_sim::Command;

        let view = view();
        let node = view.node;
        assert_eq!(
            outcome(command::FLAG, &view),
            Outcome::Command(Command::PlaceFlag { node, water: false })
        );
        assert_eq!(
            outcome(command::WATERWAY, &view),
            Outcome::Command(Command::StartRoad { node, water: true })
        );
        assert_eq!(
            outcome(command::GEOLOGIST, &view),
            Outcome::Command(Command::SendGeologist { node })
        );
        assert_eq!(outcome(command::GO_HOME, &view), Outcome::Headquarters);
        assert_eq!(
            outcome(command::FIRST_BUILDING + 5, &view),
            Outcome::Command(Command::PlaceBuilding {
                node,
                kind: s2_sim::BuildingKind(5)
            }),
            "the low byte names the type"
        );
    }

    #[test]
    fn every_flag_but_the_headquarters_own_may_be_pulled_down() {
        let mut view = view();
        view.own = true;
        view.flag = true;
        assert!(Shown::FlagDown.passes(&view), "a flag serving nothing comes down");
        view.flag_building = true;
        assert!(
            Shown::FlagDown.passes(&view),
            "a flag serving a building comes down"
        );
        view.flag_building = false;
        view.headquarters_flag = true;
        assert!(!Shown::FlagDown.passes(&view), "the headquarters keeps its flag");
    }

    #[test]
    fn pulling_a_flag_down_asks_before_a_building_goes() {
        let mut view = view();
        view.flag = true;
        assert_eq!(
            outcome(command::FLAG_DOWN, &view),
            Outcome::Command(s2_sim::Command::RemoveFlag { node: view.node })
        );
        view.flag_building = true;
        assert_eq!(outcome(command::FLAG_DOWN, &view), Outcome::Open(kind::DEMOLISH));
    }

    #[test]
    fn a_type_button_draws_its_nations_icon_and_no_chrome() {
        use crate::paint::OpKind;
        use crate::store::Ui;
        use s2_sim::BuildingKind;

        let offers = [
            Offer {
                kind: BuildingKind(5),
                group: 12,
            },
            Offer {
                kind: BuildingKind(7),
                group: 20,
            },
        ];
        let mut window = ActionWindow::new(KIND);
        window.second = Some(2);
        window.third = Some(12);
        let (width, height) = window.size(offers.len());
        let mut ui = Ui::new();
        ui.open(KIND, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.list(&mut ui, &mut painter, &Skin::new(), &offers, Pointer::default());
        let drawn: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Game { slot, .. } => Some(slot),
                _ => None,
            })
            .collect();
        assert_eq!(drawn, vec![12, 20]);
        assert_eq!(painter.ops().len(), offers.len(), "no face until one is held");
    }

    #[test]
    fn a_hovered_building_type_reports_its_name() {
        use crate::store::Ui;
        use s2_sim::BuildingKind;

        let offers = [Offer {
            kind: BuildingKind(5),
            group: 12,
        }];
        let mut window = ActionWindow::new(KIND);
        window.second = Some(2);
        window.third = Some(12);
        let (width, height) = window.size(offers.len());
        let mut ui = Ui::new();
        ui.open(KIND, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let rect = ActionWindow::place(ROWS - 1, 0, Point::new(0, 0), Shape::of(&[])).expect("a box");
        let mouse = Pointer {
            at: rect.middle(),
            ..Pointer::default()
        };
        window.list(&mut ui, &mut painter, &Skin::new(), &offers, mouse);
        assert_eq!(ui.help(), Some(Label::Named(BUILDING_NAMES + 5)));
    }

    #[test]
    fn a_tabs_face_ends_where_the_band_does() {
        let at = Point::new(10, 20);
        let shape = Shape {
            places: PLACES,
            tabs: true,
        };
        let band = ActionWindow::band(0, at).expect("a band");
        let box_ = ActionWindow::place(0, 0, at, shape).expect("a box");
        assert_eq!(box_.with_height(TAB.0).bottom(), band.bottom());
    }

    #[test]
    fn explorers_leave_any_flag_but_a_finished_storehouses() {
        use s2_sim::{BuildingState, Category};
        let finished = BuildingState::FINISHED;
        assert!(sends_from(Some(Category::Workplace), finished));
        assert!(sends_from(Some(Category::Military), finished));
        assert!(
            sends_from(Some(Category::Storehouse), BuildingState::BUILDING),
            "a site"
        );
        assert!(!sends_from(Some(Category::Storehouse), finished));
    }

    #[test]
    fn a_plain_buttons_picture_centres_on_its_whole_box() {
        use crate::paint::{ItemId, OpKind};
        use crate::store::Ui;

        let view = ActionView {
            drawing: true,
            ..view()
        };
        let shown = row(DRAWING, &view);
        let mut skin = Skin::new();
        for &index in &shown {
            skin.set_picture(RECORDS[usize::from(index)].picture, ItemId(40 + u16::from(index)));
        }
        let mut window = ActionWindow::new(KIND);
        window.open_first(&view);
        let mut ui = Ui::new();
        let (width, height) = window.size(0);
        ui.open(KIND, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.run(&mut ui, &mut painter, &skin, &view, &[], Pointer::default());
        let first = ActionWindow::place(0, 0, Point::new(0, 0), Shape::of(&shown)).expect("a box");
        assert!(
            painter
                .ops()
                .iter()
                .any(|op| matches!(op.kind, OpKind::ItemIn { within, .. } if within == first)),
            "the picture centres on the 36 by 36 box"
        );
    }

    #[test]
    fn a_row_covers_the_five_places_it_holds() {
        let at = Point::new(10, 20);
        let band = ActionWindow::band(0, at).expect("a band");
        let page = ActionWindow::page(0, at).expect("a page");
        let shape = Shape {
            places: PLACES,
            tabs: true,
        };
        let first = ActionWindow::place(0, 0, at, shape).expect("a box");
        let last = ActionWindow::place(0, PLACES - 1, at, shape).expect("a box");
        assert_eq!((band.x, band.y), (first.x, first.y));
        assert_eq!(band.right(), last.right());
        assert_eq!(band.height, TAB.0);
        assert_eq!(first.height, TAB.1, "a tab reaches the bottom of its row");
        assert_eq!(page.y, band.bottom(), "the strip runs on from the tabs");
        assert_eq!(page.bottom(), first.bottom());
        assert_eq!(page.width, band.width);
    }

    #[test]
    fn a_fresh_menu_chooses_the_first_tab_of_every_row_it_opens() {
        let view = view();
        let mut window = ActionWindow::new(kind::ACTION);
        window.open_first(&view);
        let first = *window.rows(&view)[0].first().unwrap();
        assert_eq!(window.second, Some(first), "the first tab of the first row");
        let second = *window.rows(&view)[1].first().unwrap();
        assert_eq!(window.third, Some(second), "and of the row it opens");
        assert_eq!(
            window.listing(),
            Some(SiteSize::Small),
            "the smallest buildings are listed"
        );
    }

    /// A skin with the two styles the rows draw from, so the ops carry their rectangles.
    fn tabs() -> Skin {
        use crate::paint::ItemId;
        let mut skin = Skin::new();
        skin.set_face(ROW_STYLE, Look::Idle, ItemId(1));
        skin.set_face(TAB_STYLE, Look::On, ItemId(2));
        skin.set_edge(TAB_STYLE, ItemId(3));
        skin
    }

    #[test]
    fn a_row_with_nothing_under_it_takes_no_band_and_no_strip() {
        use crate::paint::OpKind;
        use crate::store::Ui;

        let view = ActionView {
            fits: SiteSize::Flag,
            ..view()
        };
        let mut window = ActionWindow::new(KIND);
        window.open_first(&view);
        let rows = window.rows(&view);
        assert_eq!(rows.len(), 2, "the flag opens a row of flags");
        let mut ui = Ui::new();
        let (width, height) = window.size(0);
        ui.open(KIND, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.run(&mut ui, &mut painter, &tabs(), &view, &[], Pointer::default());
        let at = Point::new(0, 0);
        let tiles = |rect: Rect| {
            painter.ops().iter().any(|op| match op.kind {
                OpKind::Tile { within, .. } => within.intersect(rect) == within,
                _ => false,
            })
        };
        let band = ActionWindow::band(0, at).expect("a band");
        let page = ActionWindow::page(0, at).expect("a page");
        assert!(tiles(band), "the row that opened another fills its empty places");
        assert!(tiles(page), "and carries the strip under them");
        let black = painter
            .ops()
            .iter()
            .filter(|op| match op.kind {
                OpKind::Fill { rect, index } => index == STRIP_BLACK && rect.y == page.bottom() - 1,
                _ => false,
            })
            .count();
        assert!(black > 0, "the strip ends in a row of its own black");
        assert!(
            !tiles(ActionWindow::band(1, at).expect("a band")),
            "the row with nothing under it sits on the window's own background"
        );
        assert!(
            !tiles(ActionWindow::page(1, at).expect("a page")),
            "and carries no strip"
        );
    }

    #[test]
    fn the_tab_that_opened_a_row_is_the_one_drawn_open() {
        let mut window = ActionWindow::new(KIND);
        assert_eq!(window.open(0), None);
        window.second = Some(2);
        window.third = Some(12);
        assert_eq!(window.open(0), Some(2));
        assert_eq!(window.open(1), Some(12));
        assert_eq!(window.open(2), None, "the row of building types holds no tab");
    }

    #[test]
    fn a_press_on_a_size_opens_the_third_row() {
        let mut window = ActionWindow::new(KIND);
        let view = view();
        assert_eq!(
            window.acts(2, &view),
            Act::stays(None),
            "building opens the second row"
        );
        assert_eq!(window.second, Some(2));
        assert_eq!(window.acts(12, &view), Act::stays(None), "a size opens the third");
        assert_eq!(window.third, Some(12));
        assert_eq!(
            window.acts(3, &view),
            Act::stays(None),
            "another second row clears the third"
        );
        assert_eq!(window.third, None);
    }

    #[test]
    fn a_button_that_acts_closes_the_menu_and_one_that_opens_a_row_does_not() {
        let mut window = ActionWindow::new(KIND);
        let mut view = view();
        view.own = true;
        view.flag = true;
        assert!(window.acts(25, &view).closes, "pulling the flag down closes it");
        assert!(!window.acts(22, &view).closes, "the flag row keeps it open");
        assert!(!window.acts(8, &view).closes, "the attack row keeps it open");
    }
}
