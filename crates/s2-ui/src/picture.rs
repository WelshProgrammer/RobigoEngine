//! What a picture in the interface is, apart from which file draws it.
//!
//! A control's face comes from the style it carries, and its picture is a separate `IO.DAT` item
//! drawn over that face. The items carry transparency and are smaller than the rectangles they sit
//! in: `io014`, the tick that accepts a prompt, is 12 by 10 with under half of it drawn. A window
//! that named the file would tie what a control means to where its art happens to live, which is
//! what makes new windows and replacement art awkward ([ui.md](../../../docs/design/ui.md)).
//!
//! So a window names a [`Picture`] and the skin says which item draws it. [`ORIGINAL`] is the
//! mapping the shipped data gives, and it is allowed to send two pictures to one item: the
//! original draws the merchandise graph's food group with the same icon as the goods distribution
//! window's food page, and that is a coincidence of 1996 rather than something a later theme has
//! to keep.

/// One picture the interface can draw, whatever art a skin gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Picture(pub u16);

impl Picture {
    /// Accepts what the prompts offer.
    pub const CONFIRM: Self = Self(0);
    /// Refuses it.
    pub const CANCEL: Self = Self(1);
    /// Moves the view to a place the window names.
    pub const GO_TO: Self = Self(2);

    /// Turns the map's territory colours on and off.
    pub const MAP_TERRITORY: Self = Self(3);
    /// The same for the buildings on it.
    pub const MAP_BUILDINGS: Self = Self(4);
    /// The same for the roads.
    pub const MAP_ROADS: Self = Self(5);
    /// Swaps the map between the world's shape and a fixed size.
    pub const MAP_SIZE: Self = Self(6);

    /// The game menu's own picture.
    pub const MENU_TITLE: Self = Self(7);
    /// Sound effects are playing.
    pub const EFFECTS_ON: Self = Self(8);
    /// Sound effects are off.
    pub const EFFECTS_OFF: Self = Self(9);
    /// Music is playing.
    pub const MUSIC_ON: Self = Self(10);
    /// Music is off.
    pub const MUSIC_OFF: Self = Self(11);

    /// Every letter, whatever it is about.
    pub const LETTERS_ALL: Self = Self(12);
    /// Letters about soldiers and fighting.
    pub const LETTERS_MILITARY: Self = Self(13);
    /// Letters a geologist sends.
    pub const LETTERS_GEOLOGICAL: Self = Self(14);
    /// Letters about the economy.
    pub const LETTERS_ECONOMY: Self = Self(15);
    /// Everything else.
    pub const LETTERS_VARIOUS: Self = Self(16);

    /// Back to the start of a list.
    pub const FIRST: Self = Self(17);
    /// One step back.
    pub const PREVIOUS: Self = Self(18);
    /// One step on.
    pub const NEXT: Self = Self(19);
    /// On to the end.
    pub const LAST: Self = Self(20);

    /// Opens the goods distribution window.
    pub const OPEN_DISTRIBUTION: Self = Self(21);
    /// Opens the transport priorities window.
    pub const OPEN_TRANSPORT: Self = Self(22);
    /// Opens the tool orders window.
    pub const OPEN_TOOLS: Self = Self(23);
    /// Opens the statistics graph.
    pub const OPEN_STATISTICS: Self = Self(24);
    /// Opens the merchandise graph.
    pub const OPEN_MERCHANDISE: Self = Self(25);
    /// Opens the list of buildings.
    pub const OPEN_BUILDINGS: Self = Self(26);
    /// Opens the player's own stock.
    pub const OPEN_STOCK: Self = Self(27);
    /// Opens the production times.
    pub const OPEN_PRODUCTION: Self = Self(28);
    /// Opens the military settings.
    pub const OPEN_MILITARY: Self = Self(29);
    /// Opens the ship register.
    pub const OPEN_SHIPS: Self = Self(30);
    /// Opens the game menu.
    pub const OPEN_MENU: Self = Self(31);

    /// The goods distribution page for food, the group that feeds the mines.
    pub const DISTRIBUTION_FOOD: Self = Self(32);
    /// The page for grain.
    pub const DISTRIBUTION_GRAIN: Self = Self(33);
    /// The page for iron.
    pub const DISTRIBUTION_IRON: Self = Self(34);
    /// The page for coal.
    pub const DISTRIBUTION_COAL: Self = Self(35);
    /// The page for boards.
    pub const DISTRIBUTION_BOARDS: Self = Self(36);
    /// The page for water.
    pub const DISTRIBUTION_WATER: Self = Self(37);

