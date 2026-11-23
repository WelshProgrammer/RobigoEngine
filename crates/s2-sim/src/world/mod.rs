//! What stands and walks on the map: the object word of every node, building and flag records,
//! and figures in per-node lists, built as `S2.EXE` builds them when a map loads and a game
//! starts.

use bitflags::bitflags;
use s2_formats::map::{LayerKind, Map};

use crate::carriers::Animation;
use crate::economy::EconomyTables;
use crate::figures::{Figure, Species};
use crate::grid::{Direction, Node};
use crate::homeward::Path;
use crate::job::Job;
use crate::military::{MILITARY_SETTINGS, Military, MilitaryTables};
use crate::nodes::NodeMap;
use crate::nodes::SiteFlags;
use crate::players::{PlayerSlots, StartOrder};
use crate::pool::Pool;
use crate::rng::GameRng;
use crate::sites::refresh_sites;
use crate::tables::{SimTables, WaypointSets};
use crate::territory::{border_pass, claim};
use crate::transport::{Distribution, Order, TRANSPORT_GROUPS, Ware};
use crate::ware::WareKind;
use s2_systems::{building_types, waypoints};

mod demolition;
mod growth;
mod links;
mod object;
mod roads;
mod storehouse;
pub(crate) use storehouse::TOOLS as JOB_TOOLS;

pub use building_types::{
    BUILDING_TYPES, BuildingKind, BuildingType, BuildingTypes, BuildingTypesError, Category,
    MAX_BUILDING_TYPES, Size,
};
pub use object::{
    Ageing, Burning, Decoration, Field, FieldStage, Granite, MapObject, ObjectWord, Remnant, RemnantLook,
    ShipSite, Sign, Tree,
};

pub use roads::{
    FLAG_CAPACITY, Flag, FlagKind, FlagLink, MAX_FLAGS, MAX_ROADS, Road, RoadError, connect_to_flag,
    disconnect_from_flag,
};
pub use s2_systems::player::Player;
pub use storehouse::{StartGoods, StockSetting, Storehouse, Taken, WARE_TYPES};
pub use waypoints::{WAYPOINTS, WaypointSet, WaypointSetError, Waypoints};

bitflags! {
    /// A building record's state flags (word `+7`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    pub struct BuildingState: u16 {
        /// The door is drawn.
        const DOOR = 1 << 0;
        /// A workplace has made a ware it has not carried out yet.
        const OUTPUT = 1 << 1;
        /// The builder, later the worker, is present. For a storehouse: its door is free, and it
        /// can send someone out (cleared while someone it sent walks to the flag).
        const PRESENT = 1 << 2;
        /// With [`Self::MANNED`], the work animation runs.
        const WORKING = 1 << 3;
        /// Small flags on military buildings and the headquarters, smoke and work animations.
        const MANNED = 1 << 4;
        /// A site waiting for its planer, or being levelled.
        const LEVELLING = 1 << 5;
        /// Under construction.
        const BUILDING = 1 << 6;
        /// Finished.
        const FINISHED = 1 << 8;
        /// An armoury's next ware is swords.
        const SWORDS_NEXT = 1 << 9;
        /// A storehouse's last send-out turn was for wares; the next is for settlers. The same
        /// bit as [`Self::SWORDS_NEXT`].
        const SENDING_WARES = 1 << 9;
        /// Production stopped by the player.
        const STOPPED = 1 << 10;
    }
}

impl BuildingState {
    /// A headquarters as placed at the start of a game, as found in savegames made a few seconds
    /// into a game. The state at placement itself is inferred to be the same.
    pub const NEW_HEADQUARTERS: Self = Self::FINISHED.union(Self::MANNED).union(Self::PRESENT);
}

/// A player's emergency programme (player block `+0x150` to `+0x155`): while it is on, boards
/// and stones go only to the first woodcutter and sawmill sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Emergency {
    /// Switched on (`+0x150`).
    pub on: bool,
    /// The woodcutter site that may still have boards (`+0x152`).
    pub woodcutter: Option<u16>,
    /// The sawmill site that may still have boards and stones (`+0x154`).
    pub sawmill: Option<u16>,
}

/// The most building records. The pool's real limit has not been read.
pub const MAX_BUILDINGS: usize = 65_534;

