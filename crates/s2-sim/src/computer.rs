//! The Original: `S2.EXE`'s computer players, rebuilt to give the same game (`docs/design/ai.md`).
//!
//! Every player in the game gets a [`ComputerPlayer`] when the game is built, people's too, and
//! only the ones the computer plays think. A computer player is 22 tasks and a pool of jobs the
//! tasks hand each other. This is the frame they run in: the tasks do nothing yet.

use crate::game::Game;
use crate::nodes::NodeMap;
use crate::players::{Controller, PlayerSlots, Portraits};
use crate::rng::GameRng;
use crate::world::{Player, World};

mod attacks;
mod board;
mod chain;
mod economy;
mod expansion;
mod grid;
mod jobs;
mod keeper;
mod land_use;
mod profiles;
mod projects;
mod storehouses;
mod tasks;
mod tools;
mod towers;

pub use attacks::Attacks;
pub use board::{Request, RequestBoard};
pub use economy::EconomyPlanner;
pub use expansion::AreaScans;
pub use grid::{CELL, Cell, CellFlags, Grid};
pub use jobs::{Expansion, Job, JobFlags, JobKind, Payload, ScoutMode};
pub use keeper::FlagKeeper;
pub use land_use::LandUse;
pub use profiles::{
    Attacking, ComputerProfile, ComputerProfiles, FlagKeeping, RoadRating, Scans, TargetValues, Towers,
};
pub use storehouses::StorehouseSearch;
pub use tasks::{MilitarySets, TaskKind, Tasks};
pub use towers::Guards;

use tasks::Think;

/// How many jobs a computer player holds at once (`+0x27a`).
pub const JOBS: usize = 300;

/// A player thinks on steps whose number modulo this is its own (`0x270f8`).
const THINK_EVERY: u32 = 10;

/// The tasks' periodic pass runs on thinks whose step number, masked with this, is the player's
/// own (`0x69d8e`): every 40 steps with [`THINK_EVERY`].
const PERIODIC_MASK: u32 = 7;

/// A list of job indices with room for `N`, as the player AI keeps them: an add goes on the end
/// and is dropped when the list is full, and a remove closes the gap (`0x69ebc`, `0x69ee2`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Capped<const N: usize>(Vec<u16>);

impl<const N: usize> Capped<N> {
    /// Adds `job` on the end, unless the list is full. Returns whether it went on.
    pub fn add(&mut self, job: u16) -> bool {
        if self.0.len() >= N {
            return false;
        }
        self.0.push(job);
        true
    }

    /// Takes `job` off the list, closing the gap; nothing when it is not there.
    pub fn remove(&mut self, job: u16) {
        if let Some(place) = self.0.iter().position(|&held| held == job) {
            self.0.remove(place);
        }
    }

    /// The job at `place`, `None` past the end.
    #[must_use]
    pub fn get(&self, place: usize) -> Option<u16> {
        self.0.get(place).copied()
    }

    /// The jobs, in list order.
    #[must_use]
    pub fn as_slice(&self) -> &[u16] {
        &self.0
    }
}

/// One player's AI (the 1,926-byte object `0x69492` builds).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ComputerPlayer {
    player: Player,
    profile: u8,
    roots: Capped<100>,
    fresh: Capped<50>,
    running: Capped<100>,
    cursor: u16,
    doomed: Capped<50>,
    handlers: [Option<TaskKind>; JobKind::COUNT],
    jobs: Vec<Option<Job>>,
    tasks: Tasks,
}

impl ComputerPlayer {
    /// The AI for `player`, playing by profile `profile` of `profiles`, with its 22 tasks made in
    /// `S2.EXE`'s order. Task 17's constructor draws once from `rng` (`0x7307f`).
    #[must_use]
    pub fn new(player: Player, profile: u8, profiles: &ComputerProfiles, rng: &mut GameRng) -> Self {
        let mut made = Self {
            player,
            profile,
            roots: Capped::default(),
            fresh: Capped::default(),
            running: Capped::default(),
            cursor: 0,
            doomed: Capped::default(),
            handlers: [None; JobKind::COUNT],
            jobs: vec![None; JOBS],
            tasks: Tasks::default(),
        };
        let profile = profiles.get(profile);
        for kind in TaskKind::MADE {
            made.tasks.make(kind, player, &profile, rng);
            for &job in kind.handles() {
                made.handlers[job.index()].get_or_insert(kind);
            }
        }
        made
    }

    /// The player it plays.
    #[must_use]
    pub fn player(&self) -> Player {
        self.player
    }

    /// The profile it plays by, an index into the profiles.
    #[must_use]
    pub fn profile(&self) -> u8 {
        self.profile
    }

    /// Its tasks.
    #[must_use]
    pub fn tasks(&self) -> &Tasks {
        &self.tasks
    }

