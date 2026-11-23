//! How a computer player places a building with its road: the build order (task 8), the
//! placement (task 4), the site search (task 1), the reservation (task 0) and the road (task 2).

use super::jobs::{Job, JobFlags, JobKind, Payload, Placement, SiteSearch};
use super::tasks::Think;
use super::{ComputerPlayer, TaskKind};
use crate::grid::{Direction, Node};
use crate::marks::RoadKind;
use crate::nodes::SiteFlags;
use crate::world::{BuildingKind, BuildingState, Category, FlagKind, MapObject, Size};
use crate::{SearchOptions, search};

/// What a handler answers when it is done with a job.
pub(super) const DONE: u16 = 100;

/// The site search's spiral ends at this ring (`0x6a750`).
const LAST_RING: u16 = 12;

/// A build order gives up after this many placements (`0x72e2b`).
const ATTEMPTS: u16 = 5;

/// What a build order's `result` holds after its placement reports.
const PLACED: u16 = 2;
const FAILED: u16 = 4;
const RETRY: u16 = 8;

/// What a placement's `found` holds.
const SITE_FOUND: u16 = 1;
const RESERVED: u16 = 2;

/// The road search scores a flag from this, less twice its ring and its cost (`0x6b165`).
const SCORE: i32 = 1000;

/// The road search's cost limit to a storehouse (`0x6b16e`).
const COST_LIMIT: u16 = 200;

/// The most nodes a ring of the road search holds (`0x6b1c1`).
pub(super) const FRONTIER: usize = 1000;

/// A node the road search leaves alone.
pub(super) const BARRED: u8 = u8::MAX;

/// The neighbour order of the road search and its walk back: north-east, then round from east.
pub(super) const AROUND: [Direction; 6] = [
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
];

/// The farm, whose fields keep other buildings away, and its reach (`0x1fdc10`).
const FARM_REACH: u16 = 3;

/// How far a type keeps its own kind away (`0x1fdc50`).
fn own_kind_reach(kind: BuildingKind) -> u16 {
    match kind {
        BuildingKind::WOODCUTTER | BuildingKind::QUARRY => 4,
        BuildingKind::FISHERY | BuildingKind::FARM => 6,
        BuildingKind::GOLD_MINE
        | BuildingKind::IRON_MINE
        | BuildingKind::COAL_MINE
        | BuildingKind::GRANITE_MINE => 3,
        _ => 0,
    }
}

/// The mineral bits a mine needs (`0x1fdcf0`).
fn mineral_of(kind: BuildingKind) -> u8 {
    match kind {
        BuildingKind::GOLD_MINE => 16,
        BuildingKind::IRON_MINE => 8,
        BuildingKind::GRANITE_MINE => 24,
        _ => 0,
    }
}

impl ComputerPlayer {
    /// Adds a build order for `kind` at `node` under `parent`, owned by `owner`. One the mission
    /// bans the computer players from is lost, as an order is with every job slot taken, and
    /// `None` comes back. The ban comes after any draws the order depends on, as `S2.EXE`'s
    /// catapult check sits after its draw (`0x729c7`).
    pub(super) fn add_order(
        &mut self,
        think: &Think<'_>,
        (kind, node, radius): (BuildingKind, Node, u16),
        parent: Option<u16>,
        owner: Option<TaskKind>,
    ) -> Option<u16> {
        if think.game.computers_never_build(kind) {
            return None;
        }
        self.add(Job::build_order(kind, node, radius, think.step), parent, owner)
    }

    pub(super) fn job_mut(&mut self, index: u16) -> Option<&mut Job> {
        self.jobs.get_mut(usize::from(index))?.as_mut()
    }

    pub(super) fn set(&mut self, index: u16, flags: JobFlags) {
        if let Some(job) = self.job_mut(index) {
            job.flags |= flags;
        }
    }

    /// Takes job `index` and names `handler` its handler (`+0x14`).
    pub(super) fn take(&mut self, index: u16, handler: TaskKind, accepted: bool) {
        if let Some(job) = self.job_mut(index) {
            job.handler = Some(handler);
            if accepted {
                job.flags |= JobFlags::ACCEPTED;
            }
        }
    }

    /// Puts job `index` on the running list (`0x69bbb`).
    pub(super) fn run(&mut self, index: u16) {
        self.running.add(index);
        self.set(index, JobFlags::RUNNING);
    }