/// A building record (28 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Building {
    /// Building type (byte `+0`).
    pub kind: BuildingKind,
    /// Its flag (`+0x01`).
    pub flag: u16,
    /// Owner (byte `+3`).
    pub owner: Player,
    /// Wares ordered and not yet arrived (`+0x04`). On a site it only ever counts up: one for
    /// the builder order and one per construction ware ordered.
    pub requested: u8,
    /// The construction timer (`+0x05`): 1 after a ware is used, up by one per building tick,
    /// [`WAITING`] while the site lacks materials.
    pub timer: u8,
    /// State flags (`+0x07`).
    pub state: BuildingState,
    /// Its running animation (`+0x09`).
    pub animation: Option<u16>,
    /// A site's builder figure (`+0x0b`).
    pub builder: Option<u16>,
    /// The waypoint the builder heads for (`+0x0d`).
    pub waypoint: u8,
    /// Per input slot, wares on their way (high nibble of `+0x10` to `+0x13`). A site's slots 0
    /// and 1 are boards and stones, and count every one sent, arrived or not.
    pub on_way: [u8; 4],
    /// Per input slot, wares in stock (low nibble of `+0x10` to `+0x13`).
    pub stock: [u8; 4],
    /// A site's boards and stones used so far (`+0x14`, `+0x15`).
    pub used: [u8; 2],
    /// A workplace's wares made and not yet carried out (`+0x14` to `+0x17`, shared with
    /// [`Self::used`] on a site).
    pub outputs: [Option<WareKind>; 4],
    /// A gatherer that found nothing left to gather and has stopped looking (`+0x0b`).
    pub exhausted: bool,
    /// A shipyard building ships (`+0x0f` = 2); otherwise it builds boats.
    pub ships: bool,
    /// The node a shipyard builds its ship on (`+0x0b`, `+0x0d`).
    pub ship_site: Option<Node>,
    /// A workplace's productivity in percent (`+0x06`).
    pub productivity: u8,
    /// A workplace's work done since productivity was last rated (`+0x19`): 2 for each building
    /// tick at work, 1 for each with a ware waiting to go out.
    pub tally: u8,
    /// When it was first occupied, counting headquarters and military buildings from 0 in the
    /// order they were (`+0x1a`). Releasing land re-claims in this order.
    pub occupied: u16,
    /// A headquarters' soldier reserve, one entry per rank (`+0x14` to `+0x18`, the bytes a site
    /// uses for [`Self::used`] and a workplace for [`Self::outputs`]).
    pub reserve: [Reserve; Job::RANKS.len()],
    /// The node the building stands on.
    pub node: Node,
}

/// One rank of a headquarters' soldier reserve: soldiers kept out of the job counts, so that
/// nothing sends them to a military building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Reserve {
    /// How many to keep back, 0 to 15 (the byte's high nibble).
    pub wanted: u8,
    /// How many are kept back (the byte's low nibble).
    pub held: u8,
}

/// [`Building::timer`] on a site that has run out of materials.
pub const WAITING: u8 = u8::MAX;

/// How many finds a player's geologists can have reported.
pub const FINDS: usize = 5;

/// The word each sign kind sets, as the offsets `0x201892 + o` give them: gold first, then iron,
/// coal, granite and water.
const FOUND_WORD: [usize; FINDS] = [1, 0, 2, 3, 4];

impl Building {
    /// A new record of type `kind` for `owner` on `node` with its flag record `flag`, in
    /// `state`, with every count at 0 (`0x2c1b4`).
    #[must_use]
    pub const fn new(kind: BuildingKind, owner: Player, node: Node, flag: u16, state: BuildingState) -> Self {
        Self {
            kind,
            flag,
            owner,
            requested: 0,
            timer: 0,
            state,
            animation: None,
            builder: None,
            waypoint: 0,
            on_way: [0; 4],
            stock: [0; 4],
            used: [0; 2],
            outputs: [None; 4],
            reserve: [Reserve { wanted: 0, held: 0 }; Job::RANKS.len()],
            exhausted: false,
            ships: false,
            ship_site: None,
            productivity: 0,
            tally: 0,
            occupied: 0,
            node,
        }
    }

    /// Whether it is a building site: under construction, or waiting to be levelled.
    #[must_use]
    pub const fn is_site(&self) -> bool {
        self.state
            .intersects(BuildingState::BUILDING.union(BuildingState::LEVELLING))
    }
}

/// The figure records a new pool holds, how many it grows by when full, and the most it grows to
/// (`0x78668`, `0x78788`).
const FIGURE_ROOM: u16 = 2000;
const FIGURE_GROWTH: u16 = 500;
const FIGURE_LIMIT: u16 = 10_000;

