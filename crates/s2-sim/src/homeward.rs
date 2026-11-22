//! A lost settler's way back to a storehouse: the cross-country search, the path record it walks
//! and the walk in through the door.
//!
//! Not yet modelled: the player's people totals.

use std::hash::{Hash, Hasher};

use crate::figures::FigureKind;
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::ground::Property;
use crate::job::Job;
use crate::marks::RoadKind;
use crate::nodes::SiteFlags;
use crate::packed::Bits;
use crate::world::{MapObject, Player};

/// Most path records (the pool at `0x2016f8`, set up by `0x3d43d`).
pub(crate) const MAX_PATHS: usize = 2000;

/// Bytes a path record holds.
const PATH_BYTES: usize = 30;

/// The value a settler's search starts at, at its destination: a path is at most one shorter.
const SEARCH_START: u8 = 60;

/// The value a ship's search starts at (`0x3c650` called with 250).
const SEA_START: u8 = 250;

/// The value the search for a road being drawn starts at (`0x3c650` called with 20 by
/// `0x27a51`): a route is at most 19 steps long.
const ROAD_START: u8 = 20;

/// The mark the search leaves on a node a figure cannot walk onto.
const CLOSED: u8 = u8::MAX;

/// Most nodes in one ring of the search.
const RING_LIMIT: usize = 1000;

/// A path byte: the direction in bits 0 to 2 and the step count in bits 3 to 7.
const DIRECTION_FIELD: Bits = Bits::new(0, 3);
const STEPS_FIELD: Bits = Bits::new(3, 5);

/// Most steps in one run.
const MOST_PER_RUN: u8 = STEPS_FIELD.max();

/// The neighbours in the order the search and the wander test them: north-east, then east round
/// to north-west.
pub(crate) const RING: [Direction; 6] = [
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
];

/// The speed of a settler walking home and through a door (`0x322dc`).
pub(crate) const HOME_SPEED: u8 = 14;

/// Blocked steps a cross-country walker waits at a held node before it looks for another way,
/// the twenty-first included: `+0x0d` holds 0 on the first blocked step and the search runs on
/// the step it reaches 20 (`0x34a8e`, `0x3489d`).
pub(crate) const HELD_TRIES: u8 = 20;

/// What [`Game::held_up`] found at the next edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hold {
    /// Nothing holds the walker up.
    Clear,
    /// It waits this step.
    Waiting,
    /// It waits this step and has a new route to take on the next.
    Rerouted,
}

impl Hold {
    /// Whether the walker stays where it is this step.
    pub(crate) fn blocked(self) -> bool {
        self != Self::Clear
    }
}

/// Which nodes a cross-country path may cross: the predicate `0x33894` hands the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Crossing {
    /// Any node (`0x3c904`).
    Anywhere,
    /// Nodes figures can walk on (`0x3c8e8`).
    Walkable,
    /// Nodes figures can walk on with nobody standing there (`0x3c8a0`), which a walker held up
    /// by a fight searches with.
    Clear,
    /// Nodes of this sea (`0x40bdf`).
    Sea(u8),
    /// Nodes a road being drawn can cross (`0x279b0`): walkable, open to roads and with no road
    /// on them, the road being drawn included.
    Road,
    /// Nodes a water road being drawn can cross (`0x279f7`): a flag, or water all round with no
    /// road on the node.
    WaterRoad,
}

/// Steps in one direction: one byte of a path record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Run {
    direction: Direction,
    steps: u8,
}

impl Run {
    /// An unused byte.
    const EMPTY: Self = Self {
        direction: Direction::East,
        steps: 0,
    };

    /// The path byte.
    fn byte(self) -> u8 {
        let direction = u8::try_from(self.direction.index()).unwrap_or(0);
        DIRECTION_FIELD.set(direction) | STEPS_FIELD.set(self.steps)
    }
}

/// A path record (34 bytes in `S2.EXE`): runs of steps in one direction, from the figure to its
/// destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Path {
    read: u8,
    len: u8,
    runs: [Run; PATH_BYTES],
    /// Where the path leads: a storehouse's flag (`+0x20`, `+0x22`).
    pub to: Node,
}