    /// Takes job `index` off the running list (`0x69be8`).
    pub(super) fn rest(&mut self, index: u16) {
        if self
            .job(index)
            .is_some_and(|job| job.flags.contains(JobFlags::RUNNING))
        {
            self.running.remove(index);
            if let Some(job) = self.job_mut(index) {
                job.flags -= JobFlags::RUNNING;
            }
        }
    }

    /// The first child of job `index` of `kind` (`0x6a039`).
    pub(super) fn child_of(&self, index: u16, kind: JobKind) -> Option<u16> {
        let mut child = self.job(index)?.child;
        while let Some(at) = child {
            let job = self.job(at)?;
            if job.kind == kind {
                return Some(at);
            }
            child = job.next;
        }
        None
    }

    /// Tells job `index`'s owner how it went (`0x69fe9`).
    pub(super) fn report(&mut self, index: u16, result: u16, extra: u32, think: &mut Think<'_>) {
        let Some(job) = self.job(index).copied() else {
            return;
        };
        let (Some(owner), Some(parent)) = (job.owner, job.parent) else {
            return;
        };
        match owner {
            TaskKind::Site => self.site_heard(parent, index, result, think),
            TaskKind::Place => self.place_heard(parent, index, result, extra, think),
            TaskKind::Orders => self.order_heard(parent, result, extra),
            TaskKind::Projects => self.project_heard(parent, result, think),
            TaskKind::Expand => self.expand_heard(parent, result, extra, think),
            _ => {}
        }
    }

    /// Offers job `index` to its handler `handler` (`+0x0c`).
    pub(super) fn accept(&mut self, handler: TaskKind, index: u16, think: &mut Think<'_>) -> u16 {
        match handler {
            TaskKind::Reserve | TaskKind::Shortcut => {
                self.take(index, handler, false);
                self.run(index);
                0
            }
            TaskKind::Site => {
                self.take(index, handler, true);
                self.run(index);
                0
            }
            TaskKind::Connect => {
                self.take(index, handler, false);
                if !self.connect_valid(index, think) {
                    return DONE;
                }
                self.run(index);
                0
            }
            TaskKind::Place => self.place_accept(index, think),
            TaskKind::Orders => self.order_accept(index, think),
            TaskKind::Projects | TaskKind::Expand | TaskKind::Scout => {
                self.take(index, handler, false);
                self.run(index);
                DONE
            }
            _ => 0,
        }
    }

    /// Gives running job `index` its turn with its handler `handler` (`+0x10`).
    pub(super) fn advance(&mut self, handler: TaskKind, index: u16, think: &mut Think<'_>) -> u16 {
        match handler {
            TaskKind::Reserve => {
                self.rest(index);
                self.set(index, JobFlags::FINISHED);
                self.report(index, DONE, 0, think);
                DONE
            }
            TaskKind::Site => self.search_turn(index, think),
            TaskKind::Connect => self.connect_turn(index, think),
            TaskKind::Orders => self.order_turn(index, think),
            TaskKind::Projects => self.project_turn(index, think),
            TaskKind::Shortcut => self.shortcut_turn(index, think),
            TaskKind::Expand => self.expand_turn(index, think),
            TaskKind::Scout => self.scout_turn(index, think),
            _ => 0,
        }
    }

    /// Carries out the world action job `index` planned, through its handler (`0x6a074`).
    pub(super) fn commit(&mut self, index: u16, think: &mut Think<'_>) -> bool {
        match self.job(index).and_then(|job| job.handler) {
            Some(TaskKind::Site) => self.site_commit(index, think),
            Some(TaskKind::Connect) => self.connect_commit(index, think),
            Some(TaskKind::Place) => {
                let Some(site) = self.child_of(index, JobKind::Site) else {
                    return false;
                };
                if !self.commit(site, think) {
                    return false;
                }
                self.child_of(site, JobKind::Connect)
                    .is_some_and(|road| self.commit(road, think))
            }
            _ => false,
        }
    }

