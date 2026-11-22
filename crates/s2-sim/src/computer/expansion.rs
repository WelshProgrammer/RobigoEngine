//! How a computer player pushes its border out: task 9 sends scouts from its headquarters, its
//! storehouses and now and then for ore (`0x6eeca`), task 7 picks where each scout's search leads
//! (`0x6cb63`), and task 6 orders a military building on the player's land towards it
//! (`0x6c617`).

use super::chain::{AROUND, BARRED, DONE, FRONTIER};
use super::economy::{headquarters, next_storehouse};
use super::jobs::{Expansion, Job, JobKind, Payload, Placement, ScoutMode};
use super::tasks::Think;
use super::{ComputerPlayer, TaskKind};
use crate::construction::fits;
use crate::grid::{Direction, Node};
use crate::world::{BuildingKind, BuildingState, MapObject, Player, Size};

/// Task 9's call counter starts this many calls apart from one player to the next (`0x6ee76`).
const STAGGER: u16 = 40;

/// The storehouses, harbours and headquarters a player has before task 9 scouts round them less
/// often (`0x6ef06`).
const MANY_STOREHOUSES: u16 = 5;

/// The most candidates a search for land collects (`0x6cb53`).
const CANDIDATES: usize = 20;

/// The rings round a candidate a search for land closes (`0x6caf1`).
const CANDIDATE_RINGS: u16 = 3;

/// Nodes on a cell's side, and a cell's centre from its corner.
const CELL_NODES: u16 = 8;
const CELL_CENTRE: u16 = 4;

/// A cell's ore below this is not worth a search (`0x6cd03`).
const LEAST_ORE: u8 = 10;

/// The chance in hundredths, against a draw of 0 to 100, that a search for ore scores a cell
/// (`0x6cd1e`).
const SCORE_CHANCE: u16 = 75;

/// The base of a cell's score in a search for ore (`0x6cd33`).
const SCORE_BASE: u32 = 100;

/// The rings the plan's floods start from at the target (`0x6c4cd`).
const PLAN_DEPTH: u8 = 40;

/// The cost to a storehouse past which a flag is no use to the plan (`0x6c174`).
const PLAN_COST: u16 = 50;

/// The plan scores a flag from this, less twice its ring and its cost (`0x6c16b`).
const PLAN_SCORE: u32 = 1000;

/// How far the threat count floods round the chosen node (`0x6c6fa`).
const THREAT_REACH: u8 = 30;

/// The threat above which a barracks is never chosen (`0x6c70c`).
const SAFE: u16 = 8;

/// The chance in hundredths of a barracks where it is safe, and of a guardhouse over a
/// watchtower otherwise, each against a draw of 0 to 100 taken as at most (`0x6c723`, `0x6c73a`).
const BARRACKS_CHANCE: u16 = 25;
const GUARDHOUSE_CHANCE: u16 = 20;

/// The radius task 6 asks its placement for (`0x6c784`).
const PLACE_RADIUS: u16 = 12;

/// Task 9's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AreaScans {
    /// Calls so far, from 40 times the player's number (`+0x06`).
    pub calls: u16,
    /// The storehouse the last search round one looked from, `None` when there was none (`+0x08`).
    pub storehouse: Option<u16>,
    /// The storehouse the last search for ore looked from (`+0x0c`).
    pub ore: Option<u16>,
}

impl Default for AreaScans {
    fn default() -> Self {
        Self {
            calls: 0,
            storehouse: Some(0),
            ore: Some(0),
        }
    }
}

impl AreaScans {
    /// Task 9's fields as its constructor sets them for `player` (`0x6ee4e`).
    #[must_use]
    pub fn new(player: Player) -> Self {
        let index = u16::try_from(player.index()).unwrap_or_default();
        Self {
            calls: index * STAGGER,
            storehouse: Some(0),
            ore: Some(0),
        }
    }
}

/// What a flood does with a node it reaches (`0x3c90c`'s callbacks).
pub(super) enum Visit {
    /// Close it.
    Block,
    /// Go on through it.
    Pass,
    /// End the flood there.
    Stop,
}

/// How the plan's floods ended.
enum Reach {
    /// The flood ran out of rings or of nodes.
    Spent,
    /// A ring held 1,000 nodes.
    Overflowed,
    /// The flag looked for was met, with the flood's marks.
    Met(Vec<u8>),
}

