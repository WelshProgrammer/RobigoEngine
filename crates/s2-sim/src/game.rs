//! A game in progress: the whole state the simulation owns, advanced one step at a time.

use std::hash::Hash;

use s2_formats::map::Map;
use s2_formats::rtx::Script;

use crate::ageing::Ageing;
use crate::carriers::{AnimationScript, AnimationSound, Sounds};
use crate::commands::RoadDraft;
use crate::computer::{ComputerProfiles, Computers};
use crate::hash::StateHasher;
use crate::mission::Mission;
use crate::nodes::{MapError, NodeFlags, NodeMap};
use crate::objectives::{Goal, Objective};
use crate::players::{PlayerSlots, Portraits, StartOrder};
use crate::rng::GameRng;
use crate::tables::SimTables;
use crate::world::{Player, StartGoods, World};

/// Held figures are looked at again on steps that are a multiple of this.
const HOLD_PERIOD: u32 = 7;

/// The two counters `S2.EXE` advances at the start of every simulation step (`0x2716c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Clock {
    step: u32,
    cycle: u16,
}

impl Clock {
    /// The cycle counter wraps to 0 when it reaches this.
    pub const CYCLE: u16 = 11_520;

    /// The clock of a newly loaded map: no steps run, the cycle counter at 1 (`0x2509e`).
    pub const START: Self = Self { step: 0, cycle: 1 };

    /// Steps run since the game started (`0x200c9c`); savegames store it as the game time.
    #[must_use]
    pub const fn step(self) -> u32 {
        self.step
    }

    /// The cycle counter (`0x1fa4c0`) that periodic jobs are keyed on.
    #[must_use]
    pub const fn cycle(self) -> u16 {
        self.cycle
    }

    /// Whether a job keyed as `(cycle + offset) mod period == 0` runs this step.
    #[must_use]
    pub const fn every(self, period: u16, offset: u16) -> bool {
        (self.cycle as u32 + offset as u32).is_multiple_of(period as u32)
    }

    fn advance(&mut self) {
        self.step = self.step.wrapping_add(1);
        self.cycle += 1;
        if self.cycle == Self::CYCLE {
            self.cycle = 0;
        }
    }
}

/// Whether the game plays with the engine bugs copied from `S2.EXE` fixed. With them off it
/// matches S2 step for step, bugs and all, as an old save needs; with them on each bug
/// listed in `docs/design/bug-fixes.md` plays as the original meant it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum BugFixes {
    /// The bugs are fixed.
    #[default]
    On,
    /// The bugs play as in S2.
    Off,
}

impl BugFixes {
    /// Whether the bugs are fixed.
    #[must_use]
    pub const fn on(self) -> bool {
        matches!(self, Self::On)
    }
}

/// How a game is set up before its first step. A replay stores this beside the map, the seed and
/// the commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Setup {
    /// How many of the map's start positions get a headquarters, from player 0 up; `None` for
    /// the header's player count. Only read when `slots` is `None`.
    pub players: Option<usize>,
    /// Whether the map's animals are created.
    pub animals: bool,
    /// What each headquarters starts with.
    pub goods: StartGoods,
    /// Who plays each player and with which face; `None` for what the setup screen offers for
    /// the map, less the players from `players` up ([`PlayerSlots::new_game`]).
    pub slots: Option<PlayerSlots>,
    /// How the players are given the start positions.
    pub starts: StartOrder,
    /// What wins the game.
    pub objective: Objective,
    /// Whether the copied engine bugs are fixed.
    pub bug_fixes: BugFixes,
    /// The campaign mission the game plays, by number; `None` for Unlimited Play.
    pub mission: Option<u16>,
}

impl Default for Setup {
    /// Every start position of the header's player count with the slots the setup screen offers,
    /// in sequence, the map's animals, normal start goods, no objective, the bugs fixed and no
    /// mission.
    fn default() -> Self {
        Self {
            players: None,
            animals: true,
            goods: StartGoods::Normal,
            slots: None,
            starts: StartOrder::InSequence,
            objective: Objective::None,
            bug_fixes: BugFixes::On,
            mission: None,
        }
    }
}

/// Everything a running game holds: the map's nodes, what stands and walks on them, the game's
/// random number generator, the clock and the roads players are drawing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Game {
    pub(crate) nodes: NodeMap,
    pub(crate) world: World,
    pub(crate) rng: GameRng,
    pub(crate) clock: Clock,
    pub(crate) walk_cursor: u8,
    pub(crate) spawn_cursor: (u16, u16),
    pub(crate) ageing: Ageing,
    pub(crate) drafts: [Option<RoadDraft>; Player::COUNT],
    pub(crate) goal: Goal,
    pub(crate) computers: Computers,
    pub(crate) mission: Option<Mission>,
    #[serde(skip)]
    pub(crate) script: AnimationScript,
    #[serde(skip)]
    pub(crate) sounds: Sounds,
}