    /// Marks the merchandise graph's food group.
    pub const MERCHANDISE_FOOD: Self = Self(38);
    /// Marks its weapons group.
    pub const MERCHANDISE_WEAPONS: Self = Self(39);

    /// The sector to the north, which is where a ship's six choices start.
    pub const SECTOR_NORTH: Self = Self(40);
    /// Clockwise from it.
    pub const SECTOR_NORTH_EAST: Self = Self(41);
    /// Clockwise again.
    pub const SECTOR_SOUTH_EAST: Self = Self(42);
    /// The sector to the south.
    pub const SECTOR_SOUTH: Self = Self(43);
    /// Clockwise again.
    pub const SECTOR_SOUTH_WEST: Self = Self(44);
    /// The last of the six.
    pub const SECTOR_NORTH_WEST: Self = Self(45);

    /// The picture beside the mission story at its wide size.
    pub const DIARY_BACKDROP: Self = Self(46);
    /// The one beside it at its small size.
    pub const NOTE_BACKDROP: Self = Self(47);

    /// The action menu's own pictures, which the row of buttons on a node draws from.
    /// Places the headquarters the cheat word offers.
    pub const HEADQUARTERS: Self = Self(48);
    /// Opens the sizes of building that fit.
    pub const BUILD: Self = Self(49);
    /// Opens the flags that fit.
    pub const PLACE_FLAG: Self = Self(50);
    /// Opens the prompt that demolishes the building.
    pub const DEMOLISH: Self = Self(51);
    /// Opens the prompt that digs up the road.
    pub const DIG_ROAD: Self = Self(52);
    /// Opens the attack window.
    pub const ATTACK: Self = Self(53);
    /// Opens the row of view settings.
    pub const WINDOW_MENU: Self = Self(54);
    /// Places an ordinary flag.
    pub const FLAG: Self = Self(55);
    /// Places a flag a ship can land at.
    pub const LANDING_FLAG: Self = Self(56);
    /// Lists the small building types.
    pub const SMALL_BUILDING: Self = Self(57);
    /// Lists the medium ones.
    pub const MEDIUM_BUILDING: Self = Self(58);
    /// Lists the large ones.
    pub const LARGE_BUILDING: Self = Self(59);
    /// Lists the mines.
    pub const MINE: Self = Self(60);
    /// Opens an observation window on the node.
    pub const OBSERVE: Self = Self(61);
    /// Turns the house names on and off.
    pub const HOUSE_NAMES: Self = Self(62);
    /// Moves the view to the player's headquarters.
    pub const GO_HOME: Self = Self(63);
    /// Stops the road being drawn.
    pub const STOP_ROAD: Self = Self(64);
    /// Opens what the flag on the node offers.
    pub const FLAG_MENU: Self = Self(65);
    /// Starts a road from the flag.
    pub const ROAD: Self = Self(66);
    /// Starts a waterway from it.
    pub const WATERWAY: Self = Self(67);
    /// Pulls the flag down.
    pub const FLAG_DOWN: Self = Self(68);
    /// Orders a geologist to the flag.
    pub const GEOLOGIST: Self = Self(69);
    /// Orders a scout to it.
    pub const SCOUT: Self = Self(70);

    /// Opens the map window from the toolbar.
    pub const MAP_WINDOW: Self = Self(71);
    /// Opens the screen that opens the others.
    pub const MAIN_SELECTION: Self = Self(72);
    /// Turns the construction aid on and off.
    pub const CONSTRUCTION_AID: Self = Self(73);
    /// Opens the messages window.
    pub const MESSAGES: Self = Self(74);

    /// The backdrop the screens that name one of their own fill their client area with.
    pub const SCREEN_BACKDROP: Self = Self(75);

    /// Opens the keyboard layout.
    pub const OPEN_KEYS: Self = Self(76);
    /// Opens the shipped notes.
    pub const OPEN_README: Self = Self(77);
    /// Opens the screen that loads a game.
    pub const OPEN_LOAD_GAME: Self = Self(78);
    /// Opens the one that saves it.
    pub const OPEN_SAVE_GAME: Self = Self(79);
    /// Steps a value down.
    pub const STEP_DOWN: Self = Self(80);
    /// Steps it up.
    pub const STEP_UP: Self = Self(81);

