//! A computer player's 22 tasks. Each has its type, the job kinds it handles and whether the
//! periodic pass calls it. Their work comes in task by task; until then the periodic think and the
//! job handlers do nothing.

use crate::game::Game;
use crate::grid::Node;
use crate::military::MILITARY_SETTINGS;
use crate::rng::GameRng;
use crate::world::{BuildingKind, MapObject, Player};

use super::attacks::Attacks;
use super::board::RequestBoard;
use super::economy::EconomyPlanner;
use super::expansion::AreaScans;
use super::grid::Grid;
use super::keeper::FlagKeeper;
use super::land_use::LandUse;
use super::storehouses::StorehouseSearch;
use super::towers::Guards;
use super::{ComputerProfile, JobKind};

/// The 22 task types, by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum TaskKind {
    /// 0: reserves a site's node.
    Reserve,
    /// 1: finds a site for a building and places it.
    Site,
    /// 2: joins a flag to the roads.
    Connect,
    /// 3: lays a road between two flags.
    Shortcut,
    /// 4: places a building with its road.
    Place,
    /// 5: would count reserved sites; never reached.
    Ledger,
    /// 6: pushes the border towards a node.
    Expand,
    /// 7: looks for somewhere to expand.
    Scout,
    /// 8: turns build orders into placements.
    Orders,
    /// 9: scans round the player's buildings.
    Scans,
    /// 10: tears down what is spent and tidies the roads.
    FlagKeeper,
    /// 11: plans storehouses.
    Storehouses,
    /// 12: plans what the land is used for.
    LandUse,
    /// 13: attacks.
    Attacks,
    /// 14: plans the economy and keeps the request board.
    Economy,
    /// 15: places towers and catapults.
    Towers,
    /// 16: builds economy chains.
    Projects,
    /// 17: switches the military settings.
    MilitarySets,
    /// 18: does nothing.
    Unused18,
    /// 19: does nothing.
    Unused19,
    /// 20: does nothing.
    Unused20,
    /// 21: does nothing.
    Unused21,
}

impl TaskKind {
    /// Every task, by number: the order the periodic pass runs them in.
    pub const ALL: [Self; 22] = [
        Self::Reserve,
        Self::Site,
        Self::Connect,
        Self::Shortcut,
        Self::Place,
        Self::Ledger,
        Self::Expand,
        Self::Scout,
        Self::Orders,
        Self::Scans,
        Self::FlagKeeper,
        Self::Storehouses,
        Self::LandUse,
        Self::Attacks,
        Self::Economy,
        Self::Towers,
        Self::Projects,
        Self::MilitarySets,
        Self::Unused18,
        Self::Unused19,
        Self::Unused20,
        Self::Unused21,
    ];

    /// The order a player's tasks are made in (the list at `0x1fdcc0`), 14 before 13. The first
    /// made of the tasks that claim a job kind handles it.
    pub const MADE: [Self; 22] = [
        Self::Reserve,
        Self::Site,
        Self::Connect,
        Self::Shortcut,
        Self::Place,
        Self::Ledger,
        Self::Expand,
        Self::Scout,
        Self::Orders,
        Self::Scans,
        Self::FlagKeeper,
        Self::Storehouses,
        Self::LandUse,
        Self::Economy,
        Self::Attacks,
        Self::Towers,
        Self::Projects,
        Self::MilitarySets,
        Self::Unused18,
        Self::Unused19,
        Self::Unused20,
        Self::Unused21,
    ];

    /// Its number.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The job kinds it claims (its vtable's `+0x08`).
    #[must_use]
    pub const fn handles(self) -> &'static [JobKind] {
        match self {
            Self::Reserve | Self::Ledger => &[JobKind::Reserve],
            Self::Site => &[JobKind::Site],
            Self::Connect => &[JobKind::Connect],
            Self::Shortcut => &[JobKind::Shortcut],
            Self::Place => &[JobKind::Place],
            Self::Expand => &[JobKind::Expand],
            Self::Scout => &[JobKind::Scout],
            Self::Orders => &[JobKind::BuildOrder],
            Self::Projects => &[JobKind::Project],
            _ => &[],
        }
    }

    /// Whether the periodic pass calls it (its vtable's `+0x1c`).
    #[must_use]
    pub const fn thinks(self) -> bool {
        matches!(
            self,
            Self::Scans
                | Self::FlagKeeper
                | Self::Storehouses
                | Self::LandUse
                | Self::Attacks
                | Self::Economy
                | Self::Towers
                | Self::MilitarySets
        )
    }
}