    /// Task 8 takes a build order (`0x72d16`) and makes its placement.
    fn order_accept(&mut self, index: u16, think: &Think<'_>) -> u16 {
        self.take(index, TaskKind::Orders, true);
        let Some(Payload::BuildOrder {
            kind, node, radius, ..
        }) = self.job(index).map(|job| job.payload)
        else {
            return DONE;
        };
        let placement = Placement {
            kind,
            node,
            radius,
            found: 0,
            reports: 0,
            extra: 0,
        };
        let job = Job::with(JobKind::Place, Payload::Place(placement), think.step);
        self.add(job, Some(index), Some(TaskKind::Orders));
        DONE
    }

    /// A build order hears its placement (`0x72e72`): it goes back on the running list and keeps
    /// whether the placement was done, failed for good or is worth another try.
    fn order_heard(&mut self, order: u16, result: u16, extra: u32) {
        self.run(order);
        let outcome = if result == DONE {
            PLACED
        } else if extra & 1 == 1 {
            RETRY
        } else {
            FAILED
        };
        if let Some(Job {
            payload: Payload::BuildOrder { result, .. },
            ..
        }) = self.job_mut(order)
        {
            *result = outcome;
        }
    }

    /// A build order's turn (`0x72ddb`): a found placement is committed. One that failed for
    /// good ends the order; any other failure tries again with a new placement, five in all.
    fn order_turn(&mut self, order: u16, think: &mut Think<'_>) -> u16 {
        self.rest(order);
        let Some(Payload::BuildOrder { result, .. }) = self.job(order).map(|job| job.payload) else {
            return 0;
        };
        let committed = result & PLACED != 0
            && self
                .child_of(order, JobKind::Place)
                .is_some_and(|placement| self.commit(placement, think));
        if !committed && result & FAILED == 0 {
            let attempts = match self.job_mut(order) {
                Some(Job {
                    payload: Payload::BuildOrder { attempts, .. },
                    ..
                }) => {
                    *attempts += 1;
                    *attempts
                }
                _ => ATTEMPTS,
            };
            if attempts != ATTEMPTS {
                self.order_accept(order, think);
                return DONE;
            }
        }
        self.end(order);
        let answer = if committed { DONE } else { 0 };
        self.report(order, answer, 0, think);
        answer
    }

    /// Task 4 takes a placement (`0x6bb30`): a site search, then a reservation, under it.
    fn place_accept(&mut self, index: u16, think: &Think<'_>) -> u16 {
        self.take(index, TaskKind::Place, true);
        let Some(Payload::Place(placement)) = self.job(index).map(|job| job.payload) else {
            return DONE;
        };
        let search = SiteSearch {
            kind: placement.kind,
            centre: placement.node,
            passed: false,
            at: placement.node,
            ring: 0,
            side: 0,
            step: 0,
            radius: placement.radius,
            skip: false,
        };
        let site = Job::with(JobKind::Site, Payload::Site(search), think.step);
        self.add(site, Some(index), Some(TaskKind::Place));
        let reserve = Job::with(
            JobKind::Reserve,
            Payload::Reserve { node: placement.node },
            think.step,
        );
        self.add(reserve, Some(index), Some(TaskKind::Place));
        DONE
    }

    /// A placement hears its site search or its reservation (`0x6bc7d`). Once both have reported
    /// it is finished and reports the site when both succeeded.
    fn place_heard(&mut self, place: u16, child: u16, result: u16, extra: u32, think: &mut Think<'_>) {
        let kind = self.job(child).map(|job| job.kind);
        let Some(Job {
            payload: Payload::Place(placement),
            ..
        }) = self.job_mut(place)
        else {
            return;
        };
        match kind {
            Some(JobKind::Reserve) => {
                if result != 0 {
                    placement.found |= RESERVED;
                }
                placement.reports += 1;
            }
            Some(JobKind::Site) => {
                if result != 0 {
                    placement.found |= SITE_FOUND;
                }
                placement.reports += 1;
                placement.extra = extra;
            }
            _ => {}
        }
        let placement = *placement;
        if placement.reports != 2 {
            return;
        }
        self.set(place, JobFlags::FINISHED);
        if placement.found == SITE_FOUND | RESERVED {
            self.report(place, DONE, placement.extra, think);
        } else {
            self.report(place, 0, u32::from(placement.found), think);
        }
    }