    /// The parchment a list of things lays itself on.
    pub const SCROLL: Self = Self(82);
    /// Opens the help page a window offers.
    pub const HELP: Self = Self(83);

    /// The backdrop a panel of text lies on.
    pub const TEXT_BACKDROP: Self = Self(84);
    /// Moves a panel of text up.
    pub const LINE_UP: Self = Self(85);
    /// Moves it down.
    pub const LINE_DOWN: Self = Self(86);
    /// Takes it back to the top.
    pub const TO_THE_TOP: Self = Self(87);

    /// Raises a weight on a screen whose rows carry no picture of their own.
    pub const WEIGHT_UP: Self = Self(88);
    /// Lowers it.
    pub const WEIGHT_DOWN: Self = Self(89);
    /// Puts a screen's defaults back.
    pub const DEFAULTS: Self = Self(90);

    /// The backdrop the ring of node actions fills its client area with.
    pub const ACTION_BACKDROP: Self = Self(93);

    /// Sends the chosen row of a list to its front.
    pub const TO_THE_FRONT: Self = Self(91);
    /// Sends it to the back.
    pub const TO_THE_BACK: Self = Self(92);

    /// The first of the twelve pairs the tool orders draw, the one that raises first
    /// (`0x1fcf09`).
    pub const TOOL_FIRST: Self = Self(94);
    /// How many tools that screen shows.
    pub const TOOLS: u16 = 12;

    /// The first of the seven pairs the military settings draw, the one that raises first
    /// (`0x1fcc40`).
    pub const MILITARY_FIRST: Self = Self(Self::TOOL_FIRST.0 + 2 * Self::TOOLS);
    /// How many settings that screen shows.
    pub const MILITARY_SETTINGS: u16 = 7;

    /// The first of the eight the general statistics screen draws, in the order of the records
    /// at `0x1fd4b7`.
    pub const STATISTIC_FIRST: Self = Self(Self::MILITARY_FIRST.0 + 2 * Self::MILITARY_SETTINGS);
    /// How many statistics that screen shows.
    pub const STATISTICS: u16 = 8;

    /// Clears every group the ware counts screen draws.
    pub const CLEAR_GRAPH: Self = Self(Self::STATISTIC_FIRST.0 + Self::STATISTICS);

    /// Turns the stock screen to its other page.
    pub const TURN_PAGE: Self = Self(Self::CLEAR_GRAPH.0 + 1);

    /// Takes the open letter off the post office's list, the same item as [`Self::CLEAR_GRAPH`].
    pub const FORGET_LETTER: Self = Self(Self::TURN_PAGE.0 + 1);

    /// Opens the save screen from a reminder the post office shows.
    pub const TO_SAVE: Self = Self(Self::FORGET_LETTER.0 + 1);

    /// Steps the observation window's picture through its zoom factors.
    pub const ZOOM: Self = Self(Self::TO_SAVE.0 + 1);
    /// Moves its picture on to the next settler.
    pub const FOLLOW: Self = Self(Self::ZOOM.0 + 1);
    /// Steps the window itself through its three sizes.
    pub const OBSERVE_SIZE: Self = Self(Self::FOLLOW.0 + 1);

    /// The messages button while a letter is waiting, which the original swaps in at `0x44353`
    /// and swaps back out at `0x443e0`.
    pub const MESSAGES_WAITING: Self = Self(Self::OBSERVE_SIZE.0 + 1);