/// The share a draw of 0 to 100 is counted out of.
const PERCENT: u32 = 100;

/// Task 17 picks a new military set every this many calls (`0x72fb4`).
const SET_EVERY: u32 = 120;

/// How many calls task 17 holds the special set for (`0x73053`).
const SPECIAL_FOR: u32 = 30;

/// What a task reads and changes as it thinks.
pub(crate) struct Think<'a> {
    /// The thinking player.
    pub player: Player,
    /// The step it thinks on, which jobs it makes keep (`0x200c9c`).
    pub step: u32,
    /// The profile the player plays by.
    pub profile: ComputerProfile,
    /// The game, its computer players lent out while one of them thinks.
    pub game: &'a mut Game,
    /// The grid.
    pub grid: &'a Grid,
}

impl Think<'_> {
    /// The player's seven military settings (`+0x26f6` of its record).
    fn military(&mut self) -> &mut [u8; MILITARY_SETTINGS] {
        &mut self.game.world.military_settings[self.player.index()]
    }

    /// The distance `|dx| + |dy|`, each the shorter way round the map (`0x22720`).
    pub fn distance(&self, a: Node, b: Node) -> u32 {
        let (width, height) = (
            u32::from(self.game.nodes.width()),
            u32::from(self.game.nodes.height()),
        );
        let dx = u32::from(a.x.abs_diff(b.x));
        let dy = u32::from(a.y.abs_diff(b.y));
        dx.min(width - dx) + dy.min(height - dy)
    }

    /// How many buildings or sites of `kind`, of any kind for `None`, stand on rings 1 to `reach`
    /// round `node` on nodes `player` owns (`0x6857e`).
    pub fn count_near(&self, player: Player, kind: Option<BuildingKind>, node: Node, reach: u16) -> u16 {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let counted = (1..=reach)
            .flat_map(|ring| node.ring(ring, width, height))
            .filter(|&at| self.game.nodes.get(at).owner == Some(player))
            .filter(|&at| {
                let MapObject::Building(record) = self.game.world.object(at) else {
                    return false;
                };
                self.game
                    .world
                    .building(record)
                    .is_some_and(|building| kind.is_none_or(|kind| building.kind == kind))
            })
            .count();
        u16::try_from(counted).unwrap_or(u16::MAX)
    }

    /// Whether a building or site of `kind` stands on rings 1 to `reach` round `node` on a node
    /// `player` owns (`0x684bf`).
    pub fn any_near(&self, player: Player, kind: BuildingKind, node: Node, reach: u16) -> bool {
        self.count_near(player, Some(kind), node, reach) > 0
    }

    /// How many buildings and sites of `kind` the thinking player has (`0x29b84`).
    pub fn built(&self, kind: BuildingKind) -> u16 {
        self.game.world.finished_buildings(self.player, kind)
            + self.game.world.building_sites(self.player, kind)
    }
}

/// Task 17's fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MilitarySets {
    /// Whether the special set holds (`+0x0a`).
    pub special: bool,
    /// Calls left until the special set starts or, while it holds, ends (`+0x0e`).
    pub wait: u32,
    /// The low four bits of the last draw that picked a set (`+0x12`).
    pub pick: u8,
    /// The set in force outside the special one, 0 to 2 (`+0x16`).
    pub set: u8,
    /// Calls since the last pick (`+0x1a`).
    pub calls: u32,
}

impl MilitarySets {
    /// A new countdown to the special set: the profile's least wait plus a draw of 0 to 100
    /// hundredths of the span, truncated (`0x7305f`).
    fn rewind(&mut self, profile: &ComputerProfile, rng: &mut GameRng) {
        let (least, most) = profile.special_wait;
        let share = u32::from(rng.up_to(100));
        self.wait = least.wrapping_add(share.wrapping_mul(most.wrapping_sub(least)) / PERCENT);
    }