impl Think<'_> {
    /// The flood at `0x3c90c` from `start` over `depth − 1` rings, each ring marked one lower than
    /// the last from `depth` at the start. Nodes already marked are passed over; with
    /// `same_area`, so is a node in another area than the start, closed. `visit` hears every other
    /// node, with the marks so far. A ring of 1,000 nodes ends the flood. Returns the marks.
    pub(super) fn survey(
        &self,
        start: Node,
        depth: u8,
        same_area: bool,
        mut visit: impl FnMut(Node, &mut [u8]) -> Visit,
    ) -> Vec<u8> {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let at = |node: Node| usize::from(node.y) * usize::from(width) + usize::from(node.x);
        let mut marks = vec![0u8; usize::from(width) * usize::from(height)];
        marks[at(start)] = depth;
        let area = self.game.nodes.get(start).area;
        let mut frontier = vec![start];
        let mut level = u16::from(depth).wrapping_sub(1);
        while level > 0 {
            let mark = u8::try_from(level & u16::from(u8::MAX)).unwrap_or(u8::MAX);
            let mut next = Vec::new();
            for &from in &frontier {
                for &way in &AROUND {
                    let node = from.neighbour(way, width, height);
                    if marks[at(node)] != 0 {
                        continue;
                    }
                    if same_area && self.game.nodes.get(node).area != area {
                        marks[at(node)] = BARRED;
                        continue;
                    }
                    match visit(node, &mut marks) {
                        Visit::Block => marks[at(node)] = BARRED,
                        Visit::Stop => {
                            marks[at(node)] = mark;
                            return marks;
                        }
                        Visit::Pass => {
                            marks[at(node)] = mark;
                            next.push(node);
                            if next.len() >= FRONTIER {
                                return marks;
                            }
                        }
                    }
                }
            }
            if next.is_empty() {
                return marks;
            }
            frontier = next;
            level -= 1;
        }
        marks
    }

    /// Other players' military weight on the nodes a flood of radius 30 in the same area reaches
    /// from `node`, sites included (`0x68bc3`, `0x68b19`). A building's owner is its flag's, so a
    /// captured building counts for its new owner.
    fn threat(&self, node: Node) -> u16 {
        let mut threat: u16 = 0;
        self.survey(node, THREAT_REACH, true, |at, _| {
            if let MapObject::Building(record) = self.game.world.object(at)
                && let Some(building) = self.game.world.building(record)
                && self.game.world.building_owner(record) != Some(self.player)
            {
                threat = threat.wrapping_add(military_weight(building.kind));
            }
            Visit::Pass
        });
        threat
    }

    /// The flood of the expansion's plan (`0x6bf98`, `0x6c1f4`) from `target` over 39 rings, over
    /// flags of any owner and over the nodes stepped onto along a walkable edge, as
    /// [`Think::flood`] tests it, that figures can walk onto. `met` hears each flag with its ring
    /// and ends the flood when it answers true.
    fn plan_flood(&self, target: Node, mut met: impl FnMut(u16, Node, u16) -> bool) -> Reach {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let at = |node: Node| usize::from(node.y) * usize::from(width) + usize::from(node.x);
        let mut marks = vec![0u8; usize::from(width) * usize::from(height)];
        marks[at(target)] = PLAN_DEPTH;
        let mut frontier = vec![target];
        let mut level = PLAN_DEPTH;
        loop {
            level -= 1;
            if level == 0 {
                return Reach::Spent;
            }
            let ring = u16::from(PLAN_DEPTH - level);
            let mut next = Vec::new();
            for &from in &frontier {
                for (d, &way) in AROUND.iter().enumerate() {
                    let node = from.neighbour(way, width, height);
                    if marks[at(node)] != 0 {
                        continue;
                    }
                    if let Some(flag) = self.game.world.flag_at(node) {
                        marks[at(node)] = level;
                        if met(flag, node, ring) {
                            return Reach::Met(marks);
                        }
                    } else if !self.game.nodes.edge_walkable(from, Direction::ALL[(d + 4) % 6])
                        || !self.game.nodes.get(node).sites.passable()
                    {
                        marks[at(node)] = BARRED;
                        continue;
                    }
                    marks[at(node)] = level;
                    next.push(node);
                    if next.len() == FRONTIER {
                        return Reach::Overflowed;
                    }
                }
            }
            if next.is_empty() {
                return Reach::Spent;
            }
            frontier = next;
        }
    }

    /// Where to build towards `target` (`0x6c4c8`): the player's flag the plan's flood scores
    /// best, 1000 less twice its ring and its cost to a storehouse, then from that flag back up
    /// the second flood's marks towards the target, the last of the player's nodes before the
    /// first that is not.
    fn plan(&self, target: Node) -> Option<Node> {
        let mut best = None;
        let mut best_score = 0;
        let first = self.plan_flood(target, |flag, node, ring| {
            if self.game.nodes.get(node).owner == Some(self.player)
                && let Some(cost) = self.storehouse_cost(flag, PLAN_COST)
            {
                let score = PLAN_SCORE - 2 * u32::from(ring) - u32::from(cost);
                if score > best_score {
                    best_score = score;
                    best = Some(flag);
                }
            }
            false
        });
        if matches!(first, Reach::Overflowed) {
            return None;
        }
        let best = best?;
        let Reach::Met(marks) = self.plan_flood(target, |flag, _, _| flag == best) else {
            return None;
        };
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let mark = |node: Node| marks[usize::from(node.y) * usize::from(width) + usize::from(node.x)];
        let mut chosen = self.game.world.flag(best)?.node;
        let mut wanted = mark(chosen).wrapping_add(1);
        loop {
            let next = std::iter::once(AROUND[5])
                .chain(AROUND[..5].iter().copied())
                .map(|way| chosen.neighbour(way, width, height))
                .find(|&node| mark(node) == wanted)?;
            if self.game.nodes.get(next).owner != Some(self.player) {
                return Some(chosen);
            }
            chosen = next;
            wanted = wanted.wrapping_add(1);
        }
    }
}