    /// A shipyard's button while it builds ships (`io200`).
    pub const SHIPS: Self = Self(Self::MESSAGES_WAITING.0 + 1);
    /// The same button while it builds boats (`io201`).
    pub const BOATS: Self = Self(Self::SHIPS.0 + 1);
    /// A workplace's switch while it works (`io178`).
    pub const PRODUCING: Self = Self(Self::BOATS.0 + 1);
    /// The switch while production is stopped (`io179`).
    pub const PRODUCTION_STOPPED: Self = Self(Self::PRODUCING.0 + 1);
    /// A military building's switch while coins come in (`io301`).
    pub const COINS_COMING: Self = Self(Self::PRODUCTION_STOPPED.0 + 1);
    /// The switch while they are stopped (`io300`).
    pub const COINS_STOPPED: Self = Self(Self::COINS_COMING.0 + 1);
    /// Picks the send out set for a storehouse's cells (`io193`).
    pub const TAKE_OUT: Self = Self(Self::COINS_STOPPED.0 + 1);
    /// Picks the stop set (`io194`).
    pub const STOP_STORAGE: Self = Self(Self::TAKE_OUT.0 + 1);
    /// Fills or clears the set being edited (`io205`).
    pub const SELECT_ALL: Self = Self(Self::STOP_STORAGE.0 + 1);
    /// Starts an expedition from a harbour (`io158`).
    pub const EXPEDITION: Self = Self(Self::SELECT_ALL.0 + 1);
    /// The mark on a cell whose stock is sent out (`io203`).
    pub const SENDING_OUT: Self = Self(Self::EXPEDITION.0 + 1);
    /// The mark on a cell that takes no more in (`io204`).
    pub const NOT_STORING: Self = Self(Self::SENDING_OUT.0 + 1);
    /// The post office's campaign button, which opens the small diary on the current task
    /// (`io061`).
    pub const CAMPAIGN_TASK: Self = Self(Self::NOT_STORING.0 + 1);
    /// What the buildings and production windows show for a type a campaign chapter has not
    /// given the player (`io170`).
    pub const LOCKED_TYPE: Self = Self(Self::CAMPAIGN_TASK.0 + 1);

    /// The first of the faces a player can take, in the order of the records at `0x1f7850`.
    pub const PORTRAIT_FIRST: Self = Self(Self::LOCKED_TYPE.0 + 1);
    /// How many faces there are.
    pub const PORTRAITS: u16 = 12;

    /// How many pictures are named.
    pub const COUNT: usize = (Self::PORTRAIT_FIRST.0 + Self::PORTRAITS) as usize;

    /// The picture the button of statistic `which` draws.
    #[must_use]
    pub const fn statistic(which: u16) -> Self {
        let which = if which < Self::STATISTICS {
            which
        } else {
            Self::STATISTICS - 1
        };
        Self(Self::STATISTIC_FIRST.0 + which)
    }

    /// The picture of face `which`, the last one for a face past the run.
    #[must_use]
    pub const fn portrait(which: u16) -> Self {
        let which = if which < Self::PORTRAITS {
            which
        } else {
            Self::PORTRAITS - 1
        };
        Self(Self::PORTRAIT_FIRST.0 + which)
    }

    /// The picture the button of tool `row` draws, the one that raises it when `more`.
    #[must_use]
    pub const fn tool(row: u16, more: bool) -> Self {
        Self::pair(Self::TOOL_FIRST, Self::TOOLS, row, more)
    }

    /// The picture the button of military setting `row` draws, the one that raises it when
    /// `more`.
    #[must_use]
    pub const fn military(row: u16, more: bool) -> Self {
        Self::pair(Self::MILITARY_FIRST, Self::MILITARY_SETTINGS, row, more)
    }

    /// One of a run of `rows` pairs that starts at `first`, the raising one first.
    const fn pair(first: Self, rows: u16, row: u16, more: bool) -> Self {
        let row = if row < rows { row } else { rows - 1 };
        Self(first.0 + 2 * row + if more { 0 } else { 1 })
    }

    /// Its place in a skin's table.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The picture named `name` in mods.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let index = NAMES.iter().position(|&known| known == name)?;
        u16::try_from(index).ok().map(Self)
    }
}