    /// One call (`0x72f91`). Outside the special set, every 120th call picks a set from a raw
    /// draw's low four bits and puts it in force. Then the countdown runs down: outside the
    /// special set it ends by putting the special set in force for 30 calls, and inside it by
    /// putting the picked set back and drawing a new countdown.
    fn think(&mut self, think: &mut Think<'_>) {
        if !self.special {
            self.calls += 1;
            if self.calls >= SET_EVERY {
                self.calls = 0;
                self.pick = u8::try_from(think.game.rng.step() & 15).unwrap_or_default();
                self.set = think.profile.set_picks[usize::from(self.pick)];
                *think.military() = self.in_force(&think.profile);
            }
        }
        self.wait = self.wait.wrapping_sub(1);
        if self.wait != 0 {
            return;
        }
        if self.special {
            *think.military() = self.in_force(&think.profile);
            self.special = false;
            self.rewind(&think.profile, &mut think.game.rng);
        } else {
            *think.military() = think.profile.special_set;
            self.special = true;
            self.wait = SPECIAL_FOR;
        }
    }

    /// The settings of the picked set (`0x730a9`).
    fn in_force(self, profile: &ComputerProfile) -> [u8; MILITARY_SETTINGS] {
        profile
            .military_sets
            .get(usize::from(self.set))
            .copied()
            .unwrap_or(profile.military_sets[0])
    }
}

/// What the tasks keep between calls.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Tasks {
    /// Task 12.
    pub(super) land_use: LandUse,
    /// Task 14's request board.
    pub(super) board: RequestBoard,
    /// Task 9.
    pub(super) scans: AreaScans,
    /// Task 10.
    pub(super) flag_keeper: FlagKeeper,
    /// Task 11.
    pub(super) storehouses: StorehouseSearch,
    /// Task 13.
    pub(super) attacks: Attacks,
    /// Task 15.
    pub(super) guards: Guards,
    /// Task 14's own fields.
    pub(super) economy: EconomyPlanner,
    /// Task 17.
    military_sets: MilitarySets,
}

impl Tasks {
    /// Task 17's fields.
    #[must_use]
    pub fn military_sets(&self) -> MilitarySets {
        self.military_sets
    }

    /// Task 12's fields.
    #[must_use]
    pub fn land_use(&self) -> LandUse {
        self.land_use
    }

    /// Task 14's own fields.
    #[must_use]
    pub fn economy(&self) -> EconomyPlanner {
        self.economy
    }

    /// Task 14's request board.
    #[must_use]
    pub fn board(&self) -> &RequestBoard {
        &self.board
    }

    /// Task 17's countdown to its next special military set, or to its end.
    #[must_use]
    pub fn special_wait(&self) -> u32 {
        self.military_sets.wait
    }

    /// Task 9's fields.
    #[must_use]
    pub fn scans(&self) -> AreaScans {
        self.scans
    }

    /// Task 10's fields.
    #[must_use]
    pub fn flag_keeper(&self) -> FlagKeeper {
        self.flag_keeper
    }

    /// Task 11's fields.
    #[must_use]
    pub fn storehouses(&self) -> StorehouseSearch {
        self.storehouses
    }

    /// Task 13's fields.
    #[must_use]
    pub fn attacks(&self) -> Attacks {
        self.attacks
    }

    /// Task 15's fields.
    #[must_use]
    pub fn guards(&self) -> Guards {
        self.guards
    }

    /// Runs `kind`'s constructor for `player`. Only task 17's draws, for its first countdown.
    pub(super) fn make(
        &mut self,
        kind: TaskKind,
        player: Player,
        profile: &ComputerProfile,
        rng: &mut GameRng,
    ) {
        match kind {
            TaskKind::Scans => self.scans = AreaScans::new(player),
            TaskKind::MilitarySets => self.military_sets.rewind(profile, rng),
            _ => {}
        }
    }

    /// `kind`'s periodic think.
    pub(super) fn think(&mut self, kind: TaskKind, think: &mut Think<'_>) {
        if kind == TaskKind::MilitarySets {
            self.military_sets.think(think);
        }
    }
}

#[cfg(test)]
mod tests;