/// Objects, buildings, flags, roads and figures of one map.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct World {
    width: u16,
    height: u16,
    objects: Vec<MapObject>,
    heads: Vec<Option<u16>>,
    buildings: Pool<Building>,
    flags: Pool<Flag>,
    roads: Pool<Road>,
    pub(crate) orders: Pool<Order>,
    stores: Pool<Storehouse>,
    pub(crate) animations: Pool<Animation>,
    pub(crate) wares: Pool<Ware>,
    pub(crate) priorities: [[u8; WARE_TYPES]; Player::COUNT],
    /// Each player's rank of each transport group (`+0x42f` of the settings record), 0 first.
    pub(crate) transport_ranks: [[u8; TRANSPORT_GROUPS]; Player::COUNT],
    pub(crate) paths: Pool<Path>,
    /// Whether a lost settler has looked for a way home since the last 24-step mark (`0x2016a8`).
    pub(crate) rescue_tried: bool,
    /// Each player's emergency programme for boards and stones.
    pub(crate) emergency: [Emergency; Player::COUNT],
    /// Each player's distribution settings.
    pub(crate) distribution: [Distribution; Player::COUNT],
    /// Each player's nation, 0 Romans to 3 Africans (`0x1f794f + 8·player`).
    pub(crate) nations: [u8; Player::COUNT],
    /// Who plays each player in the game and with which face (`0x1f794d`).
    pub(crate) slots: PlayerSlots,
    /// Who counts whom as an ally (`0x1f79eb`, one byte per ordered pair), set when the game
    /// starts and broken by an attack ([`crate::Alliances`]).
    pub(crate) allies: [[bool; Player::COUNT]; Player::COUNT],
    /// Each player's average productivity per building type (`0x203e22 + 0x2745·player + type`),
    /// which a new worker starts from.
    pub(crate) averages: [[u8; BUILDING_TYPES]; Player::COUNT],
    /// Each player's finished buildings of each type (`0x20178c + 0x2745·player + 2·type`).
    pub(crate) finished_buildings: [[u16; BUILDING_TYPES]; Player::COUNT],
    /// Each player's finished buildings added up (`0x2017cc + 0x2745·player`).
    pub(crate) all_finished: [u16; Player::COUNT],
    /// Each player's sites of each type under construction (`0x2017ce + 0x2745·player +
    /// 2·type`).
    pub(crate) building_sites: [[u16; BUILDING_TYPES]; Player::COUNT],
    /// Each player's sites added up (`0x20180e + 0x2745·player`).
    pub(crate) all_sites: [u16; Player::COUNT],
    /// How many people of each job each player holds (`0x201850 + 0x2745·player + 2·job`).
    pub(crate) people: [[u16; Job::PEOPLE]; Player::COUNT],
    /// Each player's people added up (`0x20188c + 0x2745·player`), which the original keeps as a
    /// word of its own.
    pub(crate) people_total: [u16; Player::COUNT],
    /// How many of each ware each player owns (`0x201810 + 0x2745·player + 2·ware`).
    pub(crate) owned_wares: [[u16; WARE_TYPES]; Player::COUNT],
    /// Each player's owned wares added up (`0x20184e + 0x2745·player`), which the original keeps
    /// as a word of its own.
    pub(crate) owned_total: [u16; Player::COUNT],
    /// Each player's count of each ware counted in since the last sample (`0x203e49 +
    /// 0x2745·player + 2·ware`), which the merchandise graph draws.
    pub(crate) ware_arrivals: [[u16; WARE_TYPES]; Player::COUNT],
    /// Each player's coins counted in over the game (`0x203e87 + 0x2745·player`), which the
    /// statistics screen draws as gold.
    pub(crate) coins_counted: [u32; Player::COUNT],
    /// Each player's fights won (`0x203e8b + 0x2745·player`).
    pub(crate) fights_won: [u32; Player::COUNT],
    /// Which finds each player's geologists have reported (`0x201892 + 0x2745·player`), by the
    /// word the mission triggers read rather than by the sign's own kind.
    pub(crate) found_signs: [[bool; FINDS]; Player::COUNT],
    /// Each player's messages (`[0x201648]`, one holder per player).
    pub(crate) messages: [crate::messages::Messages; Player::COUNT],
    /// Each player's series of samples (`0x2018a2 + 0x2745·player`).
    pub(crate) statistics: [crate::statistics::Statistics; Player::COUNT],
    /// The building type table.
    #[serde(skip)]
    building_types: BuildingTypes,
    /// The defaults behind the economy settings.
    #[serde(skip)]
    pub(crate) economy: EconomyTables,
    /// The defaults behind the military settings.
    #[serde(skip)]
    pub(crate) military_tables: MilitaryTables,
    /// Each player's military settings (`0x203e42 + 0x2745·player`).
    pub(crate) military_settings: [[u8; MILITARY_SETTINGS]; Player::COUNT],
    /// The military records (`0x1fa514`).
    pub(crate) military: Pool<Military>,
    /// Where the next military pass starts (pool header `+0x30`).
    pub(crate) military_cursor: u16,
    /// Catapult stones in flight, by record (`0x200c6a`).
    pub(crate) stones: Vec<Option<crate::catapult::Stone>>,
    /// The soldier records (`0x1fa532`).
    pub(crate) soldiers: Pool<crate::combat::SoldierRecord>,
    /// The harbour records (`0x1fa53c`).
    pub(crate) harbours: Pool<crate::harbours::Harbour>,
    /// The ship records (`0x1fa528`).
    pub(crate) ships: Pool<crate::ships::Ship>,
    /// Each player's ships launched so far (`0x203e8f + 0x2745·player`).
    pub(crate) ship_counts: [u16; Player::COUNT],
    /// What ships weigh wares and people by.
    #[serde(skip)]
    pub(crate) cargo_weights: crate::ships::CargoWeights,
    /// The fight and death animations.
    #[serde(skip)]
    pub(crate) fight_animations: crate::combat::FightAnimations,
    /// The occupation date the next building to be occupied takes (building pool header
    /// `+0x30`).
    pub(crate) occupations: u16,
    /// Each player's builder waypoints.
    #[serde(skip)]
    waypoints: [WaypointSet; Player::COUNT],
    /// The waypoint sets a player can be given, by nation byte.
    #[serde(skip)]
    waypoint_sets: WaypointSets,
    figures: Vec<Option<Figure>>,
    /// How many figure records the pool holds before it must grow (`0x78788`).
    figure_room: u16,
    /// Whether the figure pool grew since the figure loop started, which ends the loop.
    pub(crate) figures_moved: bool,
    /// Whether the copied engine bugs are fixed.
    pub(crate) bug_fixes: crate::game::BugFixes,
}

