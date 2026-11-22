//! The building type table.
//!
//! [`BuildingTypes::ORIGINAL`] holds the rows of `S2.EXE`'s table at `0x1faa34` (27 bytes
//! per type; Gold has the same bytes). [`BuildingTypes::new`] builds other tables, for tests or
//! mods, without touching the original.

use std::borrow::Cow;

use crate::ware::WareKind;

/// The number of building types in the original table.
pub const BUILDING_TYPES: usize = 32;

/// The most types a table can hold: a building record keeps its type in one byte.
pub const MAX_BUILDING_TYPES: usize = 256;

/// A building type: the row of the building type table it uses (byte `+0` of a building record).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct BuildingKind(pub u8);

impl BuildingKind {
    /// Woodcutter's hut.
    pub const WOODCUTTER: Self = Self(0);
    /// Forester's hut.
    pub const FORESTER: Self = Self(1);
    /// Quarry.
    pub const QUARRY: Self = Self(2);
    /// Fishery.
    pub const FISHERY: Self = Self(3);
    /// Hunter's hut.
    pub const HUNTER: Self = Self(4);
    /// Sawmill.
    pub const SAWMILL: Self = Self(5);
    /// Farm.
    pub const FARM: Self = Self(6);
    /// Pig farm.
    pub const PIG_FARM: Self = Self(7);
    /// Donkey breeder.
    pub const DONKEY_BREEDER: Self = Self(8);
    /// Slaughterhouse.
    pub const SLAUGHTERHOUSE: Self = Self(9);
    /// Mill.
    pub const MILL: Self = Self(10);
    /// Bakery.
    pub const BAKERY: Self = Self(11);
    /// Iron smelter.
    pub const IRON_SMELTER: Self = Self(12);
    /// Metalworks.
    pub const METALWORKS: Self = Self(13);
    /// Armoury.
    pub const ARMOURY: Self = Self(14);
    /// Mint.
    pub const MINT: Self = Self(15);
    /// Shipyard.
    pub const SHIPYARD: Self = Self(16);
    /// Brewery.
    pub const BREWERY: Self = Self(17);
    /// Gold mine.
    pub const GOLD_MINE: Self = Self(18);
    /// Iron mine.
    pub const IRON_MINE: Self = Self(19);
    /// Coal mine.
    pub const COAL_MINE: Self = Self(20);
    /// Granite mine.
    pub const GRANITE_MINE: Self = Self(21);
    /// Storehouse.
    pub const STOREHOUSE: Self = Self(22);
    /// Harbour (inferred): the large storehouse type that is never levelled.
    pub const HARBOUR: Self = Self(23);
    /// Headquarters.
    pub const HEADQUARTERS: Self = Self(24);
    /// Barracks.
    pub const BARRACKS: Self = Self(25);
    /// Guardhouse.
    pub const GUARDHOUSE: Self = Self(26);
    /// Watchtower.
    pub const WATCHTOWER: Self = Self(27);
    /// Fortress.
    pub const FORTRESS: Self = Self(28);
    /// Lookout tower.
    pub const LOOKOUT_TOWER: Self = Self(29);
    /// Well.
    pub const WELL: Self = Self(30);
    /// Catapult.
    pub const CATAPULT: Self = Self(31);

    /// The mines, gold to granite.
    pub const MINES: [Self; 4] = [
        Self::GOLD_MINE,
        Self::IRON_MINE,
        Self::COAL_MINE,
        Self::GRANITE_MINE,
    ];

    /// Whether this is one of the four mines.
    #[must_use]
    pub const fn is_mine(self) -> bool {
        self.0 >= Self::GOLD_MINE.0 && self.0 <= Self::GRANITE_MINE.0
    }

    /// Whether this is a gatherer, woodcutter to hunter or the farm, whose worker brings its ware
    /// out itself (`0x2d429`).
    #[must_use]
    pub const fn is_gatherer(self) -> bool {
        self.0 <= Self::HUNTER.0 || self.0 == Self::FARM.0
    }

    /// Whether this is a storehouse type, category 2 of the building type table: storehouse,
    /// harbour or headquarters.
    #[must_use]
    pub const fn is_storehouse(self) -> bool {
        self.0 >= Self::STOREHOUSE.0 && self.0 <= Self::HEADQUARTERS.0
    }