impl Game {
    /// Starts a game on `map` as `S2.EXE` starts the first game after launch. The generator is in
    /// its start-up state, the map's objects and animals are placed, every player `setup` puts in
    /// the game gets an AI, people's included, then a headquarters. Without animals the generator still draws for the map's
    /// rabbits, as the map loader always creates them. The walking-speed cursor starts at 0, as
    /// every map load sets it. The spawner's cursor starts at 0, which matches only the first
    /// game of a session: `S2.EXE` never resets it.
    ///
    /// # Errors
    ///
    /// [`MapError`] when the map lacks a layer the nodes are read from.
    pub fn start(map: &Map, setup: &Setup) -> Result<Self, MapError> {
        Self::start_with(map, setup, &SimTables::ORIGINAL)
    }

    /// [`Game::start`], played by `tables`.
    ///
    /// # Errors
    ///
    /// As [`Game::start`].
    pub fn start_with(map: &Map, setup: &Setup, tables: &SimTables) -> Result<Self, MapError> {
        Self::start_mission(map, setup, &Script::default(), tables)
    }

    /// [`Game::start_with`] for a campaign chapter when `setup` names a mission, which plays
    /// `script`. Unless `setup` gives the slots, they are seated as the
    /// campaign start seats them ([`PlayerSlots::campaign`]). The script's alliances, computer
    /// barriers and text table are read before the world is built, every player with a start
    /// position in the header gets an empty headquarters where one fits, and then the script's
    /// `[MAPCOMMANDS]` run. Without a mission `script` is not read.
    ///
    /// # Errors
    ///
    /// As [`Game::start`].
    pub fn start_mission(
        map: &Map,
        setup: &Setup,
        script: &Script,
        tables: &SimTables,
    ) -> Result<Self, MapError> {
        let mut nodes = NodeMap::from_map(map)?;
        let mut rng = GameRng::at_start();
        let mut world = World::from_map(map, &mut rng);
        world.set_tables(tables);
        world.bug_fixes = setup.bug_fixes;
        if !setup.animals {
            world.clear_figures();
        }
        let slots = setup.slots.unwrap_or_else(|| match setup.mission {
            Some(_) => PlayerSlots::campaign(map, script),
            None => PlayerSlots::new_game(map, setup.players, &Portraits::ORIGINAL),
        });
        let computers = Computers::build(
            (&nodes, &world),
            &slots,
            (&Portraits::ORIGINAL, &ComputerProfiles::ORIGINAL),
            &mut rng,
        );
        let mission = setup.mission.map(|number| Mission::new(number, &tables.missions));
        if mission.is_some() {
            nodes.keep_contacts();
            world.start_campaign(map, &mut nodes, &slots);
        } else {
            world.start_game(map, &mut nodes, (&slots, setup.starts), setup.goods, &mut rng);
        }
        let mut game = Self {
            nodes,
            world,
            rng,
            clock: Clock::START,
            walk_cursor: 0,
            spawn_cursor: (0, 0),
            ageing: Ageing::default(),
            drafts: Default::default(),
            goal: Goal {
                objective: setup.objective,
                winner: None,
            },
            computers,
            mission,
            script: AnimationScript::default(),
            sounds: Sounds::default(),
        };
        if game.mission.is_some() {
            if let Some(settings) = script.global_settings() {
                game.read_settings(settings);
            }
            if let Some(commands) = script.map_commands() {
                game.run_section(commands);
            }
        }
        Ok(game)
    }

    /// A game at the start clock on `nodes` and `world`, for tests that build their own map.
    #[cfg(test)]
    pub(crate) fn from_parts(nodes: NodeMap, world: World) -> Self {
        Self {
            nodes,
            world,
            rng: GameRng::at_start(),
            clock: Clock::START,
            walk_cursor: 0,
            spawn_cursor: (0, 0),
            ageing: Ageing::default(),
            drafts: Default::default(),
            goal: Goal::default(),
            computers: Computers::default(),
            mission: None,
            script: AnimationScript::default(),
            sounds: Sounds::default(),
        }
    }

    /// The map's nodes.
    #[must_use]
    pub fn nodes(&self) -> &NodeMap {
        &self.nodes
    }

    /// Objects, buildings, flags and figures.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Whether the copied engine bugs are fixed.
    #[must_use]
    pub fn bug_fixes(&self) -> BugFixes {
        self.world.bug_fixes
    }

    /// The clock.
    #[must_use]
    pub fn clock(&self) -> Clock {
        self.clock
    }

    /// The game's generator as it stands.
    #[must_use]
    pub fn generator(&self) -> GameRng {
        self.rng
    }

