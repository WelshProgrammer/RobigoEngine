//! The data tables a game is played by, which a mod can replace.

use crate::combat::FightAnimations;
use crate::economy::EconomyTables;
use crate::military::MilitaryTables;
use crate::mission::MissionTables;
use crate::ships::CargoWeights;
use crate::world::{BuildingTypes, WaypointSet};

/// How many waypoint sets a game offers, one a nation.
pub const WAYPOINT_SETS: usize = 4;

/// Every table the sim reads besides the animation script. A game takes them when it starts;
/// neither its state nor its state hash holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimTables {
    /// The building types.
    pub building_types: BuildingTypes,
    /// The defaults behind the economy settings.
    pub economy: EconomyTables,
    /// The defaults behind the military settings.
    pub military: MilitaryTables,
    /// The builders' waypoint sets a player can be given, by nation byte.
    pub waypoints: [WaypointSet; WAYPOINT_SETS],
    /// What ships weigh wares and people by.
    pub cargo: CargoWeights,
    /// The fight and death animations.
    pub fights: FightAnimations,
    /// The rules missions play by besides their scripts.
    pub missions: MissionTables,
}

impl SimTables {
    /// The tables of `S2.EXE`.
    pub const ORIGINAL: Self = Self {
        building_types: BuildingTypes::ORIGINAL,
        economy: EconomyTables::ORIGINAL,
        military: MilitaryTables::ORIGINAL,
        waypoints: WaypointSet::ORIGINAL,
        cargo: CargoWeights::ORIGINAL,
        fights: FightAnimations::ORIGINAL,
        missions: MissionTables::ORIGINAL,
    };
}

impl Default for SimTables {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// The waypoint sets a world offers its players, by nation byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WaypointSets(pub(crate) [WaypointSet; WAYPOINT_SETS]);

impl Default for WaypointSets {
    fn default() -> Self {
        Self(WaypointSet::ORIGINAL)
    }
}

impl std::hash::Hash for WaypointSets {
    /// The sets are input data, checked by the tables hash a replay carries; the state hash
    /// leaves them out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}