    /// The task that handles jobs of `kind`: the first made of those that claim it (`0x6997c`).
    #[must_use]
    pub fn handler(&self, kind: JobKind) -> Option<TaskKind> {
        self.handlers[kind.index()]
    }

    /// Job `index`, when it is in use.
    #[must_use]
    pub fn job(&self, index: u16) -> Option<&Job> {
        self.jobs.get(usize::from(index))?.as_ref()
    }

    /// The jobs in use, by index.
    pub fn jobs(&self) -> impl Iterator<Item = (u16, &Job)> {
        (0u16..)
            .zip(&self.jobs)
            .filter_map(|(index, job)| Some((index, job.as_ref()?)))
    }

    /// The root jobs, in list order.
    #[must_use]
    pub fn roots(&self) -> &[u16] {
        self.roots.as_slice()
    }

    /// The jobs ended and waiting to be deleted, in order.
    #[must_use]
    pub fn doomed(&self) -> &[u16] {
        self.doomed.as_slice()
    }

    /// Where the round robin over the running jobs stands.
    #[must_use]
    pub fn cursor(&self) -> u16 {
        self.cursor
    }

    /// The jobs waiting to start, in order.
    #[must_use]
    pub fn fresh(&self) -> &[u16] {
        self.fresh.as_slice()
    }

    /// The jobs being worked, in the order the round robin takes them.
    #[must_use]
    pub fn running(&self) -> &[u16] {
        self.running.as_slice()
    }

    /// Adds `job` under `parent`, or as a root job, owned by `owner` (`0x697c8`). It takes the
    /// lowest free index, goes to the head of its parent's children and waits to start. With all
    /// 300 in use it is lost, as in `S2.EXE`, and `None` comes back.
    pub fn add(&mut self, mut job: Job, parent: Option<u16>, owner: Option<TaskKind>) -> Option<u16> {
        let index = self.jobs.iter().position(Option::is_none)?;
        let index = u16::try_from(index).ok()?;
        job.flags = JobFlags::empty();
        job.parent = parent;
        job.owner = owner;
        job.child = None;
        job.next = None;
        if let Some(above) = parent.and_then(|parent| self.jobs.get_mut(usize::from(parent))?.as_mut()) {
            job.next = above.child.replace(index);
        } else {
            job.parent = None;
            self.roots.add(index);
        }
        self.jobs[usize::from(index)] = Some(job);
        self.fresh.add(index);
        Some(index)
    }

    /// Deletes job `index` and everything under it, children first (`0x6989b`), off every list.
    pub fn delete(&mut self, index: u16) {
        let Some(job) = self.jobs.get(usize::from(index)).copied().flatten() else {
            return;
        };
        match job.parent {
            Some(parent) => self.unlink(parent, index),
            None => self.roots.remove(index),
        }
        let mut child = job.child;
        while let Some(below) = child {
            child = self.job(below).and_then(|below| below.next);
            self.delete(below);
        }
        self.fresh.remove(index);
        self.running.remove(index);
        self.jobs[usize::from(index)] = None;
    }

    /// Puts job `index` on the list of jobs to delete once the running job's turn ends
    /// (`0x69d1f`).
    pub fn end(&mut self, index: u16) {
        self.doomed.add(index);
    }

    /// Takes `index` out of `parent`'s children.
    fn unlink(&mut self, parent: u16, index: u16) {
        let next = self.job(index).and_then(|job| job.next);
        let Some(first) = self.job(parent).and_then(|job| job.child) else {
            return;
        };
        if first == index {
            if let Some(Some(above)) = self.jobs.get_mut(usize::from(parent)) {
                above.child = next;
            }
            return;
        }
        let mut at = first;
        while let Some(sibling) = self.job(at).and_then(|job| job.next) {
            if sibling == index {
                if let Some(Some(before)) = self.jobs.get_mut(usize::from(at)) {
                    before.next = next;
                }
                return;
            }
            at = sibling;
        }
    }

    /// Starts every job waiting, those added meanwhile included (`0x69a21`). A job whose kind has
    /// no handler is deleted; the rest are marked started and offered to their handler.
    fn start_jobs(&mut self, think: &mut Think<'_>) {
        while let Some(index) = self.fresh.get(0) {
            match self.job(index).and_then(|job| self.handler(job.kind)) {
                None => self.delete(index),
                Some(handler) => {
                    if let Some(Some(job)) = self.jobs.get_mut(usize::from(index)) {
                        job.flags |= JobFlags::STARTED;
                    }
                    let _: u16 = self.accept(handler, index, think);
                }
            }
            self.fresh.remove(index);
        }
    }