    /// The site search's turn (`0x6a93e`): up to the profile's number of nodes of the spiral. A
    /// node that passes and whose flag node the player owns gets a road job and the search rests;
    /// a spiral run out reports failure. With the bug fixes on it also leaves the running list,
    /// so it never tests the node where its spiral ended.
    fn search_turn(&mut self, index: u16, think: &mut Think<'_>) -> u16 {
        for _ in 0..think.profile.placing.tests {
            let Some(Payload::Site(search)) = self.job(index).map(|job| job.payload) else {
                return DONE;
            };
            if search.skip {
                self.update_search(index, |search| search.skip = false);
            } else if think.game.nodes.get(search.at).owner == Some(think.player)
                && Self::site_fits(&search, think)
            {
                self.update_search(index, |search| search.passed = true);
                let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
                let flag = search.at.neighbour(Direction::SouthEast, width, height);
                if think.game.nodes.get(flag).owner == Some(think.player) {
                    self.rest(index);
                    let road = Job::with(
                        JobKind::Connect,
                        Payload::Connect {
                            flag,
                            mode: 0,
                            kind: search.kind,
                        },
                        think.step,
                    );
                    self.add(road, Some(index), Some(TaskKind::Site));
                    self.set(index, JobFlags::ACCEPTED);
                    return DONE;
                }
            }
            if !self.spiral_on(index, think) {
                self.set(index, JobFlags::FINISHED);
                self.report(index, 0, 0, think);
                if think.game.world.bug_fixes.on() {
                    self.rest(index);
                }
                return DONE;
            }
        }
        DONE
    }

    fn update_search(&mut self, index: u16, change: impl FnOnce(&mut SiteSearch)) {
        if let Some(Job {
            payload: Payload::Site(search),
            ..
        }) = self.job_mut(index)
        {
            change(search);
        }
    }

    /// Moves the site search's spiral on a node (`0x6a6ef`): out of the centre, then round each
    /// ring from its north-west corner; false when ring 12 is reached. With the bug fixes off, a
    /// search that has run out and is turned again counts its side on past 5, and the cursor step (`0x22498`) ignores a
    /// direction above 5, so it stays on the node where ring 12 ended.
    fn spiral_on(&mut self, index: u16, think: &Think<'_>) -> bool {
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        let mut more = true;
        self.update_search(index, |search| {
            if search.ring == 0 {
                search.ring = 1;
                search.at = search.at.neighbour(Direction::NorthWest, width, height);
                search.side = 0;
                search.step = 0;
                return;
            }
            if let Some(&way) = Direction::ALL.get(usize::from(search.side)) {
                search.at = search.at.neighbour(way, width, height);
            }
            search.step += 1;
            if search.step != search.ring {
                return;
            }
            search.step = 0;
            search.side += 1;
            if search.side != 6 {
                return;
            }
            search.ring += 1;
            if search.ring == LAST_RING {
                more = false;
                return;
            }
            search.at = search.at.neighbour(Direction::NorthWest, width, height);
            search.side = 0;
        });
        more
    }

    /// The site rules (`0x6a443`), in order: the player's node; for a military type the spacing
    /// rule, the computer barriers and, for a barracks or guardhouse, no military building near;
    /// the size test, with enough ore for a mine; no building near a farm; none of a kind's own
    /// near it; and for every type but military ones, the catapult and the storehouse, no farm
    /// near.
    fn site_fits(search: &SiteSearch, think: &Think<'_>) -> bool {
        let (node, kind, player) = (search.at, search.kind, think.player);
        let types = think.game.world.building_types();
        let Some(row) = types.get(kind) else {
            return false;
        };
        let military = row.category == Category::Military;
        if military {
            if !think.game.world.military_room(&think.game.nodes, player, node) || think.game.in_barrier(node)
            {
                return false;
            }
            if matches!(kind, BuildingKind::BARRACKS | BuildingKind::GUARDHOUSE)
                && think.military_near(node, think.profile.placing.military_free)
            {
                return false;
            }
        }
        let offered = think.game.nodes.get(node).sites.size;
        let fits = crate::construction::fits(row.size, offered)
            && (row.size != Size::Mine
                || think.ore_near(node, mineral_of(kind)) >= think.profile.placing.mine_ore);
        if !fits {
            return false;
        }
        if kind == BuildingKind::FARM && think.count_near(player, None, node, FARM_REACH) != 0 {
            return false;
        }
        let reach = own_kind_reach(kind);
        if reach != 0 && think.any_near(player, kind, node, reach) {
            return false;
        }
        if military || kind == BuildingKind::CATAPULT || kind == BuildingKind::STOREHOUSE {
            return true;
        }
        !think.any_near(player, BuildingKind::FARM, node, FARM_REACH)
    }

