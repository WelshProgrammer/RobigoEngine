//! The deterministic, headless game world. The same map, settings, seed and commands give the
//! same state on every machine.
//!
//! A [`Game`] holds the map's nodes ([`NodeMap`]), the objects, buildings, flags, roads and
//! animals on them ([`World`]), territory and the game's random number generator ([`GameRng`]),
//! built the way `S2.EXE` builds them. It changes only by [`Game::step`] and by player
//! [`Command`]s, which so far place and remove flags and draw and remove roads. The renderer draws
//! from these types and never changes them.
//!
//! The crate takes no input but its arguments and keeps no clock. Its `clippy.toml` rejects
//! floats, hash-ordered collections and wall-clock time.

#![deny(clippy::float_arithmetic, clippy::disallowed_types)]

use s2_systems::{grid, job, ware};

mod ageing;
mod animals;
mod builder;
mod carriers;
mod catapult;
mod combat;
mod commands;
pub mod computer;
mod construction;
mod demolish;
mod economy;
mod explore;
mod figures;
mod game;
mod gather;
mod ground;
mod harbours;
mod hash;
mod helpers;
mod homeward;
mod hunt;
mod lost;
mod marks;
mod messages;
mod military;
mod mines;
mod mission;
mod nodes;
mod objectives;
mod packed;
mod people;
mod planer;
mod players;
mod pool;
mod repair;
pub mod replay;
mod rng;
pub mod save;
mod send_out;
mod settlers;
mod ships;
mod shipyard;
mod sight;
mod sites;
mod statistics;
mod supply;
mod tables;
mod territory;
mod transport;
mod wares;
mod work;
mod workplace;
mod world;

pub use carriers::{Animation, AnimationScript, AnimationSound};
pub use catapult::{Flight, Stone};
pub use combat::{
    Alliances, FightAnimations, FightAnimationsError, FightStep, FighterAnimations, Role, SoldierRecord,
};
pub use commands::{
    Command, MAX_SEGMENTS, MAX_WATER_SEGMENTS, Refusal, RoadDraft, Stamped, StepPreview, StockItem,
};
pub use construction::{BUILDER_MODE, PLANER_MODE, fits};
pub use economy::{
    Consumer, DistributionGroup, EconomyTables, EconomyTablesError, MAX_CONSUMERS, MAX_WEIGHT,
};
pub use figures::{
    AnimalState, Builder, Carrier, CarrierState, Delivery, Entry, Explorer, Exploring, Figure, FigureKind,
    Gatherer, OnRoad, Planer, PlanerWork, Porter, Soldier, SoldierState, Species, SpeciesTraits, Transport,
    Trip, Walker,
};
pub use game::{BugFixes, Clock, Game, Setup};
pub use grid::{Direction, Node, NodeRect};
pub use ground::{Property, TerrainClass};
pub use harbours::{Harbour, MAX_HARBOURS};
pub use hash::StateHasher;
pub use job::Job;
pub use marks::{Edges, RoadKind, Roads};
pub use messages::{MESSAGE_IDS, MOST_MESSAGES, Message, MessageId, MessageType, MessageTypes, Messages};
pub use military::{
    ATTACK, DEFENCE, MILITARY_SETTINGS, MOST_MILITARY, Military, MilitaryTables, MilitaryTablesError,
    OCCUPATION, RANK, RECRUITING,
};
pub use mission::{
    ACTIVE_SLOTS, BARRIERS, Barrier, CHAPTER_FLAGS, ComputerBan, EventRecord, Letter, Mission, MissionTables,
    map_path, script_path,
};
pub use nodes::{MapError, Mineral, NodeData, NodeFlags, NodeMap, Resources, SiteFlags, SiteSize, Sites};
pub use objectives::{Goal, Objective};
pub use players::{
    Controller, NATIONS, PlayerSlot, PlayerSlots, Portrait, PortraitRow, Portraits, PortraitsError, Roster,
    RosterEntry, StartOrder,
};
pub use pool::Pool;
pub use repair::{BeyondRepair, Repair, Repaired};
pub use rng::{GameRng, START_SEED};
pub use ships::{CargoWeights, MAX_SHIPS, SECTORS, Ship, ShipMode};
pub use sight::{Seen, reveal_walk};
pub use sites::{refresh_sites, site};
pub use statistics::{GRAPH_SLOTS, PERIODS, SAMPLES, SERIES, Sample, Series, Statistic, Statistics};
pub use tables::{SimTables, WAYPOINT_SETS};
pub use territory::{HQ_RADIUS, border_pass, claim, mark_borders};
pub use transport::{
    CHOSEN, CONSTRUCTION_WEIGHT, MAX_ORDERS, MAX_WARES, MOST_URGENT, NO_BUILDING_URGENCY, Order, OrderKind,
    ROAD_MODE, Reached, SearchOptions, TRANSPORT_GROUP_NAMES, TRANSPORT_GROUPS, WARE_MODE, Wanted, Ware,
    WareState, next_flag, search,
};
pub use ware::WareKind;
pub use world::{
    Ageing, BUILDING_TYPES, Building, BuildingKind, BuildingState, BuildingType, BuildingTypes,
    BuildingTypesError, Burning, Category, Decoration, FLAG_CAPACITY, Field, FieldStage, Flag, FlagKind,
    FlagLink, Granite, MAX_BUILDING_TYPES, MAX_BUILDINGS, MAX_FLAGS, MAX_ROADS, MapObject, ObjectWord,
    Player, Remnant, RemnantLook, Reserve, Road, RoadError, ShipSite, Sign, Size, StartGoods, StockSetting,
    Storehouse, Tree, WAITING, WARE_TYPES, WAYPOINTS, WaypointSet, WaypointSetError, Waypoints, World,
    connect_to_flag,
};