/// The name each picture goes by in mods, by [`Picture::index`]: the constant's name in snake
/// case, and for the runs, the tool, military setting or statistic it stands for.
pub const NAMES: [&str; Picture::COUNT] = [
    "confirm",
    "cancel",
    "go_to",
    "map_territory",
    "map_buildings",
    "map_roads",
    "map_size",
    "menu_title",
    "effects_on",
    "effects_off",
    "music_on",
    "music_off",
    "letters_all",
    "letters_military",
    "letters_geological",
    "letters_economy",
    "letters_various",
    "first",
    "previous",
    "next",
    "last",
    "open_distribution",
    "open_transport",
    "open_tools",
    "open_statistics",
    "open_merchandise",
    "open_buildings",
    "open_stock",
    "open_production",
    "open_military",
    "open_ships",
    "open_menu",
    "distribution_food",
    "distribution_grain",
    "distribution_iron",
    "distribution_coal",
    "distribution_boards",
    "distribution_water",
    "merchandise_food",
    "merchandise_weapons",
    "sector_north",
    "sector_north_east",
    "sector_south_east",
    "sector_south",
    "sector_south_west",
    "sector_north_west",
    "diary_backdrop",
    "note_backdrop",
    "headquarters",
    "build",
    "place_flag",
    "demolish",
    "dig_road",
    "attack",
    "window_menu",
    "flag",
    "landing_flag",
    "small_building",
    "medium_building",
    "large_building",
    "mine",
    "observe",
    "house_names",
    "go_home",
    "stop_road",
    "flag_menu",
    "road",
    "waterway",
    "flag_down",
    "geologist",
    "scout",
    "map_window",
    "main_selection",
    "construction_aid",
    "messages",
    "screen_backdrop",
    "open_keys",
    "open_readme",
    "open_load_game",
    "open_save_game",
    "step_down",
    "step_up",
    "scroll",
    "help",
    "text_backdrop",
    "line_up",
    "line_down",
    "to_the_top",
    "weight_up",
    "weight_down",
    "defaults",
    "to_the_front",
    "to_the_back",
    "action_backdrop",
    "tool_tongs_more",
    "tool_tongs_less",
    "tool_axe_more",
    "tool_axe_less",
    "tool_saw_more",
    "tool_saw_less",
    "tool_pick_axe_more",
    "tool_pick_axe_less",
    "tool_hammer_more",
    "tool_hammer_less",
    "tool_shovel_more",
    "tool_shovel_less",
    "tool_crucible_more",
    "tool_crucible_less",
    "tool_rod_and_line_more",
    "tool_rod_and_line_less",
    "tool_scythe_more",
    "tool_scythe_less",
    "tool_cleaver_more",
    "tool_cleaver_less",
    "tool_rolling_pin_more",
    "tool_rolling_pin_less",
    "tool_bow_more",
    "tool_bow_less",
    "military_recruiting_more",
    "military_recruiting_less",
    "military_rank_more",
    "military_rank_less",
    "military_defence_more",
    "military_defence_less",
    "military_attack_more",
    "military_attack_less",
    "military_occupation_centre_more",
    "military_occupation_centre_less",
    "military_occupation_middle_more",
    "military_occupation_middle_less",
    "military_occupation_border_more",
    "military_occupation_border_less",
    "statistic_land",
    "statistic_buildings",
    "statistic_people",
    "statistic_wares",
    "statistic_military",
    "statistic_gold",
    "statistic_productivity",
    "statistic_fights",
    "clear_graph",
    "turn_page",
    "forget_letter",
    "to_save",
    "zoom",
    "follow",
    "observe_size",
    "messages_waiting",
    "ships",
    "boats",
    "producing",
    "production_stopped",
    "coins_coming",
    "coins_stopped",
    "take_out",
    "stop_storage",
    "select_all",
    "expedition",
    "sending_out",
    "not_storing",
    "campaign_task",
    "locked_type",
    "portrait_0",
    "portrait_1",
    "portrait_2",
    "portrait_3",
    "portrait_4",
    "portrait_5",
    "portrait_6",
    "portrait_7",
    "portrait_8",
    "portrait_9",
    "portrait_10",
    "portrait_11",
];