    /// A site search hears its road job (`0x6aac4`): success finishes it and reports the site;
    /// failure passes over the node next turn and sends the road job away.
    fn site_heard(&mut self, site: u16, child: u16, result: u16, think: &mut Think<'_>) {
        if result != 0 {
            self.set(site, JobFlags::FINISHED);
            let Some(Payload::Site(search)) = self.job(site).map(|job| job.payload) else {
                return;
            };
            let packed = u32::from(search.at.y) << 16 | u32::from(search.at.x);
            self.report(site, result, packed, think);
        } else {
            self.update_search(site, |search| search.skip = true);
            self.run(site);
            self.end(child);
        }
    }

    /// The site search's commit (`0x6ab18`): the player's node still, the spacing rule again for a
    /// military type, then the site goes down (`0x26221`) where the node still offers its size.
    fn site_commit(&mut self, index: u16, think: &mut Think<'_>) -> bool {
        let Some(Payload::Site(search)) = self.job(index).map(|job| job.payload) else {
            return false;
        };
        if think.game.nodes.get(search.at).owner != Some(think.player) {
            return false;
        }
        let military = think
            .game
            .world
            .building_types()
            .get(search.kind)
            .is_some_and(|row| row.category == Category::Military);
        if military
            && !think
                .game
                .world
                .military_room(&think.game.nodes, think.player, search.at)
        {
            return false;
        }
        let offered = think.game.nodes.get(search.at).sites.size;
        if !think
            .game
            .world
            .building_types()
            .get(search.kind)
            .is_some_and(|row| crate::construction::fits(row.size, offered))
        {
            return false;
        }
        think
            .game
            .world
            .place_site(&mut think.game.nodes, search.at, search.kind, think.player)
            .is_some()
    }

    /// Whether the player still owns a road job's flag node (`0x6b4bd`); a job that lost it reports
    /// 0 and is sent away.
    fn connect_valid(&mut self, index: u16, think: &mut Think<'_>) -> bool {
        let Some(Payload::Connect { flag, .. }) = self.job(index).map(|job| job.payload) else {
            return false;
        };
        if think.game.nodes.get(flag).owner == Some(think.player) {
            return true;
        }
        self.report(index, 0, 0, think);
        self.end(index);
        false
    }

    /// The road job's turn (`0x6b3fc`): it finishes and asks whether a road can be made. With a
    /// parent it reports that and waits for the commit; with none it commits at once and goes.
    fn connect_turn(&mut self, index: u16, think: &mut Think<'_>) -> u16 {
        self.rest(index);
        if !self.connect_valid(index, think) {
            return DONE;
        }
        self.set(index, JobFlags::FINISHED);
        let answer = if self.connect_feasible(index, think) {
            DONE
        } else {
            0
        };
        if self.job(index).is_some_and(|job| job.parent.is_some()) {
            self.report(index, answer, 0, think);
        } else {
            if answer != 0 {
                self.connect_commit(index, think);
            }
            self.end(index);
        }
        answer
    }

    /// Whether the road job's flag node can be joined up (`0x6acb6`): the player's node with a flag
    /// on it already, or one the road search finds.
    fn connect_feasible(&self, index: u16, think: &Think<'_>) -> bool {
        let Some(Payload::Connect { flag, .. }) = self.job(index).map(|job| job.payload) else {
            return false;
        };
        if think.game.nodes.get(flag).owner != Some(think.player) {
            return false;
        }
        think.game.world.flag_at(flag).is_some() || self.road_search(index, think).is_some()
    }