    /// Runs one simulation step: the counters, then every figure in record order, then the jobs
    /// keyed on the cycle counter: building animations every 2 steps, workplace input orders
    /// every 240, transport orders every 16, the harbour pass every 100, the building tick every
    /// 48, a fresh chance for a lost settler to look for home every 24, the objective pass every
    /// 160 or in a campaign chapter the mission's event check every 20, productivity averages
    /// every 400, the military pass every 32, stranded wares every 80,
    /// the donkey counters' decay every 500, the statistics samples at each series' own period
    /// and new helpers and recruits every 240.
    /// Then the ageing pass, the catapult stones, the computer players' think and every 8 steps
    /// the animal spawner.
    pub fn step(&mut self) {
        self.clock.advance();
        self.step_figures();
        if self.clock.every(2, 1) {
            self.animate_buildings();
        }
        if self.clock.every(240, 0) {
            self.order_inputs();
        }
        if self.clock.every(16, 0) {
            self.serve_orders();
        }
        if self.clock.every(100, 1) {
            self.harbour_pass();
        }
        if self.clock.every(48, 2) {
            self.building_tick();
        }
        if self.clock.every(24, 0) {
            self.world.rescue_tried = false;
        }
        if self.mission.is_some() {
            if self.clock.every(20, 6) {
                self.check_events();
            }
        } else if self.clock.every(160, 1) {
            self.decide();
        }
        if self.clock.every(400, 1) {
            self.average_productivity();
        }
        if self.clock.every(32, 4) {
            self.military_pass();
        }
        if self.clock.every(80, 3) {
            self.world.route_stranded();
        }
        if self.clock.every(500, 1) {
            self.world.decay_traffic();
        }
        for (series, &period) in crate::statistics::PERIODS.iter().enumerate() {
            if self.clock.every(period, 0) {
                self.world.take_samples(series, &self.nodes);
            }
        }
        if self.clock.every(240, 1) {
            self.make_helpers();
        }
        self.age();
        self.step_stones();
        self.computers_step();
        if self.clock.every(8, 7) {
            self.spawn_animals();
        }
    }

    /// Every figure record in use, in record order (`0x32b34`). Records added during the loop
    /// past the range it started with wait for the next step. With the bug fixes off, once a
    /// figure's turn grows the figure pool the loop ends for the step, as `S2.EXE`'s loop goes
    /// on through the freed block where every record reads as free.
    fn step_figures(&mut self) {
        self.world.figures_moved = false;
        for record in 0..self.world.figure_records() {
            if self.world.figures_moved && !self.world.bug_fixes.on() {
                break;
            }
            let Some(figure) = self.world.figure(record) else {
                continue;
            };
            if figure.held && !self.release(record) {
                continue;
            }
            let Some(figure) = self.world.figure(record) else {
                continue;
            };
            match figure.kind {
                crate::figures::FigureKind::Animal { .. } => self.step_animal(record),
                crate::figures::FigureKind::Ship { .. } => self.step_ship(record),
                _ => self.step_settler(record),
            }
        }
    }

    /// Releases held figure `record` on a step that is a multiple of 7 whose own node is not
    /// held (`0x32cf8`, `0x323d0`): a figure that took the held bit on the node behind it gives
    /// it back, and it runs this step. Returns whether it was released, so a figure stopped
    /// part way by a node a fight holds waits until that node clears.
    fn release(&mut self, record: u16) -> bool {
        if !self.clock.step().is_multiple_of(HOLD_PERIOD) {
            return false;
        }
        let Some(figure) = self.world.figure(record).copied() else {
            return false;
        };
        if self.nodes.get(figure.node()).flags.contains(NodeFlags::HELD) {
            return false;
        }
        self.clear_queue_marks(record);
        true
    }

    /// Figure `record` stops queueing (`0x323d0`): it gives back the held bit it took on the
    /// node behind it, if it took one, and is no longer held.
    pub(crate) fn clear_queue_marks(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        if figure.holds_behind {
            let behind = figure.node().neighbour(
                figure.direction.opposite(),
                self.nodes.width(),
                self.nodes.height(),
            );
            let mut data = self.nodes.get(behind);
            data.flags -= NodeFlags::HELD;
            self.nodes.set(behind, data);
        }
        if let Some(figure) = self.world.figure_mut(record) {
            figure.held = false;
            figure.holds_behind = false;
        }
    }

    /// Takes `other`'s configuration tables: its animation script, building types and
    /// waypoints, which a saved state leaves out.
    pub(crate) fn adopt_tables(&mut self, other: &Self) {
        self.script.clone_from(&other.script);
        self.world.adopt_tables(&other.world);
    }

    /// A hash of the whole state, equal on every machine for equal states.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        StateHasher::of(self)
    }

    /// The effects animations asked for since the last call, each with the node it plays on.
    /// Taking them changes nothing the simulation reads.
    pub fn take_sounds(&mut self) -> Vec<AnimationSound> {
        std::mem::take(&mut self.sounds.0)
    }
}

#[cfg(test)]
mod tests;