impl Path {
    /// An empty record leading to `to`.
    pub(crate) fn empty(to: Node) -> Self {
        Self::encode(&[], to).0
    }

    /// Encodes `steps`, stopping at once when a change of direction finds the record full
    /// (`0x3c4a4`). Returns the path and whether it stopped there, at the last step.
    fn encode(steps: &[Direction], to: Node) -> (Self, bool) {
        let mut path = Self {
            read: 0,
            len: 0,
            runs: [Run::EMPTY; PATH_BYTES],
            to,
        };
        let mut written = 0;
        let mut run: Option<(Direction, u8)> = None;
        for &direction in steps {
            match run {
                Some((held, count)) if held == direction => run = Some((held, count + 1)),
                Some((held, count)) => {
                    if path.write_run(&mut written, held, count, true) {
                        path.len = u8::try_from(PATH_BYTES).unwrap_or(u8::MAX);
                        return (path, true);
                    }
                    run = Some((direction, 1));
                }
                None => run = Some((direction, 1)),
            }
        }
        if let Some((held, count)) = run
            && path.write_run(&mut written, held, count, false)
        {
            path.len = u8::try_from(PATH_BYTES).unwrap_or(u8::MAX);
            return (path, false);
        }
        path.len = u8::try_from(written).unwrap_or(u8::MAX);
        (path, false)
    }

    /// Writes a run of `count` steps towards `direction`, 31 to a byte. Returns true when the
    /// record filled: after any byte with `check_last` set, otherwise only after a full byte.
    fn write_run(
        &mut self,
        written: &mut usize,
        direction: Direction,
        mut count: u8,
        check_last: bool,
    ) -> bool {
        while count > MOST_PER_RUN {
            self.runs[*written] = Run {
                direction,
                steps: MOST_PER_RUN,
            };
            *written += 1;
            count -= MOST_PER_RUN;
            if *written == PATH_BYTES {
                return true;
            }
        }
        self.runs[*written] = Run {
            direction,
            steps: count,
        };
        *written += 1;
        check_last && *written == PATH_BYTES
    }

    /// The next step (`0x3d624`), or `None` when the path is used up.
    pub(crate) fn next(&mut self) -> Option<Direction> {
        if self.read >= self.len {
            return None;
        }
        if self.runs[usize::from(self.read)].steps == 0 {
            self.read += 1;
            if self.read >= self.len {
                return None;
            }
        }
        let run = &mut self.runs[usize::from(self.read)];
        run.steps -= 1;
        Some(run.direction)
    }
}

impl Hash for Path {
    /// Hashes the record's bytes, so state hashes match those of the packed form.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.read.hash(state);
        self.len.hash(state);
        self.runs.map(Run::byte).hash(state);
        self.to.hash(state);
    }
}

impl Game {
    /// Looks for a way home for lost settler `record` of `owner` and `job` on `node` with
    /// `countdown` left (`0x37300` step 2). Returns true when it found one and set off.
    pub(crate) fn try_rescue(
        &mut self,
        record: u16,
        owner: Player,
        job: Job,
        node: Node,
        countdown: u8,
    ) -> bool {
        let Some(flag) = self.nearest_storehouse(owner, job, node) else {
            return false;
        };
        let Some(path) = self.find_path(node, flag, Crossing::Walkable, true) else {
            return false;
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Homeward {
                owner,
                job,
                path,
                countdown,
            };
        }
        true
    }