    /// The road search (`0x6b201`): for a building's flag the building's node, and a large one's
    /// footprint, are barred, then [`Think::flood`] runs from the flag.
    fn road_search(&self, index: u16, think: &Think<'_>) -> Option<Route> {
        let Some(Payload::Connect { flag, mode, kind }) = self.job(index).map(|job| job.payload) else {
            return None;
        };
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        let mut marks = vec![0u8; usize::from(width) * usize::from(height)];
        let mark = |marks: &mut Vec<u8>, node: Node, value: u8| {
            marks[usize::from(node.y) * usize::from(width) + usize::from(node.x)] = value;
        };
        if mode == 0 {
            let building = flag.neighbour(Direction::NorthWest, width, height);
            mark(&mut marks, building, BARRED);
            let large = think
                .game
                .world
                .building_types()
                .get(kind)
                .is_some_and(|row| row.size == Size::Large);
            if large {
                for way in [Direction::West, Direction::NorthWest, Direction::NorthEast] {
                    mark(&mut marks, building.neighbour(way, width, height), BARRED);
                }
            }
        }
        think.flood(
            flag,
            u8::try_from(think.profile.placing.route_rings).unwrap_or(u8::MAX),
            marks,
        )
    }

    /// The road job's commit (`0x6b482`, `0x6acff`): the search runs again, and a building's flag
    /// that already has a road is done. Otherwise the walk goes from the best flag back down the
    /// flood to the job's flag, closing a road at each flag it meets and putting a flag down once
    /// enough nodes are pending and one may stand. Anything built before a failure stays.
    fn connect_commit(&mut self, index: u16, think: &mut Think<'_>) -> bool {
        let Some(Payload::Connect {
            flag: target, mode, ..
        }) = self.job(index).map(|job| job.payload)
        else {
            return false;
        };
        if think.game.nodes.get(target).owner != Some(think.player) {
            return false;
        }
        let Some(route) = self.road_search(index, think) else {
            return false;
        };
        let end = think.game.world.flag_at(target);
        if mode == 0
            && end
                .and_then(|flag| think.game.world.flag(flag))
                .is_some_and(|flag| flag.links.iter().any(Option::is_some))
        {
            return true;
        }
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        let Some(mut at) = think.game.world.flag(route.best).map(|flag| flag.node) else {
            return false;
        };
        let mut current = route.best;
        let mut pending: Vec<Node> = Vec::new();
        let mut wanted = route.mark(at, width).wrapping_add(1);
        let spacing = usize::from(think.profile.placing.flag_spacing.saturating_sub(2));
        for _ in 0..route.ring.saturating_sub(1) {
            let Some(next) = AROUND
                .iter()
                .map(|&way| at.neighbour(way, width, height))
                .find(|&node| route.mark(node, width) == wanted)
            else {
                return false;
            };
            at = next;
            if let Some(flag) = think.game.world.flag_at(at) {
                if !think.road(current, flag, &mut pending) {
                    return false;
                }
                current = flag;
            } else if spacing >= pending.len()
                || think.game.nodes.get(at).sites.flags.contains(SiteFlags::NO_FLAG)
            {
                pending.push(at);
            } else if let Some(flag) =
                think
                    .game
                    .world
                    .place_flag(&mut think.game.nodes, at, FlagKind::Normal, true)
            {
                if !think.road(current, flag, &mut pending) {
                    return false;
                }
                current = flag;
            } else {
                pending.push(at);
            }
            wanted = wanted.wrapping_add(1);
        }
        end.is_some_and(|flag| think.road(current, flag, &mut pending))
    }
}

/// What the road search found: the best flag and the ring it was found on, with the flood's marks.
pub(super) struct Route {
    best: u16,
    ring: u16,
    marks: Vec<u8>,
}

impl Route {
    fn mark(&self, node: Node, width: u16) -> u8 {
        self.marks[usize::from(node.y) * usize::from(width) + usize::from(node.x)]
    }
}