    /// The name each building type goes by in mods, by type.
    pub const NAMES: [&'static str; BUILDING_TYPES] = [
        "woodcutter",
        "forester",
        "quarry",
        "fishery",
        "hunter",
        "sawmill",
        "farm",
        "pig_farm",
        "donkey_breeder",
        "slaughterhouse",
        "mill",
        "bakery",
        "iron_smelter",
        "metalworks",
        "armoury",
        "mint",
        "shipyard",
        "brewery",
        "gold_mine",
        "iron_mine",
        "coal_mine",
        "granite_mine",
        "storehouse",
        "harbour",
        "headquarters",
        "barracks",
        "guardhouse",
        "watchtower",
        "fortress",
        "lookout_tower",
        "well",
        "catapult",
    ];

    /// The building type named `name` in mods.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let index = Self::NAMES.iter().position(|&known| known == name)?;
        u8::try_from(index).ok().map(Self)
    }

    /// The name the building type goes by in mods, or `None` for one without a name.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        Self::NAMES.get(self.index()).copied()
    }

    /// The type's row in tables indexed by building type.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// What kind of building a type is (byte `+0x00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Category {
    /// 1: a building with a worker that makes or gathers something.
    Workplace,
    /// 2: a storehouse, harbour or headquarters.
    Storehouse,
    /// 3: a military building.
    Military,
}

/// How much room a type needs (byte `+0x16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Size {
    /// 1.
    Small,
    /// 2.
    Medium,
    /// 3: on a mountain.
    Mine,
    /// 4.
    Large,
}

/// One row of the type table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BuildingType {
    /// What kind of building it is.
    pub category: Category,
    /// Its graphics group `g` (word `+0x01`): the renderer draws it from items 250 + 5·g to
    /// 254 + 5·g of the owner's nation archive.
    pub group: u16,
    /// The job of its worker (byte `+0x03`).
    pub worker: Option<Job>,
    /// Boards it takes to build (byte `+0x10`).
    pub boards: u8,
    /// Stones it takes to build (byte `+0x11`).
    pub stones: u8,
    /// How much room it needs.
    pub size: Size,
    /// Whether a site on uneven ground is levelled before building. `S2.EXE` levels medium and
    /// large types other than the harbour (`0x2c1b4`).
    pub levelled: bool,
    /// The ware a work cycle makes (byte `+0x04`).
    pub output: Option<WareKind>,
    /// Building ticks a work cycle takes (byte `+0x07`).
    pub duration: u8,
    /// The wares a work cycle uses, one of each, by input slot (bytes `+0x08` to `+0x0b`).
    pub inputs: [Option<WareKind>; 4],
    /// How many of each input the building stocks (bytes `+0x0c` to `+0x0f`).
    pub capacity: [u8; 4],
    /// Whether its productivity is rated (word `+0x19` non-zero): a new worker starts from the
    /// player's average for the type.
    pub rated: bool,
    /// Whether its window offers to stop production once it is finished. `S2.EXE` offers it on
    /// workplaces that make a ware, and on the forester (`0x56211`).
    pub stoppable: bool,
    /// How far round it its owner sees once it claims land or, for the lookout tower, once it
    /// is manned (byte `+0x13`).
    pub sight: u8,
    /// The territory strength its claim starts from (byte `+0x12`): it holds one ring less than
    /// this. 0 for a type that claims nothing.
    pub strength: u8,
    /// How many soldiers a military type holds (byte `+0x15`).
    pub soldiers: u8,
}

impl Size {
    /// The size class byte (`+0x16`): 1 small, 2 medium, 3 mine, 4 large.
    #[must_use]
    pub const fn class(self) -> u8 {
        match self {
            Self::Small => 1,
            Self::Medium => 2,
            Self::Mine => 3,
            Self::Large => 4,
        }
    }
}

impl BuildingType {
    /// Wares of construction index `index` it takes: 0 boards, 1 stones.
    #[must_use]
    pub const fn cost(self, index: usize) -> u8 {
        if index == 0 { self.boards } else { self.stones }
    }
}

/// The rows of a building type table, indexed by type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingTypes {
    types: Cow<'static, [BuildingType]>,
}

/// Why custom building types were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BuildingTypesError {
    /// More rows than a type byte can name.
    #[error("{0} building types, more than {MAX_BUILDING_TYPES}")]
    TooMany(usize),
}

impl BuildingTypes {
    /// The table of `S2.EXE` (and Gold, which has the same bytes).
    pub const ORIGINAL: Self = Self {
        types: Cow::Borrowed(&TYPES),
    };