/// What a building of `kind` adds to the threat count (`0x68b8d`).
fn military_weight(kind: BuildingKind) -> u16 {
    match kind {
        BuildingKind::HEADQUARTERS => 12,
        BuildingKind::BARRACKS => 1,
        BuildingKind::GUARDHOUSE | BuildingKind::CATAPULT => 2,
        BuildingKind::WATCHTOWER => 4,
        BuildingKind::FORTRESS => 6,
        _ => 0,
    }
}

impl ComputerPlayer {
    /// Task 9's call (`0x6eeca`): every so many calls a search for land round the headquarters,
    /// one round the next storehouse other than the headquarters, and one for ore round the next
    /// storehouse.
    pub(super) fn scans(&mut self, think: &mut Think<'_>) {
        let numbers = think.profile.scans;
        let scans = &mut self.tasks.scans;
        scans.calls = scans.calls.wrapping_add(1);
        let calls = scans.calls;
        let storehouses = [
            BuildingKind::STOREHOUSE,
            BuildingKind::HARBOUR,
            BuildingKind::HEADQUARTERS,
        ]
        .into_iter()
        .map(|kind| think.game.world.finished_buildings(think.player, kind))
        .fold(0u16, u16::wrapping_add);
        let every = if storehouses < MANY_STOREHOUSES {
            numbers.storehouse_every.0
        } else {
            numbers.storehouse_every.1
        };
        if calls.is_multiple_of(numbers.headquarters_every.max(1)) {
            let (x, y) = headquarters(&think.game.world, &think.game.nodes, think.player);
            if x >= 0 {
                let node = Node {
                    x: x.cast_unsigned(),
                    y: y.cast_unsigned(),
                };
                self.add(Job::scout(node, ScoutMode::Land, 0, think.step), None, None);
            }
        }
        if calls.is_multiple_of(every.max(1)) {
            let next = next_storehouse(
                &think.game.world,
                &think.game.nodes,
                self.tasks.scans.storehouse,
                think.player,
            );
            self.tasks.scans.storehouse = next;
            if let Some(building) = next.and_then(|record| think.game.world.building(record))
                && building.kind != BuildingKind::HEADQUARTERS
            {
                let job = Job::scout(
                    building.node,
                    ScoutMode::Land,
                    numbers.storehouse_reach,
                    think.step,
                );
                self.add(job, None, None);
            }
        }
        if calls.is_multiple_of(numbers.ore_every.max(1)) {
            let next = next_storehouse(
                &think.game.world,
                &think.game.nodes,
                self.tasks.scans.ore,
                think.player,
            );
            self.tasks.scans.ore = next;
            let Some(node) = next
                .and_then(|record| think.game.world.building(record))
                .map(|building| building.node)
            else {
                return;
            };
            let mut draw = i32::from(think.game.rng.up_to(100));
            let mut mode = None;
            for (chance, found) in
                numbers
                    .ore_chances
                    .into_iter()
                    .zip([ScoutMode::Gold, ScoutMode::Iron, ScoutMode::Coal])
            {
                draw -= i32::from(chance);
                if draw <= 0 {
                    mode = Some(found);
                    break;
                }
            }
            if let Some(mode) = mode {
                self.add(Job::scout(node, mode, 0, think.step), None, None);
            }
        }
    }