    /// The flag of `owner`'s storehouse nearest to `node` that takes `job` in, by `|dx| + |dy|`
    /// on the stored coordinates with no wrap-around (`0x37174`); the lowest storehouse record
    /// among equals.
    fn nearest_storehouse(&self, owner: Player, job: Job, node: Node) -> Option<Node> {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let mut best: Option<(u16, Node)> = None;
        for (_, store) in self.world.storehouses() {
            if !store.takes_job(job) {
                continue;
            }
            let Some(building) = self.world.building(store.building) else {
                continue;
            };
            let flag_node = building.node.neighbour(Direction::SouthEast, width, height);
            let flag_owner = self
                .world
                .flag_at(flag_node)
                .and_then(|flag| self.world.flag(flag))
                .map(|flag| flag.owner);
            if flag_owner != Some(owner) {
                continue;
            }
            let distance = flag_node.x.abs_diff(node.x) + flag_node.y.abs_diff(node.y);
            if best.is_none_or(|(least, _)| distance < least) {
                best = Some((distance, flag_node));
            }
        }
        best.map(|(_, flag)| flag)
    }

    /// A path record for a cross-country path from `from` to `to` (`0x33894`), or `None` when
    /// the search fails or the pool is full.
    pub(crate) fn find_path(
        &mut self,
        from: Node,
        to: Node,
        crossing: Crossing,
        same_area: bool,
    ) -> Option<u16> {
        self.find_path_leaving(from, to, crossing, same_area).0
    }

    /// [`Self::find_path`], with the node the search leaves `S2.EXE`'s shared position on
    /// (`0x2016ac`), `None` when it does not get as far as moving it. A ring walk that runs a
    /// search carries on from there.
    pub(crate) fn find_path_leaving(
        &mut self,
        from: Node,
        to: Node,
        crossing: Crossing,
        same_area: bool,
    ) -> (Option<u16>, Option<Node>) {
        if self.world.paths.len() >= MAX_PATHS {
            return (None, None);
        }
        let (steps, cursor) = self.search_cross_country(from, to, crossing, same_area, SEARCH_START);
        let Some(steps) = steps else {
            return (None, cursor);
        };
        let (path, _) = Path::encode(&steps, to);
        (self.world.paths.insert(path, MAX_PATHS), cursor)
    }

    /// Searches a ship's route on sea `sea` from `from` to `to` into path record `path`
    /// (`0x3c650` with predicate `0x40bdf` and limit 250). Returns false, with the record left as
    /// it was, when there is no route.
    pub(crate) fn sea_route(&mut self, path: u16, from: Node, to: Node, sea: u8) -> bool {
        let Some(steps) = self.cross_country(from, to, Crossing::Sea(sea), false, SEA_START) else {
            return false;
        };
        if let Some(record) = self.world.paths.get_mut(path) {
            *record = Path::encode(&steps, to).0;
        }
        true
    }

    /// Searches a route over the nodes `crossing` allows from `from` to path record `path`'s
    /// target into that record (`0x3c650`, limit 60), in the same area. Returns false, with the
    /// record left as it was, when there is no route.
    pub(crate) fn route_into(&mut self, path: u16, from: Node, crossing: Crossing) -> bool {
        let Some(to) = self.world.paths.get(path).map(|record| record.to) else {
            return false;
        };
        let Some(steps) = self.cross_country(from, to, crossing, true, SEARCH_START) else {
            return false;
        };
        if let Some(record) = self.world.paths.get_mut(path) {
            *record = Path::encode(&steps, to).0;
        }
        true
    }

    /// The route a click on `to` plans for a road of `kind` being drawn whose head is `from`
    /// (`0x3c650` from `0x27a51`). `None` when there is none within [`ROAD_START`] − 1 steps;
    /// the search draws from the game's generator only once it has found one.
    pub(crate) fn road_route(&mut self, from: Node, to: Node, kind: RoadKind) -> Option<Vec<Direction>> {
        let crossing = if kind == RoadKind::Water {
            Crossing::WaterRoad
        } else {
            Crossing::Road
        };
        self.cross_country(from, to, crossing, false, ROAD_START)
    }