    /// A custom table: `types[t]` describes building type `t`.
    ///
    /// # Errors
    ///
    /// [`BuildingTypesError::TooMany`] past [`MAX_BUILDING_TYPES`] rows.
    pub fn new(types: Vec<BuildingType>) -> Result<Self, BuildingTypesError> {
        if types.len() > MAX_BUILDING_TYPES {
            return Err(BuildingTypesError::TooMany(types.len()));
        }
        Ok(Self {
            types: Cow::Owned(types),
        })
    }

    /// The row for type `kind`, or `None` past the table.
    #[must_use]
    pub fn get(&self, kind: BuildingKind) -> Option<BuildingType> {
        self.types.get(kind.index()).copied()
    }

    /// Every row, indexed by type.
    #[must_use]
    pub fn types(&self) -> &[BuildingType] {
        &self.types
    }
}

impl Default for BuildingTypes {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl std::hash::Hash for BuildingTypes {
    /// The table is input data, checked by the data hash a replay carries; the state hash leaves
    /// it out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

const fn row(
    category: Category,
    group: u16,
    worker: Option<Job>,
    boards: u8,
    stones: u8,
    size: Size,
) -> BuildingType {
    BuildingType {
        category,
        group,
        worker,
        boards,
        stones,
        size,
        levelled: matches!(size, Size::Medium | Size::Large),
        output: None,
        duration: 0,
        inputs: [None; 4],
        capacity: [0; 4],
        rated: false,
        stoppable: false,
        sight: 0,
        strength: 0,
        soldiers: 0,
    }
}

/// A workplace row: graphics group, worker, boards, stones and size.
const fn workplace(group: u16, worker: Job, boards: u8, stones: u8, size: Size) -> BuildingType {
    row(Workplace, group, Some(worker), boards, stones, size)
}

/// A storehouse row, with no worker.
const fn storehouse(group: u16, boards: u8, stones: u8, size: Size) -> BuildingType {
    row(Storehouse, group, None, boards, stones, size)
}

/// A military row: privates man it, `soldiers` of them at most, and it stocks coins, which the
/// player can stop.
const fn military(group: u16, boards: u8, stones: u8, size: Size, coins: u8, soldiers: u8) -> BuildingType {
    let mut row =
        row(Military, group, Some(Job::PRIVATE), boards, stones, size).uses(&[(WareKind::COINS, coins)]);
    row.soldiers = soldiers;
    row.stoppable = true;
    row
}

impl BuildingType {
    /// The row with a work cycle of `duration` building ticks.
    const fn cycle(mut self, duration: u8) -> Self {
        self.duration = duration;
        self
    }

    /// The row with `ware` as what a work cycle makes, which production can be stopped for.
    const fn makes(mut self, ware: WareKind) -> Self {
        self.output = Some(ware);
        self.stoppable = true;
        self
    }

    /// The row whose claim starts from strength `strength`.
    const fn claims(mut self, strength: u8) -> Self {
        self.strength = strength;
        self
    }

    /// The row whose owner sees `radius` rings round it.
    const fn sees(mut self, radius: u8) -> Self {
        self.sight = radius;
        self
    }

    /// The row with production that can be stopped.
    const fn stoppable(mut self) -> Self {
        self.stoppable = true;
        self
    }

    /// The row with its inputs and how many of each it stocks, by input slot.
    const fn uses(mut self, inputs: &[(WareKind, u8)]) -> Self {
        let mut slot = 0;
        while slot < inputs.len() {
            self.inputs[slot] = Some(inputs[slot].0);
            self.capacity[slot] = inputs[slot].1;
            slot += 1;
        }
        self
    }