    /// A scout's turn (`0x6cb63`): it picks a target, makes an expansion towards it with no owner,
    /// and ends.
    pub(super) fn scout_turn(&mut self, index: u16, think: &mut Think<'_>) -> u16 {
        let Some(Payload::Scout { node, mode, radius }) = self.job(index).map(|job| job.payload) else {
            return DONE;
        };
        let target = match mode {
            ScoutMode::Land => Self::land(node, radius, think),
            ScoutMode::Gold | ScoutMode::Iron | ScoutMode::Coal => Self::ore(node, mode, think),
        };
        if let Some(target) = target {
            self.add(Job::expand(target, think.step), None, None);
        }
        self.end(index);
        DONE
    }

    /// A search for land (`0x6cbc0`): a flood in the same area from `node` passes over the
    /// player's land and anywhere no building fits. A node off it where a small building fits
    /// is a candidate, closed with the three rings round it; 20 end the flood. A draw picks one.
    fn land(node: Node, radius: u16, think: &mut Think<'_>) -> Option<Node> {
        let reach = if radius == 0 {
            think.profile.scans.reach
        } else {
            radius
        };
        let depth = u8::try_from(reach & u16::from(u8::MAX)).unwrap_or(u8::MAX);
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        let mut candidates = Vec::new();
        think.survey(node, depth, true, |at, marks| {
            let data = think.game.nodes.get(at);
            if data.owner == Some(think.player) || !fits(Size::Small, data.sites.size) {
                return Visit::Pass;
            }
            candidates.push(at);
            for ring in 1..=CANDIDATE_RINGS {
                for near in at.ring(ring, width, height) {
                    let mark = &mut marks[usize::from(near.y) * usize::from(width) + usize::from(near.x)];
                    if *mark == 0 {
                        *mark = BARRED;
                    }
                }
            }
            if candidates.len() == CANDIDATES {
                Visit::Stop
            } else {
                Visit::Block
            }
        });
        let last = u16::try_from(candidates.len()).ok()?.checked_sub(1)?;
        let pick = think.game.rng.between(0, last);
        candidates.get(usize::from(pick)).copied()
    }