impl Think<'_> {
    /// Whether a military building or site of any owner stands on rings 1 to `reach` round
    /// `node` (`0x68a0c`).
    fn military_near(&self, node: Node, reach: u16) -> bool {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        (1..=reach)
            .flat_map(|ring| node.ring(ring, width, height))
            .any(|at| {
                let MapObject::Building(record) = self.game.world.object(at) else {
                    return false;
                };
                self.game.world.building(record).is_some_and(|building| {
                    self.game
                        .world
                        .building_types()
                        .get(building.kind)
                        .is_some_and(|row| row.category == Category::Military)
                })
            })
    }

    /// The ore of mineral bits `mineral` on `node` and rings 1 and 2 (`0x6895b`): each node's
    /// amount when it holds that mineral (`0x68918`).
    fn ore_near(&self, node: Node, mineral: u8) -> u16 {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        node.rings(2, width, height)
            .map(|at| {
                let resources = self.game.nodes.get(at).resources;
                if matches!(resources, crate::nodes::Resources::Mineral(..))
                    && resources.mineral_bits() == mineral
                {
                    u16::from(resources.amount())
                } else {
                    0
                }
            })
            .sum()
    }

    /// The road cost from `flag` to the nearest finished storehouse, headquarters or harbour,
    /// at most `limit`; `None` when none is in reach (`0x6af61`, predicate `0x6aefc`).
    pub(super) fn storehouse_cost(&self, flag: u16, limit: u16) -> Option<u16> {
        search(&self.game.world, flag, limit, SearchOptions::empty(), |reached| {
            self.game
                .world
                .building_at_flag(reached.flag)
                .is_some_and(|record| {
                    self.game.world.building(record).is_some_and(|building| {
                        building.state.contains(BuildingState::FINISHED)
                            && self
                                .game
                                .world
                                .building_types()
                                .get(building.kind)
                                .is_some_and(|row| row.category == Category::Storehouse)
                    })
                })
        })
        .map(|reached| reached.cost)
    }

    /// The flood of the road search (`0x6af91`) from `start` over `depth − 1` rings, on `marks`.
    /// Each ring is marked one lower than the last, from `depth` at the start. A flag met is kept
    /// and scored 1000 less twice its ring and its cost to a storehouse; the best score wins,
    /// the first on a tie. A plain node is kept when the edge one direction round from the one
    /// stepped along is walkable, as `S2.EXE` tests it, the node takes roads, can be walked onto
    /// and has the start's owner; any other is barred. A ring of 1,000 nodes fails the search.
    fn flood(&self, start: Node, depth: u8, mut marks: Vec<u8>) -> Option<Route> {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let owner = self.game.nodes.get(start).owner;
        if owner != Some(self.player) {
            return None;
        }
        let at = |node: Node| usize::from(node.y) * usize::from(width) + usize::from(node.x);
        marks[at(start)] = depth;
        let mut frontier = vec![start];
        let mut best: Option<(u16, u16)> = None;
        let mut best_score = 0i32;
        let mut value = depth;
        loop {
            value = value.wrapping_sub(1);
            if value == 0 {
                break;
            }
            let ring = u16::from(depth - value);
            let mut next = Vec::new();
            for &from in &frontier {
                for (d, &way) in AROUND.iter().enumerate() {
                    let node = from.neighbour(way, width, height);
                    if marks[at(node)] != 0 {
                        continue;
                    }
                    let keep = if let Some(flag) = self.game.world.flag_at(node) {
                        if let Some(cost) = self.storehouse_cost(flag, COST_LIMIT) {
                            let score = SCORE - 2 * i32::from(ring) - i32::from(cost);
                            if score > best_score {
                                best_score = score;
                                best = Some((flag, ring));
                            }
                        }
                        true
                    } else {
                        let tested = Direction::ALL[(d + 4) % 6];
                        let data = self.game.nodes.get(node);
                        self.game.nodes.edge_walkable(from, tested)
                            && !data.sites.flags.contains(SiteFlags::NO_ROAD)
                            && data.sites.passable()
                            && data.owner == owner
                    };
                    if keep {
                        marks[at(node)] = value;
                        next.push(node);
                        if next.len() == FRONTIER {
                            return None;
                        }
                    } else {
                        marks[at(node)] = BARRED;
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        let (best, ring) = best?;
        Some(Route { best, ring, marks })
    }

    /// A road from flag `from` along `pending` to flag `to` unless the two are joined already
    /// (`0x68302`, `0x25b58`); `pending` is emptied either way.
    pub(super) fn road(&mut self, from: u16, to: u16, pending: &mut Vec<Node>) -> bool {
        let path = std::mem::take(pending);
        let joined = self
            .game
            .world
            .flag(from)
            .is_some_and(|flag| flag.links.iter().flatten().any(|link| link.far == Some(to)));
        joined
            || self
                .game
                .world
                .create_road(&mut self.game.nodes, RoadKind::Normal, from, Some(to), path)
                .is_ok()
    }
}

#[cfg(test)]
pub(super) mod tests;