/// The item each picture draws from in the shipped data, by [`Picture::index`].
///
/// A name here is the short form a control record carries, which the loader turns into the
/// index's own name ([`crate::items`]).
pub const ORIGINAL: [&str; Picture::COUNT] = [
    "io014",    // CONFIRM
    "io022",    // CANCEL
    "io089",    // GO_TO
    "io067",    // MAP_TERRITORY
    "io068",    // MAP_BUILDINGS
    "io069",    // MAP_ROADS
    "io090",    // MAP_SIZE
    "io012",    // MENU_TITLE
    "io096",    // EFFECTS_ON
    "io097",    // EFFECTS_OFF
    "io098",    // MUSIC_ON
    "io099",    // MUSIC_OFF
    "io172",    // LETTERS_ALL
    "io012",    // LETTERS_MILITARY, the same item as MENU_TITLE
    "io002",    // LETTERS_GEOLOGICAL
    "io010",    // LETTERS_ECONOMY
    "io171",    // LETTERS_VARIOUS
    "io084",    // FIRST
    "io085",    // PREVIOUS
    "io086",    // NEXT
    "io087",    // LAST
    "io116",    // OPEN_DISTRIBUTION
    "io180",    // OPEN_TRANSPORT
    "io119",    // OPEN_TOOLS
    "io148",    // OPEN_STATISTICS
    "io117",    // OPEN_MERCHANDISE
    "io114",    // OPEN_BUILDINGS
    "io196",    // OPEN_STOCK
    "io118",    // OPEN_PRODUCTION
    "io115",    // OPEN_MILITARY
    "io157",    // OPEN_SHIPS
    "io019",    // OPEN_MENU
    "io062",    // DISTRIBUTION_FOOD
    "io072",    // DISTRIBUTION_GRAIN
    "io063",    // DISTRIBUTION_IRON
    "io073",    // DISTRIBUTION_COAL
    "io064",    // DISTRIBUTION_BOARDS
    "io074",    // DISTRIBUTION_WATER
    "io062",    // MERCHANDISE_FOOD, the same item as DISTRIBUTION_FOOD
    "io093",    // MERCHANDISE_WEAPONS
    "io167",    // SECTOR_NORTH
    "io168",    // SECTOR_NORTH_EAST
    "io163",    // SECTOR_SOUTH_EAST
    "io164",    // SECTOR_SOUTH
    "io165",    // SECTOR_SOUTH_WEST
    "io166",    // SECTOR_NORTH_WEST
    "cast_01",  // DIARY_BACKDROP
    "cast_14",  // NOTE_BACKDROP
    "io000",    // HEADQUARTERS
    "io000",    // BUILD
    "io027",    // PLACE_FLAG
    "io005",    // DEMOLISH
    "io001",    // DIG_ROAD
    "io007",    // ATTACK
    "io018",    // WINDOW_MENU
    "io052",    // FLAG
    "io076",    // LANDING_FLAG
    "io049",    // SMALL_BUILDING
    "io050",    // MEDIUM_BUILDING
    "io051",    // LARGE_BUILDING
    "io058",    // MINE
    "io090",    // OBSERVE
    "io206",    // HOUSE_NAMES
    "io162",    // GO_HOME
    "io092",    // STOP_ROAD
    "io052",    // FLAG_MENU
    "io047",    // ROAD
    "io077",    // WATERWAY
    "io100",    // FLAG_DOWN
    "io002",    // GEOLOGIST
    "io078",    // SCOUT
    "io032",    // MAP_WINDOW
    "io174",    // MAIN_SELECTION
    "io065",    // CONSTRUCTION_AID
    "io044",    // MESSAGES
    "back_05",  // SCREEN_BACKDROP
    "io061",    // OPEN_KEYS
    "io061",    // OPEN_README, the same item as OPEN_KEYS
    "io030",    // OPEN_LOAD_GAME
    "io029",    // OPEN_SAVE_GAME
    "io020",    // STEP_DOWN
    "io021",    // STEP_UP
    "iobob_00", // SCROLL
    "io207",    // HELP
    "back_01",  // TEXT_BACKDROP
    "io015",    // LINE_UP
    "io016",    // LINE_DOWN
    "io017",    // TO_THE_TOP
    "io120",    // WEIGHT_UP
    "io121",    // WEIGHT_DOWN
    "io173",    // DEFAULTS
    "io197",    // TO_THE_FRONT
    "io198",    // TO_THE_BACK
    "back_01",  // ACTION_BACKDROP, the same item as TEXT_BACKDROP
    "io122",    // TOOL_FIRST, tongs
    "io123", "io126", // axe
    "io127", "io128", // saw
    "io129", "io130", // pick-axe
    "io131", "io124", // hammer
    "io125", "io132", // shovel
    "io133", "io134", // crucible
    "io135", "io136", // rod and line
    "io137", "io138", // scythe
    "io139", "io140", // cleaver
    "io141", "io142", // rolling pin
    "io143", "io144", // bow
    "io145", "io102", // MILITARY_FIRST, recruiting
    "io101", "io104", // rank
    "io103", "io106", // defenders
    "io105", "io192", // attackers
    "io191", "io112", // soldiers inland
    "io111", "io110", // soldiers in the middle
    "io109", "io108", // soldiers on the border
    "io107", "io149", // STATISTIC_FIRST, the land a player owns
    "io150", // the buildings standing
    "io151", // the people
    "io152", // the wares in store
    "io153", // the military strength
    "io154", // the gold
    "io155", // the average productivity
    "io199", // the fights won
    "io088", // CLEAR_GRAPH
    "io066", // TURN_PAGE
    "io088", // FORGET_LETTER, the same item as CLEAR_GRAPH
    "io156", // TO_SAVE
    "io018", // ZOOM
    "io025", // FOLLOW
    "io091", // OBSERVE_SIZE
    "io041", // MESSAGES_WAITING
    "io200", // SHIPS
    "io201", // BOATS
    "io178", // PRODUCING
    "io179", // PRODUCTION_STOPPED
    "io301", // COINS_COMING
    "io300", // COINS_STOPPED
    "io193", // TAKE_OUT
    "io194", // STOP_STORAGE
    "io205", // SELECT_ALL
    "io158", // EXPEDITION
    "io203", // SENDING_OUT
    "io204", // NOT_STORING
    "io061", // CAMPAIGN_TASK
    "io170", // LOCKED_TYPE
    "cast_30", "cast_18", "cast_19", // PORTRAIT_FIRST, the Romans' three
    "cast_28", "cast_23", "cast_22", // the Vikings'
    "cast_21", "cast_20", "cast_27", // the Japanese
    "cast_25", "cast_29", "cast_24", // the Africans'
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_names_an_item() {
        assert_eq!(ORIGINAL.len(), Picture::COUNT);
        for (place, name) in ORIGINAL.iter().enumerate() {
            assert!(
                name.is_ascii() && !name.is_empty(),
                "picture {place} names {name}"
            );
        }
    }

    #[test]
    fn every_picture_has_its_own_name() {
        assert_eq!(Picture::from_name("confirm"), Some(Picture::CONFIRM));
        assert_eq!(Picture::from_name("not_storing"), Some(Picture::NOT_STORING));
        assert_eq!(
            Picture::from_name("tool_tongs_more"),
            Some(Picture::tool(0, true))
        );
        assert_eq!(
            Picture::from_name("tool_bow_less"),
            Some(Picture::tool(Picture::TOOLS - 1, false))
        );
        assert_eq!(
            Picture::from_name("military_occupation_border_less"),
            Some(Picture::military(Picture::MILITARY_SETTINGS - 1, false))
        );
        assert_eq!(
            Picture::from_name("statistic_fights"),
            Some(Picture::statistic(Picture::STATISTICS - 1))
        );
        assert_eq!(Picture::from_name("clear_graph"), Some(Picture::CLEAR_GRAPH));
        for (index, name) in NAMES.iter().enumerate() {
            assert_eq!(
                Picture::from_name(name).map(Picture::index),
                Some(index),
                "{name}"
            );
        }
    }

    #[test]
    fn a_pictures_index_is_its_own_number() {
        assert_eq!(Picture::CONFIRM.index(), 0);
        assert_eq!(
            Picture::military(Picture::MILITARY_SETTINGS - 1, false).index() + 1,
            Picture::STATISTIC_FIRST.index()
        );
        assert_eq!(Picture::NOT_STORING.index() + 1, Picture::CAMPAIGN_TASK.index());
        assert_eq!(Picture::LOCKED_TYPE.index() + 1, Picture::PORTRAIT_FIRST.index());
        assert_eq!(
            Picture::portrait(Picture::PORTRAITS - 1).index(),
            Picture::COUNT - 1
        );
        assert_eq!(
            Picture::portrait(u16::MAX),
            Picture::portrait(Picture::PORTRAITS - 1)
        );
    }

    #[test]
    fn the_original_shares_two_items_between_four_pictures() {
        assert_eq!(
            ORIGINAL[Picture::MENU_TITLE.index()],
            ORIGINAL[Picture::LETTERS_MILITARY.index()]
        );
        assert_eq!(
            ORIGINAL[Picture::DISTRIBUTION_FOOD.index()],
            ORIGINAL[Picture::MERCHANDISE_FOOD.index()]
        );
        assert_eq!(
            ORIGINAL[Picture::CLEAR_GRAPH.index()],
            ORIGINAL[Picture::FORGET_LETTER.index()]
        );
    }
}