    /// The row with its productivity rated.
    const fn rated(mut self) -> Self {
        self.rated = true;
        self
    }
}

/// The harbour's row: large, never levelled.
const fn harbour() -> BuildingType {
    let mut row = storehouse(HARBOUR_GROUP, 4, 6, Large);
    row.levelled = false;
    row
}

/// The harbour's graphics group.
const HARBOUR_GROUP: u16 = 39;

use crate::job::Job;
use Category::{Military, Storehouse, Workplace};
use Size::{Large, Medium, Mine, Small};
use WareKind as W;

/// What a mine eats: fish, bread and meat, two of each.
const MINE_FOOD: [(WareKind, u8); 3] = [(W::FISH, 2), (W::BREAD, 2), (W::MEAT, 2)];

/// The original rows, by building type. The donkey breeder's water and the metalworks' axe are
/// what the bytes say; both types make something else when they finish ([`crate::Game`]'s work
/// cycle breeds a donkey or picks a tool).
#[rustfmt::skip]
const TYPES: [BuildingType; BUILDING_TYPES] = [
    workplace(17, Job::WOODCUTTER,     2, 0, Small).makes(W::WOOD).cycle(14).rated(),
    workplace(20, Job::FORESTER,       2, 0, Small).cycle(7).stoppable(),
    workplace(19, Job::STONEMASON,     2, 0, Small).makes(W::STONES).cycle(14).rated(),
    workplace(18, Job::FISHER,         2, 0, Small).makes(W::FISH).cycle(17).rated(),
    workplace(22, Job::HUNTER,         2, 0, Small).makes(W::MEAT).cycle(14).rated(),
    workplace(33, Job::CARPENTER,      2, 2, Medium).makes(W::BOARDS).cycle(5).uses(&[(W::WOOD, 6)]).rated(),
    workplace(37, Job::FARMER,         3, 3, Large).makes(W::GRAIN).cycle(3).rated(),
    workplace(28, Job::PIG_BREEDER,    3, 3, Large).makes(W::HAM).cycle(5).uses(&[(W::GRAIN, 6), (W::WATER, 6)]).rated(),
    workplace(38, Job::DONKEY_BREEDER, 3, 3, Large).makes(W::WATER).cycle(10).uses(&[(W::GRAIN, 6), (W::WATER, 6)]).rated(),
    workplace(21, Job::BUTCHER,        2, 2, Medium).makes(W::MEAT).cycle(5).uses(&[(W::HAM, 6)]).rated(),
    workplace(31, Job::MILLER,         2, 2, Medium).makes(W::FLOUR).cycle(5).uses(&[(W::GRAIN, 6)]).rated(),
    workplace(32, Job::BAKER,          2, 2, Medium).makes(W::BREAD).cycle(5).uses(&[(W::FLOUR, 6), (W::WATER, 6)]).rated(),
    workplace(26, Job::IRON_FOUNDER,   2, 2, Medium).makes(W::IRON).cycle(12).uses(&[(W::IRON_ORE, 6), (W::COAL, 6)]).rated(),
    workplace(25, Job::METALWORKER,    2, 2, Medium).makes(W::AXE).cycle(14).uses(&[(W::IRON, 6), (W::BOARDS, 6)]).rated(),
    workplace(24, Job::ARMOURER,       2, 2, Medium).makes(W::SWORD).cycle(12).uses(&[(W::IRON, 6), (W::COAL, 6)]).rated(),
    workplace(34, Job::MINTER,         2, 2, Medium).makes(W::COINS).cycle(14).uses(&[(W::GOLD, 6), (W::COAL, 6)]).rated(),
    workplace(36, Job::SHIPWRIGHT,     2, 3, Medium).makes(W::BOAT).cycle(14).uses(&[(W::BOARDS, 6)]),
    workplace(23, Job::BREWER,         2, 2, Medium).makes(W::BEER).cycle(6).uses(&[(W::GRAIN, 6), (W::WATER, 6)]).rated(),
    workplace(13, Job::MINER,          4, 0, Mine).makes(W::GOLD).cycle(12).uses(&MINE_FOOD).rated(),
    workplace(12, Job::MINER,          4, 0, Mine).makes(W::IRON_ORE).cycle(12).uses(&MINE_FOOD).rated(),
    workplace(11, Job::MINER,          4, 0, Mine).makes(W::COAL).cycle(12).uses(&MINE_FOOD).rated(),
    workplace(10, Job::MINER,          4, 0, Mine).makes(W::STONES).cycle(12).uses(&MINE_FOOD).rated(),
    storehouse(29, 4, 3, Medium),
    harbour().claims(5).sees(6),
    storehouse(0, 10, 10, Large).claims(10).sees(12),
    military(1, 2, 0, Small, 1, 2).claims(9).sees(11),
    military(2, 2, 3, Small, 2, 3).claims(10).sees(12),
    military(4, 3, 5, Medium, 4, 6).claims(11).sees(13),
    military(9, 4, 7, Large, 6, 9).claims(12).sees(14),
    workplace(14, Job::SCOUT,          4, 0, Small).sees(20),
    workplace(35, Job::HELPER,         2, 0, Small).makes(W::WATER).cycle(5).rated(),
    workplace(16, Job::HELPER,         4, 2, Medium).cycle(2).uses(&[(W::STONES, 4)]),
];