    /// Whether a fight stops cross-country walker `record` from taking the edge to `next`
    /// (`0x3473c`). A figure with a soldier record looks for a soldier standing there, anything
    /// else reads the held bit (`0x345f8`). A walker that may go has its wait count cleared; one
    /// that may not counts this step, and on the step its count passes [`HELD_TRIES`] searches
    /// a new route to `path`'s target into the same record. A route found is taken next step,
    /// even when it is the route the record already held,
    /// and when its first edge is held too the walker waits out a fresh count before it searches
    /// again (`0x34a71` clears it as the wait begins). With none the old route is kept and the
    /// next search comes 20 blocked steps later: `S2.EXE` clears `+0x0d` and searches when it
    /// reaches 20 again, where the first wait's step clears it without a search, so the count
    /// here, one ahead of `+0x0d` through that first wait, starts again at 1.
    pub(crate) fn held_up(&mut self, record: u16, next: Node, path: u16) -> Hold {
        let Some(figure) = self.world.figure(record).copied() else {
            return Hold::Clear;
        };
        let blocked = if self.fighting_soldier(record) {
            self.held_back(record, next)
        } else {
            self.nodes.get(next).flags.contains(crate::nodes::NodeFlags::HELD)
        };
        if !blocked {
            if let Some(figure) = self.world.figure_mut(record) {
                figure.waits = 0;
            }
            return Hold::Clear;
        }
        let waited = figure.waits + 1;
        if waited <= HELD_TRIES {
            if let Some(figure) = self.world.figure_mut(record) {
                figure.waits = waited;
            }
            return Hold::Waiting;
        }
        let found = self.route_into(path, figure.node(), Crossing::Clear);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.waits = u8::from(!found);
        }
        if found { Hold::Rerouted } else { Hold::Waiting }
    }

    /// The cross-country search (`0x3c650`, `0x3c4a4`): rings outward from `to` over the nodes
    /// `crossing` allows until `from` is reached, at most `start` − 1 steps away. With `same_area`
    /// both must be in the same area. The path back goes to a neighbour one ring nearer at each step,
    /// chosen by a draw even when there is only one; the draws stop where the path record fills.
    fn cross_country(
        &mut self,
        from: Node,
        to: Node,
        crossing: Crossing,
        same_area: bool,
        start: u8,
    ) -> Option<Vec<Direction>> {
        self.search_cross_country(from, to, crossing, same_area, start).0
    }

    /// [`Self::cross_country`], with where it leaves the shared position: untouched (`None`) when
    /// the two ends lie in different areas; the north-west neighbour of the last node it
    /// spread from when it runs out of rings or nodes; the node that filled a ring; and on
    /// success `to`, or the north-west neighbour of the node where the path record filled.
    fn search_cross_country(
        &mut self,
        from: Node,
        to: Node,
        crossing: Crossing,
        same_area: bool,
        start: u8,
    ) -> (Option<Vec<Direction>>, Option<Node>) {
        if same_area && self.nodes.get(from).area != self.nodes.get(to).area {
            return (None, None);
        }
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let index = |node: Node| usize::from(node.y) * usize::from(width) + usize::from(node.x);
        let mut marks = vec![0_u8; usize::from(width) * usize::from(height)];
        marks[index(to)] = start;
        let mut frontier = vec![to];
        let mut level = start - 1;
        let mut found = false;
        let mut cursor = to;
        'rings: while level > 0 {
            let mut next = Vec::new();
            for &node in &frontier {
                cursor = node.neighbour(Direction::NorthWest, width, height);
                for direction in RING {
                    let near = node.neighbour(direction, width, height);
                    let mark = &mut marks[index(near)];
                    if *mark != 0 {
                        continue;
                    }
                    if near == from {
                        *mark = level;
                        found = true;
                        break 'rings;
                    }
                    let open = match crossing {
                        Crossing::Anywhere => true,
                        Crossing::Walkable => self.nodes.get(near).sites.passable(),
                        Crossing::Clear => {
                            self.nodes.get(near).sites.passable()
                                && self.world.figures_at(near).next().is_none()
                        }
                        Crossing::Sea(sea) => self.nodes.get(near).area == sea,
                        Crossing::Road => {
                            let data = self.nodes.get(near);
                            data.sites.passable()
                                && !data.sites.flags.contains(SiteFlags::NO_ROAD)
                                && data.roads.is_empty()
                        }
                        Crossing::WaterRoad => {
                            self.world.flag_at(near).is_some()
                                || (self.nodes.property(near).contains(Property::ALL_WATER)
                                    && self.nodes.get(near).roads.is_empty())
                        }
                    };
                    if open {
                        *mark = level;
                        next.push(near);
                        if next.len() == RING_LIMIT {
                            return (None, Some(near));
                        }
                    } else {
                        *mark = CLOSED;
                    }
                }
            }
            if next.is_empty() {
                return (None, Some(cursor));
            }
            frontier = next;
            level -= 1;
        }
        if !found {
            return (None, Some(cursor));
        }
        let mut steps = Vec::new();
        let mut at = from;
        let mut mark = marks[index(from)];
        while mark < start {
            mark += 1;
            let nearer: Vec<Direction> = RING
                .into_iter()
                .filter(|&direction| marks[index(at.neighbour(direction, width, height))] == mark)
                .collect();
            let pick = self
                .rng
                .up_to(u16::try_from(nearer.len().saturating_sub(1)).unwrap_or(0));
            let Some(&direction) = nearer.get(usize::from(pick)) else {
                return (None, Some(at));
            };
            steps.push(direction);
            if Path::encode(&steps, to).1 {
                return (
                    Some(steps),
                    Some(at.neighbour(Direction::NorthWest, width, height)),
                );
            }
            at = at.neighbour(direction, width, height);
        }
        (Some(steps), Some(at))
    }

    /// One step of settler `record` walking home that is not walking (`0x3473c`, sub-state 3):
    /// at the path's end it turns in at the storehouse's door; a path used up short of it, or
    /// no path, leaves it lost again.
    pub(crate) fn step_homeward(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Homeward { path, owner, job, .. } = figure.kind else {
            return;
        };
        let Some(path_record) = self.world.paths.get_mut(path) else {
            self.make_lost(record);
            return;
        };
        let step = path_record.next();
        let to = path_record.to;
        let node = figure.node();
        if node == to {
            self.reach_storehouse(record);
            return;
        }
        let Some(direction) = step else {
            self.make_lost(record);
            return;
        };
        let next = node.neighbour(direction, self.nodes.width(), self.nodes.height());
        self.scout_ahead(job, owner, next);
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, HOME_SPEED);
        }
    }

    /// A settler walking home stands on its path's last node (`0x33b30`, `0x33a00`): its path is
    /// freed, and at a storehouse's flag it walks in through the door. Anywhere else it is lost
    /// again.
    fn reach_storehouse(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Homeward { owner, job, path, .. } = figure.kind else {
            return;
        };
        self.world.paths.remove(path);
        let node = figure.node();
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let door = node.neighbour(Direction::NorthWest, width, height);
        let storehouse = self.world.flag_at(node).is_some()
            && matches!(
                self.world.object(door),
                MapObject::Building(building)
                    if self.world.building(building).is_some_and(crate::world::Building::is_storehouse)
            );
        if !storehouse {
            self.make_lost(record);
            return;
        }
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Entering { owner, job };
        }
        self.world.move_figure(record, door);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, HOME_SPEED);
        }
    }

    /// A settler has walked in through a door (`0x3596c`, `0x360b9`): a storehouse on its node
    /// counts it in ([`crate::world::Storehouse::take_in`]) and the figure is gone. Anywhere else it is lost again.
    pub(crate) fn step_entering(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Entering { job, .. } = figure.kind else {
            return;
        };
        let store = match self.world.object(figure.node()) {
            MapObject::Building(building) => self
                .world
                .building(building)
                .filter(|record| record.is_storehouse())
                .and_then(|_| self.world.storehouse_of(building))
                .map(|(store, _)| store),
            _ => None,
        };
        let Some(store) = store else {
            self.make_lost(record);
            return;
        };
        let fixes = self.world.bug_fixes;
        if let Some(store) = self.world.storehouse_mut(store) {
            store.take_in(job, fixes);
        }
        self.world.remove_figure(record);
    }
}

#[cfg(test)]
mod tests;