impl World {
    /// An empty `width` × `height` world: no objects, no figures. Either side must be non-zero.
    #[must_use]
    pub fn empty(width: u16, height: u16) -> Self {
        let count = usize::from(width) * usize::from(height);
        let mut world = Self {
            width,
            height,
            objects: vec![MapObject::Nothing; count],
            heads: vec![None; count],
            buildings: Pool::new(),
            flags: Pool::new(),
            roads: Pool::new(),
            orders: Pool::new(),
            stores: Pool::new(),
            animations: Pool::new(),
            wares: Pool::new(),
            priorities: [[0; WARE_TYPES]; Player::COUNT],
            transport_ranks: [[0; TRANSPORT_GROUPS]; Player::COUNT],
            paths: Pool::new(),
            rescue_tried: false,
            emergency: [Emergency::default(); Player::COUNT],
            averages: [[0; BUILDING_TYPES]; Player::COUNT],
            finished_buildings: [[0; BUILDING_TYPES]; Player::COUNT],
            all_finished: [0; Player::COUNT],
            building_sites: [[0; BUILDING_TYPES]; Player::COUNT],
            all_sites: [0; Player::COUNT],
            people: [[0; Job::PEOPLE]; Player::COUNT],
            people_total: [0; Player::COUNT],
            owned_wares: [[0; WARE_TYPES]; Player::COUNT],
            owned_total: [0; Player::COUNT],
            ware_arrivals: [[0; WARE_TYPES]; Player::COUNT],
            coins_counted: [0; Player::COUNT],
            fights_won: [0; Player::COUNT],
            found_signs: [[false; FINDS]; Player::COUNT],
            messages: std::array::from_fn(|_| crate::messages::Messages::default()),
            statistics: std::array::from_fn(|_| crate::statistics::Statistics::new()),
            nations: [0; Player::COUNT],
            slots: PlayerSlots::EMPTY,
            allies: [[false; Player::COUNT]; Player::COUNT],
            distribution: Default::default(),
            building_types: BuildingTypes::ORIGINAL,
            economy: EconomyTables::ORIGINAL,
            military_tables: MilitaryTables::ORIGINAL,
            military_settings: [MilitaryTables::ORIGINAL.settings(); Player::COUNT],
            military: Pool::new(),
            military_cursor: 0,
            stones: Vec::new(),
            soldiers: Pool::new(),
            harbours: Pool::new(),
            ships: Pool::new(),
            ship_counts: [0; Player::COUNT],
            cargo_weights: crate::ships::CargoWeights::ORIGINAL,
            fight_animations: crate::combat::FightAnimations::ORIGINAL,
            occupations: 0,
            waypoints: std::array::from_fn(|_| WaypointSet::default()),
            waypoint_sets: WaypointSets::default(),
            figures: Vec::new(),
            figure_room: FIGURE_ROOM,
            figures_moved: false,
            bug_fixes: crate::game::BugFixes::On,
        };
        world.reset_economy();
        world
    }

    /// The world as `S2.EXE`'s map loader leaves it (`0x14619`). Object words come from slots 4
    /// and 5 with the start-position markers (class 2) cleared. The figure lists start
    /// empty, then get one animal per animal-list record in file order through
    /// [`Species::from_map`], which draws from `rng` for each rabbit.
    ///
    /// Inferred: list records outside the map are placed on the wrapped node, as the layer
    /// accessors wrap.
    #[must_use]
    pub fn from_map(map: &Map, rng: &mut GameRng) -> Self {
        let mut world = Self::empty(map.width(), map.height());
        let low = map
            .layer(LayerKind::ObjectIndex)
            .map(s2_formats::map::Layer::data);
        let high = map.layer(LayerKind::ObjectType).map(s2_formats::map::Layer::data);
        if let (Some(low), Some(high)) = (low, high) {
            for ((object, &lo), &hi) in world.objects.iter_mut().zip(low).zip(high) {
                let value = MapObject::from(ObjectWord(u16::from_le_bytes([lo, hi])));
                // A map file's class 2 words mark start positions; they name no flag record.
                if !value.is_flag() {
                    *object = value;
                }
            }
        }
        for animal in map.animals() {
            let Some(species) = Species::from_map(animal.species, rng) else {
                continue;
            };
            let node = Node::wrapped(
                animal.position.x.into(),
                animal.position.y.into(),
                world.width,
                world.height,
            );
            world.add_figure(node, Figure::animal(species));
        }
        world
    }