    /// A search for ore (`0x6cc9e`): the cells in the square of the profile's half-width round
    /// `node`'s, row by row, on the grid, with their centre in `node`'s area and not the player's.
    /// Each with more than 10 of the ore draws, and below 75 scores the amount plus 100 less its
    /// distance in cells either way. The best centre wins, the first on a tie.
    fn ore(node: Node, mode: ScoutMode, think: &mut Think<'_>) -> Option<Node> {
        let reach = i32::from(think.profile.scans.ore_cells);
        let (column, row) = (i32::from(node.x / CELL_NODES), i32::from(node.y / CELL_NODES));
        let area = think.game.nodes.get(node).area;
        let (columns, rows) = think.grid.size();
        let mut best = None;
        let mut best_score = 0;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (Ok(cx), Ok(cy)) = (u16::try_from(column + dx), u16::try_from(row + dy)) else {
                    continue;
                };
                if cx >= columns || cy >= rows {
                    continue;
                }
                let centre = Node {
                    x: cx * CELL_NODES + CELL_CENTRE,
                    y: cy * CELL_NODES + CELL_CENTRE,
                };
                let data = think.game.nodes.get(centre);
                if data.area != area || data.owner == Some(think.player) {
                    continue;
                }
                let Some(cell) = think.grid.cell(cx, cy) else {
                    continue;
                };
                let amount = match mode {
                    ScoutMode::Gold => cell.ore[2],
                    ScoutMode::Iron => cell.ore[0],
                    _ => cell.ore[1],
                };
                if amount <= LEAST_ORE || think.game.rng.up_to(100) >= SCORE_CHANCE {
                    continue;
                }
                let score = (u32::from(amount) + SCORE_BASE)
                    .wrapping_sub(dy.unsigned_abs())
                    .wrapping_sub(dx.unsigned_abs());
                if score > best_score {
                    best_score = score;
                    best = Some(centre);
                }
            }
        }
        best
    }

    /// An expansion's turn (`0x6c617`). It ends once the player owns the target. A building
    /// placed is waited on until finished and manned, then the expansion plans again; one gone
    /// fails it. Otherwise it plans and places a barracks, guardhouse or watchtower there,
    /// picked by draws and by the threat round it.
    pub(super) fn expand_turn(&mut self, index: u16, think: &mut Think<'_>) -> u16 {
        let Some(Payload::Expand {
            target,
            state,
            chosen,
            building,
        }) = self.job(index).map(|job| job.payload)
        else {
            return DONE;
        };
        if think.game.nodes.get(target).owner == Some(think.player) {
            self.end(index);
            return DONE;
        }
        let mut state = state;
        if state == Expansion::Building {
            let standing = match think.game.world.object(chosen) {
                MapObject::Building(record) => Some(record),
                _ => None,
            };
            if standing == building {
                let ready = building
                    .and_then(|record| think.game.world.building(record))
                    .is_some_and(|it| it.state.contains(BuildingState::FINISHED | BuildingState::MANNED));
                if ready {
                    self.set_expansion(index, |state, _, _| *state = Expansion::Plan);
                }
                return DONE;
            }
            state = Expansion::Failed;
            self.set_expansion(index, |at, _, _| *at = Expansion::Failed);
        }
        if state == Expansion::Failed {
            self.end(index);
            return 0;
        }
        let Some(site) = think.plan(target) else {
            self.end(index);
            return 0;
        };
        self.set_expansion(index, |_, chosen, _| *chosen = site);
        let kind = if think.threat(site) <= SAFE && think.game.rng.up_to(100) <= BARRACKS_CHANCE {
            BuildingKind::BARRACKS
        } else if think.game.rng.up_to(100) > GUARDHOUSE_CHANCE {
            BuildingKind::GUARDHOUSE
        } else {
            BuildingKind::WATCHTOWER
        };
        let placement = Placement {
            kind,
            node: site,
            radius: PLACE_RADIUS,
            found: 0,
            reports: 0,
            extra: 0,
        };
        let job = Job::with(JobKind::Place, Payload::Place(placement), think.step);
        self.add(job, Some(index), Some(TaskKind::Expand));
        self.set_expansion(index, |state, _, _| *state = Expansion::Ordered);
        self.rest(index);
        DONE
    }

    /// An expansion hears its placement (`0x6c7db`): it runs again, and a placement done is
    /// committed and its building waited on. Anything else fails it.
    pub(super) fn expand_heard(&mut self, index: u16, result: u16, extra: u32, think: &mut Think<'_>) {
        self.run(index);
        let committed = result == DONE
            && self
                .child_of(index, JobKind::Place)
                .is_some_and(|placement| self.commit(placement, think));
        if !committed {
            self.set_expansion(index, |state, _, _| *state = Expansion::Failed);
            return;
        }
        let site = Node {
            x: u16::try_from(extra & u32::from(u16::MAX)).unwrap_or_default(),
            y: u16::try_from(extra >> u16::BITS).unwrap_or_default(),
        };
        let standing = match think.game.world.object(site) {
            MapObject::Building(record) => Some(record),
            _ => None,
        };
        self.set_expansion(index, |state, chosen, building| {
            *chosen = site;
            *building = standing;
            *state = if standing.is_some() {
                Expansion::Building
            } else {
                Expansion::Failed
            };
        });
    }

    /// Changes expansion `index`'s state, chosen node and building.
    fn set_expansion(
        &mut self,
        index: u16,
        change: impl FnOnce(&mut Expansion, &mut Node, &mut Option<u16>),
    ) {
        if let Some(Job {
            payload:
                Payload::Expand {
                    state,
                    chosen,
                    building,
                    ..
                },
            ..
        }) = self.job_mut(index)
        {
            change(state, chosen, building);
        }
    }
}

#[cfg(test)]
mod tests;