    /// Gives the next running job its turn and then deletes the jobs ended meanwhile
    /// (`0x69c27`). On an empty entry the round robin goes back to the first job and serves it.
    /// A job whose kind has no handler is passed over and the deletions wait.
    fn advance_one(&mut self, think: &mut Think<'_>) {
        if self.running.get(usize::from(self.cursor)).is_none() {
            self.cursor = 0;
        }
        let Some(index) = self.running.get(usize::from(self.cursor)) else {
            return;
        };
        self.cursor += 1;
        let Some(handler) = self.job(index).and_then(|job| self.handler(job.kind)) else {
            return;
        };
        let _: u16 = self.advance(handler, index, think);
        while let Some(doomed) = self.doomed.get(0) {
            self.doomed.remove(doomed);
            self.delete(doomed);
        }
    }

    /// One think on step `step` (`0x69d7d`): the tasks' periodic pass when the step's low three
    /// bits are the player's, then the new jobs, then one running job.
    fn think(&mut self, step: u32, think: &mut Think<'_>) {
        if usize::try_from(step & PERIODIC_MASK).ok() == Some(self.player.index()) {
            for kind in TaskKind::ALL {
                if !kind.thinks() {
                    continue;
                }
                if kind == TaskKind::LandUse {
                    self.land_use(think);
                } else if kind == TaskKind::Economy {
                    self.economy(think);
                } else if kind == TaskKind::Scans {
                    self.scans(think);
                } else if kind == TaskKind::FlagKeeper {
                    self.keep_flags(think);
                } else if kind == TaskKind::Storehouses {
                    self.storehouses(think);
                } else if kind == TaskKind::Attacks {
                    self.attack(think);
                } else if kind == TaskKind::Towers {
                    self.guard(think);
                } else {
                    self.tasks.think(kind, think);
                }
            }
        }
        self.start_jobs(think);
        self.advance_one(think);
    }
}

/// Every player's AI and the grid they share, the manager at `0x2163e4`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Computers {
    grid: Grid,
    players: Vec<Option<ComputerPlayer>>,
}

impl Computers {
    /// The AIs a new game builds (`0x6917e`): the grid over `nodes` and `world`, then one AI for
    /// each player `slots` puts in the game, in player order, whoever plays it, each by the
    /// profile its face gives in `portraits`.
    #[must_use]
    pub fn build(
        (nodes, world): (&NodeMap, &World),
        slots: &PlayerSlots,
        (portraits, profiles): (&Portraits, &ComputerProfiles),
        rng: &mut GameRng,
    ) -> Self {
        let grid = Grid::build(nodes, world);
        let players = Player::ALL
            .into_iter()
            .map(|player| {
                let slot = slots.get(player)?;
                let profile = portraits.get(slot.portrait).map_or(0, |row| row.profile);
                Some(ComputerPlayer::new(player, profile, profiles, rng))
            })
            .collect();
        Self { grid, players }
    }

    /// The grid.
    #[must_use]
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// `player`'s AI, when the game built one.
    #[must_use]
    pub fn get(&self, player: Player) -> Option<&ComputerPlayer> {
        self.players.get(player.index())?.as_ref()
    }

    #[cfg(test)]
    fn get_mut(&mut self, player: Player) -> Option<&mut ComputerPlayer> {
        self.players.get_mut(player.index())?.as_mut()
    }
}

impl Game {
    /// Every player's AI.
    #[must_use]
    pub fn computers(&self) -> &Computers {
        &self.computers
    }

    /// The computer players' part of the step (`0x270a3` to `0x2713f`): the grid pass, then the
    /// think of the one player whose turn it is, when the computer plays it. The tool pass that
    /// comes first is not modelled yet.
    pub(crate) fn computers_step(&mut self) {
        if self.clock.every(tools::TOOL_PERIOD, tools::TOOL_OFFSET) {
            for player in Player::ALL {
                let computer = self
                    .world
                    .slots()
                    .get(player)
                    .is_some_and(|slot| slot.controller == Controller::Computer);
                if computer {
                    self.tool_pass(player);
                }
            }
        }
        self.computers.grid.pass(&self.nodes, &self.world);
        let step = self.clock.step();
        let Some(player) = u8::try_from(step % THINK_EVERY).ok().and_then(Player::new) else {
            return;
        };
        let computer = self
            .world
            .slots()
            .get(player)
            .is_some_and(|slot| slot.controller == Controller::Computer);
        if !computer {
            return;
        }
        let mut computers = std::mem::take(&mut self.computers);
        let Computers { grid, players } = &mut computers;
        if let Some(Some(ai)) = players.get_mut(player.index()) {
            let mut think = Think {
                player,
                step,
                profile: ComputerProfiles::ORIGINAL.get(ai.profile),
                game: self,
                grid,
            };
            ai.think(step, &mut think);
        }
        self.computers = computers;
    }
}

#[cfg(test)]
mod tests;