    /// Nodes per row.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Rows.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    fn index(&self, node: Node) -> usize {
        let node = Node::wrapped(node.x.into(), node.y.into(), self.width, self.height);
        usize::from(node.y) * usize::from(self.width) + usize::from(node.x)
    }

    /// What stands on `node`, which wraps into the map.
    #[must_use]
    pub fn object(&self, node: Node) -> MapObject {
        self.objects.get(self.index(node)).copied().unwrap_or_default()
    }

    /// Replaces what stands on `node`.
    pub fn set_object(&mut self, node: Node, object: MapObject) {
        let at = self.index(node);
        if let Some(slot) = self.objects.get_mut(at) {
            *slot = object;
        }
    }

    /// How productive `kind` is for `player` on average (`0x29c65`), which the average pass
    /// stores every 400 steps. A type the pass leaves out stays 0.
    #[must_use]
    pub fn productivity(&self, player: Player, kind: BuildingKind) -> u8 {
        self.averages[player.index()]
            .get(kind.index())
            .copied()
            .unwrap_or_default()
    }

    /// How many finished buildings of `kind` `player` holds (`0x29aea`).
    #[must_use]
    pub fn finished_buildings(&self, player: Player, kind: BuildingKind) -> u16 {
        self.finished_buildings[player.index()]
            .get(kind.index())
            .copied()
            .unwrap_or_default()
    }

    /// How many finished buildings `player` holds in all (`0x29c32`).
    #[must_use]
    pub fn all_finished_buildings(&self, player: Player) -> u16 {
        self.all_finished[player.index()]
    }

    /// How many sites of `kind` `player` has under construction (`0x29b39`).
    #[must_use]
    pub fn building_sites(&self, player: Player, kind: BuildingKind) -> u16 {
        self.building_sites[player.index()]
            .get(kind.index())
            .copied()
            .unwrap_or_default()
    }

    /// How many sites `player` has under construction in all (`0x29c43`).
    #[must_use]
    pub fn all_building_sites(&self, player: Player) -> u16 {
        self.all_sites[player.index()]
    }

    /// How many of `ware` `player` has counted in since the last sample.
    #[must_use]
    pub fn ware_arrivals(&self, player: Player, ware: WareKind) -> u16 {
        self.ware_arrivals[player.index()]
            .get(ware.index())
            .copied()
            .unwrap_or_default()
    }

    /// How many coins `player` has counted in over the game, which the screens call gold.
    #[must_use]
    pub fn coins_counted(&self, player: Player) -> u32 {
        self.coins_counted[player.index()]
    }

    /// Counts `count` of `ware` in for `player` (`0x29953`), which happens as the ware is made:
    /// the count the player owns, the total, and the arrival counter the merchandise graph draws.
    /// Coins add to the gold count as well.
    pub(crate) fn count_ware(&mut self, player: Player, ware: WareKind, count: u16) {
        if let Some(owned) = self.owned_wares[player.index()].get_mut(ware.index()) {
            *owned = owned.wrapping_add(count);
            let total = &mut self.owned_total[player.index()];
            *total = total.wrapping_add(count);
        }
        if let Some(arrivals) = self.ware_arrivals[player.index()].get_mut(ware.index()) {
            *arrivals = arrivals.saturating_add(count);
        }
        if ware == WareKind::COINS {
            let coins = &mut self.coins_counted[player.index()];
            *coins = coins.saturating_add(u32::from(count));
        }
    }

    /// Forgets `player`'s arrivals of `ware` since the last sample, as stocking a headquarters
    /// does so that its wares never show as production.
    pub(crate) fn clear_arrivals(&mut self, player: Player, ware: WareKind) {
        if let Some(arrivals) = self.ware_arrivals[player.index()].get_mut(ware.index()) {
            *arrivals = 0;
        }
    }

    /// Whether `player`'s geologists have reported a find of sign kind `found`. Only the mission
    /// triggers read these words, and only for the first player.
    #[must_use]
    pub fn found_sign(&self, player: Player, found: u8) -> bool {
        FOUND_WORD
            .get(usize::from(found))
            .and_then(|&word| self.found_signs[player.index()].get(word))
            .copied()
            .unwrap_or_default()
    }

    /// Notes that `player` has had a find of sign kind `found` reported. A kind above the five
    /// the words cover sets nothing.
    pub(crate) fn note_found_sign(&mut self, player: Player, found: u8) {
        if let Some(&word) = FOUND_WORD.get(usize::from(found))
            && let Some(set) = self.found_signs[player.index()].get_mut(word)
        {
            *set = true;
        }
    }

    /// How many fights `player` has won (`0x29bf0`).
    #[must_use]
    pub fn fights_won(&self, player: Player) -> u32 {
        self.fights_won[player.index()]
    }

    /// Counts a won fight for `player` (`0x29c00`).
    pub(crate) fn count_fight_won(&mut self, player: Player) {
        let won = &mut self.fights_won[player.index()];
        *won = won.saturating_add(1);
    }

    /// Counts a finished building of `kind` for `player` (`0x29a9e`).
    pub(crate) fn count_building(&mut self, player: Player, kind: BuildingKind) {
        if let Some(count) = self.finished_buildings[player.index()].get_mut(kind.index()) {
            *count = count.saturating_add(1);
            let total = &mut self.all_finished[player.index()];
            *total = total.saturating_add(1);
        }
    }

    /// Takes a finished building of `kind` off `player`'s counts (`0x29aba`), and marks the
    /// player beaten when that was its last.
    pub(crate) fn uncount_building(&mut self, player: Player, kind: BuildingKind) {
        if let Some(count) = self.finished_buildings[player.index()].get_mut(kind.index()) {
            *count = count.saturating_sub(1);
            let total = &mut self.all_finished[player.index()];
            if *total > 0 {
                *total -= 1;
                if *total == 0 {
                    self.slots.beat(player);
                }
            }
        }
    }

    /// Counts a site of `kind` for `player` (`0x29b01`).
    pub(crate) fn count_site(&mut self, player: Player, kind: BuildingKind) {
        if let Some(count) = self.building_sites[player.index()].get_mut(kind.index()) {
            *count = count.saturating_add(1);
            let total = &mut self.all_sites[player.index()];
            *total = total.saturating_add(1);
        }
    }

    /// Takes a site of `kind` off `player`'s counts (`0x29b1d`).
    pub(crate) fn uncount_site(&mut self, player: Player, kind: BuildingKind) {
        if let Some(count) = self.building_sites[player.index()].get_mut(kind.index()) {
            *count = count.saturating_sub(1);
            let total = &mut self.all_sites[player.index()];
            *total = total.saturating_sub(1);
        }
    }

    /// Building record `record`.
    #[must_use]
    pub fn building(&self, record: u16) -> Option<&Building> {
        self.buildings.get(record)
    }

    /// The building records in use, in record order, with their numbers.
    pub fn buildings(&self) -> impl Iterator<Item = (u16, &Building)> {
        self.buildings.iter()
    }

    /// Ship record `record`.
    #[must_use]
    pub fn ship(&self, record: u16) -> Option<&crate::ships::Ship> {
        self.ships.get(record)
    }

    /// The ship records in use, in record order, with their numbers.
    pub fn ships(&self) -> impl Iterator<Item = (u16, &crate::ships::Ship)> {
        self.ships.iter()
    }

    /// Building record `record` to change.
    pub(crate) fn building_mut(&mut self, record: u16) -> Option<&mut Building> {
        self.buildings.get_mut(record)
    }

    /// Adds a building record in the lowest free slot and returns its number, or `None` when the
    /// pool is full. The object word is separate.
    pub fn add_building(&mut self, building: Building) -> Option<u16> {
        self.buildings.insert(building, MAX_BUILDINGS)
    }

    /// The building type table the game plays by.
    #[must_use]
    pub const fn building_types(&self) -> &BuildingTypes {
        &self.building_types
    }

    /// Plays by building type table `types` from now on.
    pub fn set_building_types(&mut self, types: BuildingTypes) {
        self.building_types = types;
    }

    /// The occupation date for the next building to be occupied, counting on (`0x2c30a`,
    /// `0x35d1f`).
    pub(crate) fn next_occupation(&mut self) -> u16 {
        let next = self.occupations;
        self.occupations = self.occupations.wrapping_add(1);
        next
    }

    /// The defaults behind the economy settings.
    #[must_use]
    pub const fn economy_tables(&self) -> &EconomyTables {
        &self.economy
    }

    /// Plays by economy tables `tables` from now on, giving every player the settings they start
    /// a game with.
    pub fn set_economy_tables(&mut self, tables: EconomyTables) {
        self.economy = tables;
        self.reset_economy();
    }

    /// Plays by `tables` from now on, giving every player the economy and military settings
    /// they start a game with.
    pub fn set_tables(&mut self, tables: &SimTables) {
        self.building_types.clone_from(&tables.building_types);
        self.set_economy_tables(tables.economy.clone());
        self.military_tables.clone_from(&tables.military);
        self.military_settings = [tables.military.settings(); Player::COUNT];
        self.waypoint_sets.0.clone_from(&tables.waypoints);
        self.cargo_weights.clone_from(&tables.cargo);
        self.fight_animations.clone_from(&tables.fights);
    }

    /// Waypoint set `index` of the ones a player can be given, or `None` past them.
    #[must_use]
    pub fn waypoint_set(&self, index: usize) -> Option<&WaypointSet> {
        self.waypoint_sets.0.get(index)
    }

    /// Takes `other`'s tables and each player's waypoints.
    pub(crate) fn adopt_tables(&mut self, other: &Self) {
        self.building_types.clone_from(&other.building_types);
        self.economy.clone_from(&other.economy);
        self.military_tables.clone_from(&other.military_tables);
        self.fight_animations.clone_from(&other.fight_animations);
        self.cargo_weights.clone_from(&other.cargo_weights);
        self.waypoints.clone_from(&other.waypoints);
        self.waypoint_sets.clone_from(&other.waypoint_sets);
    }

    /// The waypoints `player`'s builders walk between.
    #[must_use]
    pub fn waypoints(&self, player: Player) -> &WaypointSet {
        &self.waypoints[player.index()]
    }

    /// Gives `player`'s builders waypoint set `set`; every player starts with
    /// [`WaypointSet::default`], the first original set.
    pub fn set_waypoints(&mut self, player: Player, set: WaypointSet) {
        self.waypoints[player.index()] = set;
    }

    /// The building on the node north-west of flag `flag` (`0x260d5`).
    #[must_use]
    pub fn building_at_flag(&self, flag: u16) -> Option<u16> {
        let node = self
            .flag(flag)?
            .node
            .neighbour(Direction::NorthWest, self.width, self.height);
        self.object(node).building()
    }

    /// Figure record `record`, if it is in use.
    #[must_use]
    pub fn figure(&self, record: u16) -> Option<&Figure> {
        self.figures.get(usize::from(record))?.as_ref()
    }

    /// Running animation `record`, if it is in use.
    #[must_use]
    pub fn animation(&self, record: u16) -> Option<&crate::carriers::Animation> {
        self.animations.get(record)
    }

    pub(crate) fn figure_mut(&mut self, record: u16) -> Option<&mut Figure> {
        self.figures.get_mut(usize::from(record))?.as_mut()
    }

    /// The figure records in use, in record order, with their numbers.
    pub fn figures(&self) -> impl Iterator<Item = (u16, &Figure)> {
        (0_u16..)
            .zip(&self.figures)
            .filter_map(|(record, figure)| Some((record, figure.as_ref()?)))
    }

    /// One past the highest record number ever handed out and not yet given back from the
    /// top: the figure loop's range.
    pub(crate) fn figure_records(&self) -> u16 {
        u16::try_from(self.figures.len()).unwrap_or(u16::MAX)
    }

    /// The figures on `node`, head first: the one that arrived last comes first and is drawn
    /// under the others.
    pub fn figures_at(&self, node: Node) -> impl Iterator<Item = (u16, &Figure)> {
        let mut next = self.heads.get(self.index(node)).copied().flatten();
        std::iter::from_fn(move || {
            let record = next?;
            let figure = self.figure(record)?;
            next = figure.next;
            Some((record, figure))
        })
    }

    /// Whether any figure stands on or walks to `node`.
    #[must_use]
    pub fn has_figures(&self, node: Node) -> bool {
        matches!(self.heads.get(self.index(node)), Some(Some(_)))
    }

    /// Adds `figure` on `node` in the lowest free record (`0x78788`) and pushes it at the head
    /// of the node's list (`0x32994`). Returns its record number.
    pub fn add_figure(&mut self, node: Node, mut figure: Figure) -> u16 {
        figure.node = node;
        let record = self.add_loose_figure(figure);
        self.push(record, node);
        record
    }

    /// Adds `figure` in the lowest free record without putting it on any node's list, as
    /// `0x3340c` makes a builder, growing the pool when it is full. Returns its record number.
    pub(crate) fn add_loose_figure(&mut self, figure: Figure) -> u16 {
        let record = if let Some(free) = self.figures.iter().position(Option::is_none) {
            free
        } else {
            self.figures.push(None);
            self.figures.len() - 1
        };
        if record >= usize::from(self.figure_room) {
            self.figure_room = (self.figure_room + FIGURE_GROWTH).min(FIGURE_LIMIT);
            self.figures_moved = true;
        }
        self.figures[record] = Some(figure);
        u16::try_from(record).unwrap_or(u16::MAX)
    }

    /// Puts `figure` in free record `record` without putting it on any node's list. Nothing
    /// happens unless the record is free and below the highest record in use, the range the
    /// figure loop runs. Returns whether it went in.
    pub(crate) fn place_loose_figure(&mut self, record: u16, figure: Figure) -> bool {
        match self.figures.get_mut(usize::from(record)) {
            Some(slot @ None) => {
                *slot = Some(figure);
                true
            }
            _ => false,
        }
    }

    /// Frees figure `record`, which is on no node's list, straight back to the pool (`0x788d0`).
    pub(crate) fn free_loose_figure(&mut self, record: u16) {
        if let Some(slot) = self.figures.get_mut(usize::from(record)) {
            *slot = None;
        }
        while self.figures.last().is_some_and(Option::is_none) {
            self.figures.pop();
        }
    }

    /// Takes figure `record` off its node and frees the record (`0x33104` for animals).
    pub fn remove_figure(&mut self, record: u16) {
        self.unlink(record);
        if let Some(slot) = self.figures.get_mut(usize::from(record)) {
            *slot = None;
        }
        while self.figures.last().is_some_and(Option::is_none) {
            self.figures.pop();
        }
    }

    /// Moves figure `record` from its node's list to the head of `to`'s (`0x32a14`, then
    /// `0x32994`).
    pub(crate) fn move_figure(&mut self, record: u16, to: Node) {
        self.unlink(record);
        self.push(record, to);
    }

    /// Removes every figure.
    pub fn clear_figures(&mut self) {
        self.figures.clear();
        self.heads.fill(None);
    }

    fn push(&mut self, record: u16, node: Node) {
        let at = self.index(node);
        let head = self.heads[at];
        if let Some(figure) = self.figure_mut(record) {
            figure.node = node;
            figure.next = head;
            self.heads[at] = Some(record);
        }
    }

    fn unlink(&mut self, record: u16) {
        let Some(figure) = self.figure(record) else {
            return;
        };
        let (at, next) = (self.index(figure.node), figure.next);
        if self.heads[at] == Some(record) {
            self.heads[at] = next;
            return;
        }
        let mut previous = self.heads[at];
        while let Some(current) = previous {
            let Some(figure) = self.figure_mut(current) else {
                return;
            };
            if figure.next == Some(record) {
                figure.next = next;
                return;
            }
            previous = figure.next;
        }
    }

    /// Starts a game on `map` as `S2.EXE` starts an Unlimited Play game. Every node's owner and
    /// strength are cleared (`0x3e078`), then the players `slots` puts in the game take the
    /// start positions `order` sets out, lowest player first ([`StartOrder::arrange`]), and each
    /// gets a headquarters there, placed in turn with [`Self::place_headquarters`] and stocked
    /// with `goods`. A player left without a position is taken out of the game, where `S2.EXE`
    /// reads past the end of its list. Returns how many were placed; maps without a header
    /// place none.
    pub fn start_game(
        &mut self,
        map: &Map,
        nodes: &mut NodeMap,
        (slots, order): (&PlayerSlots, StartOrder),
        goods: StartGoods,
        rng: &mut GameRng,
    ) -> usize {
        self.slots = PlayerSlots::EMPTY;
        let Some(header) = map.header() else {
            return 0;
        };
        Self::clear_owners(nodes);
        let starts = order.arrange(header.headquarters(), rng);
        let mut placed = 0;
        for (player, position) in slots.players().zip(starts) {
            let Some(slot) = slots.get(player) else {
                continue;
            };
            self.slots.set(player, Some(slot));
            let node = Node {
                x: position.x,
                y: position.y,
            };
            self.place_headquarters(nodes, node, player, goods);
            placed += 1;
        }
        placed
    }

    /// Seats `slots` and gives each player with a start position in `map`'s header an empty
    /// headquarters there, where one fits, from player 0 up (`0x108ee`). A campaign chapter
    /// starts this way, in the game or not, with no shuffle. `S2.EXE` also tries an eighth slot
    /// at (0, 0), which has no player here.
    pub(crate) fn start_campaign(&mut self, map: &Map, nodes: &mut NodeMap, slots: &PlayerSlots) {
        self.slots = *slots;
        Self::clear_owners(nodes);
        let Some(header) = map.header() else {
            return;
        };
        for (player, start) in Player::ALL.into_iter().zip(header.headquarters()) {
            if let Some(start) = start {
                let node = Node {
                    x: start.x,
                    y: start.y,
                };
                if self.headquarters_fit(nodes, node) {
                    self.place_headquarters_with(nodes, node, player, None);
                }
            }
        }
    }

    /// Gives every node back to nobody, as a new game does before the headquarters go down.
    fn clear_owners(nodes: &mut NodeMap) {
        for y in 0..nodes.height() {
            for x in 0..nodes.width() {
                let node = Node { x, y };
                let mut data = nodes.get(node);
                data.owner = None;
                data.strength = 0;
                nodes.set(node, data);
            }
        }
    }

    /// Who plays each player in the game and with which face.
    #[must_use]
    pub fn slots(&self) -> &PlayerSlots {
        &self.slots
    }

    /// Seats the players `slots` describes, as the setup screen does before a game starts. It
    /// places nothing: [`Self::start_game`] is what gives each one a headquarters.
    pub fn set_slots(&mut self, slots: PlayerSlots) {
        self.slots = slots;
    }

    /// Places `owner`'s headquarters on `node` as a game start does (`0x26221`), with every
    /// slot 8 recompute in the game's order:
    ///
    /// 1. a type 1 flag one step south-east, then two rings around it recomputed, twice;
    /// 2. a finished building of type [`BuildingKind::HEADQUARTERS`] in state
    ///    [`BuildingState::NEW_HEADQUARTERS`], with a storehouse record stocked with `goods` and
    ///    the large footprint on its west, north-west and north-east neighbours;
    /// 3. its territory claimed from the type's strength (10, holding nine rings) and the border
    ///    pass around it at that radius;
    /// 4. the road to its flag, which recomputes the flag's ring;
    /// 5. two rings around the building recomputed, twice.
    ///
    /// Records take the lowest free slots. Nothing is placed when the flag pool is full.
    pub fn place_headquarters(&mut self, nodes: &mut NodeMap, node: Node, owner: Player, goods: StartGoods) {
        self.place_headquarters_with(nodes, node, owner, Some(goods));
    }

    /// Whether a headquarters fits on `node` (`0x198a3`): its ten protected nodes all belong to
    /// nobody, and slot 8 offers at least a small building on the node itself.
    #[must_use]
    pub fn headquarters_fit(&self, nodes: &NodeMap, node: Node) -> bool {
        crate::territory::protected_nodes((self.width, self.height), node)
            .into_iter()
            .all(|near| nodes.get(near).owner.is_none())
            && crate::construction::fits(Size::Small, nodes.get(node).sites.size)
    }

    /// [`Self::place_headquarters`], stocked from `goods`, or left empty with `None` as a
    /// campaign chapter's headquarters is.
    pub(crate) fn place_headquarters_with(
        &mut self,
        nodes: &mut NodeMap,
        node: Node,
        owner: Player,
        goods: Option<StartGoods>,
    ) {
        let flag_node = node.neighbour(Direction::SouthEast, self.width, self.height);
        let Some(flag) = self.add_flag(Flag::new(FlagKind::Normal, owner, flag_node)) else {
            return;
        };
        self.set_object(flag_node, MapObject::Flag(flag));
        for _ in 0..2 {
            refresh_sites(nodes, self, flag_node, 2);
        }

        let mut headquarters = Building::new(
            BuildingKind::HEADQUARTERS,
            owner,
            node,
            flag,
            BuildingState::NEW_HEADQUARTERS,
        );
        headquarters.occupied = self.next_occupation();
        headquarters.reserve = [Reserve {
            wanted: crate::military::RESERVE_WANTED,
            held: 0,
        }; Job::RANKS.len()];
        let Some(building) = self.add_building(headquarters) else {
            return;
        };
        if let Some(store) = self.add_storehouse(Storehouse::new(building))
            && let Some(goods) = goods
            && let Some(store) = self.storehouse_mut(store)
        {
            store.stock_headquarters(goods);
            let stocked = store.jobs;
            let wares = store.wares;
            for (index, count) in stocked.iter().enumerate() {
                let job = Job(u8::try_from(index).unwrap_or_default());
                self.count_people(owner, job, *count);
            }
            for (index, count) in wares.iter().enumerate() {
                let ware = WareKind(u8::try_from(index).unwrap_or_default());
                self.count_ware(owner, ware, *count);
                self.clear_arrivals(owner, ware);
            }
        }
        self.set_object(node, MapObject::Building(building));
        self.count_building(owner, BuildingKind::HEADQUARTERS);
        for direction in LARGE_FOOTPRINT {
            let at = node.neighbour(direction, self.width, self.height);
            let mut data = nodes.get(at);
            data.sites.flags |= SiteFlags::FOOTPRINT;
            nodes.set(at, data);
        }
        let strength = self
            .building_types()
            .get(BuildingKind::HEADQUARTERS)
            .map_or(0, |row| row.strength);
        claim(nodes, node, strength, owner);
        border_pass(nodes, self, node, u16::from(strength));
        let sight = self
            .building_types()
            .get(BuildingKind::HEADQUARTERS)
            .map_or(0, |row| row.sight);
        nodes.reveal(node, owner, u16::from(sight));

        connect_to_flag(nodes, self, node);
        for _ in 0..2 {
            refresh_sites(nodes, self, node, 2);
        }
    }
}

/// The neighbours a large building also covers (`0x2fdd8`).
pub(crate) const LARGE_FOOTPRINT: [Direction; 3] =
    [Direction::West, Direction::NorthWest, Direction::NorthEast];

#[cfg(test)]
pub(crate) mod tests;
