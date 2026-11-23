//! Gatherers: the woodcutter, forester, stonemason, fisher and farmer walking out to a node,
//! working there and bringing the product home.
//!
//! The hunter's part of the trip is in `hunt.rs`.

use crate::figures::{Figure, FigureKind, Gatherer, Trip};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::ground::Property;
use crate::homeward::{Crossing, HOME_SPEED};
use crate::job::Job;
use crate::nodes::{Mineral, Resources};
use crate::settlers::DOOR_SPEED;
use crate::sites::refresh_sites;
use crate::ware::WareKind;
use crate::world::{BuildingState, Field, FieldStage, MapObject, Remnant, Tree, World};

/// The work animations in `ROM_ANIM.DAT` (`0x339b8`).
const CHOPPING: u16 = 89;
const PLANTING: u16 = 211;
const QUARRYING: u16 = 178;
const HARVESTING: u16 = 895;
const SOWING: u16 = 743;

/// The fisher's animation for each way it faces, east first (the table at `0x33b18`).
const FISHING: [u16; 6] = [2701, 2762, 2823, 2518, 2579, 2640];

/// Byte 0 of each tree type's entry in the tree table at `0x1fb694`: whether a woodcutter fells
/// it. Types 9 to 15 read past the nine trees into other data.
const FELLABLE: [bool; 16] = [
    true, true, true, true, true, false, true, true, true, true, true, true, true, false, true, true,
];

/// The tree types a forester plants, in the order it rolls for them (byte 1 of the table).
const PLANTED: [u8; 6] = [0, 1, 2, 6, 7, 8];

/// A forester's roll for each tree type: a draw from 0 to this, plus 1 (`0x380a0`).
const PLANT_ROLL: u16 = 199;

/// The highest of the field types a farmer sows, one drawn per field (`0x384bf`).
const LAST_FIELD_TYPE: u16 = 1;

/// The felling stage that leaves the stump.
const STUMP_STAGE: u8 = 4;

/// A search kind's row of the table at `0x1fa5ca`: what it looks for, whether the centre is
/// tested, how far, how many hits it keeps and whether a hit must share the building's area.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Search {
    pub(crate) find: Find,
    pub(crate) centre: bool,
    pub(crate) radius: u16,
    pub(crate) hits: usize,
    pub(crate) same_area: bool,
}

/// What a search looks for.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Find {
    /// A grown tree a woodcutter fells (`0x2e78c`).
    Tree,
    /// A node a tree can be planted on (`0x2ed31`).
    Plantable,
    /// Granite (`0x2ee66`).
    Granite,
    /// A ripe field (`0x2ef78`).
    Ripe,
    /// Fish (`0x2f0a2`).
    Fish,
    /// An animal a hunter can take (`0x2f0f6`).
    Prey,
    /// Water for a well (`0x2f4ac`).
    Water,
    /// Ore or granite of a mineral, 0 to 3, for a mine (`0x2f504`–`0x2f60e`).
    Mineral(Mineral),
    /// A node a geologist can examine (`0x2edd4`).
    Surveyable,
    /// A node nothing on which shows to the player yet (`0x2edea`).
    Unseen(crate::world::Player),
    /// A finished, occupied military building of any owner (`0x2e880`).
    Garrison,
    /// A node on the shore a ship can be built on (`0x2e7bf`).
    ShipSite,
    /// A free node of a sea for a ship to anchor on (`0x30c5f`).
    Anchorage(u8),
}

/// Search kinds 5, 6 and 7.
const WOODCUTTER_SEARCH: Search = Search {
    centre: false,
    find: Find::Tree,
    radius: 6,
    hits: 5,
    same_area: true,
};
const FORESTER_SEARCH: Search = Search {
    centre: false,
    find: Find::Plantable,
    radius: 6,
    hits: 8,
    same_area: true,
};
const QUARRY_SEARCH: Search = Search {
    centre: false,
    find: Find::Granite,
    radius: 8,
    hits: 2,
    same_area: true,
};

/// Search kind 0: water with fish, in any area.
const FISHERY_SEARCH: Search = Search {
    centre: false,
    find: Find::Fish,
    radius: 8,
    hits: 3,
    same_area: false,
};

/// Search kinds 1 and 2: the farmer's harvest and sowing.
const HARVEST_SEARCH: Search = Search {
    centre: false,
    find: Find::Ripe,
    radius: 2,
    hits: 3,
    same_area: true,
};
const SOW_SEARCH: Search = Search {
    centre: false,
    find: Find::Plantable,
    radius: 2,
    hits: 4,
    same_area: true,
};

/// The ring search walks each ring in these directions, `r` steps each.
const RING_WALK: [Direction; 6] = [
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
    Direction::NorthEast,
];

impl Game {
    /// Starts a woodcutter's trip (`0x2ec98`): with room at the flag, a grown tree nearby.
    pub(crate) fn start_woodcutter(&mut self, building: u16) -> bool {
        if self.flag_full(building) {
            return false;
        }
        let hits = self.search_round(building, WOODCUTTER_SEARCH);
        self.go_gathering(building, Job::WOODCUTTER, &hits)
    }

    /// Starts a forester's trip (`0x2ed47`): a node to plant on nearby.
    pub(crate) fn start_forester(&mut self, building: u16) -> bool {
        let hits = self.search_round(building, FORESTER_SEARCH);
        self.go_gathering(building, Job::FORESTER, &hits)
    }

    /// Starts a stonemason's trip (`0x2ee8a`), unless the quarry has given up: with room at the
    /// flag, granite nearby. A quarry that finds none rates its productivity 0 and gives up for
    /// good.
    pub(crate) fn start_quarry(&mut self, building: u16) -> bool {
        self.start_until_exhausted(building, QUARRY_SEARCH, Job::STONEMASON)
    }

    /// Starts a fisher's trip (`0x2f196`), as the quarry does with fish.
    pub(crate) fn start_fishery(&mut self, building: u16) -> bool {
        self.start_until_exhausted(building, FISHERY_SEARCH, Job::FISHER)
    }

    /// Starts worker `job`'s trip to a hit of `search`, unless the building has given up: with
    /// room at the flag and a hit nearby. A building that finds nothing rates its productivity 0
    /// and gives up for good.
    fn start_until_exhausted(&mut self, building: u16, search: Search, job: Job) -> bool {
        if self
            .world
            .building(building)
            .is_none_or(|quarry| quarry.exhausted)
            || self.flag_full(building)
        {
            return false;
        }
        let hits = self.search_round(building, search);
        if hits.is_empty() {
            self.give_up(building);
            return false;
        }
        self.go_gathering(building, job, &hits)
    }

    /// Starts a hunter's trip (`0x2f26b`): with room at the flag, game within 21 steps.
    pub(crate) fn start_hunter(&mut self, building: u16) -> bool {
        if self.flag_full(building) {
            return false;
        }
        let hits = self.search_round(building, crate::hunt::PREY_FAR);
        self.go_gathering(building, Job::HUNTER, &hits)
    }

    /// Starts a farmer's trip (`0x2ef97`): with room at the flag, a ripe field nearby to
    /// harvest, or failing that a node to sow. A harvest with no path to its field falls back
    /// to sowing.
    pub(crate) fn start_farm(&mut self, building: u16) -> bool {
        if self.flag_full(building) {
            return false;
        }
        let hits = self.search_round(building, HARVEST_SEARCH);
        if self.go_gathering(building, Job::FARMER, &hits) {
            return true;
        }
        let hits = self.search_round(building, SOW_SEARCH);
        self.go_gathering(building, Job::SOWER, &hits)
    }

    /// `building` found nothing to work on: it rates its productivity 0 and never starts again
    /// (`+0x0b`).
    pub(crate) fn give_up(&mut self, building: u16) {
        if let Some(site) = self.world.building_mut(building) {
            site.tally = 0;
            site.productivity = 0;
            site.exhausted = true;
        }
    }

    /// Whether `building`'s flag holds all the wares it can (`0x2d33a`).
    fn flag_full(&self, building: u16) -> bool {
        self.world
            .building(building)
            .and_then(|building| self.world.flag(building.flag))
            .is_some_and(|flag| flag.waiting >= crate::world::FLAG_CAPACITY)
    }

    /// The hits of `search` round `building`'s node.
    pub(crate) fn search_round(&self, building: u16, search: Search) -> Vec<Node> {
        self.world
            .building(building)
            .map(|site| self.search(site.node, search))
            .unwrap_or_default()
    }

    /// Picks one of `hits`, a draw even when there is one, and sends worker `job` out to it
    /// (`0x2f460`, `0x334c4`). Returns whether the worker set off; with no hits nothing is drawn.
    fn go_gathering(&mut self, building: u16, job: Job, hits: &[Node]) -> bool {
        let Some(target) = self.pick(hits) else {
            return false;
        };
        self.send_out(building, job, target)
    }

    /// One of `hits` by a draw, even when there is one (`0x2f460`); `None`, with nothing drawn,
    /// when there are none.
    pub(crate) fn pick(&mut self, hits: &[Node]) -> Option<Node> {
        let last = u16::try_from(hits.len().checked_sub(1)?).unwrap_or(0);
        let pick = self.rng.between(0, last);
        hits.get(usize::from(pick)).copied()
    }

    /// The ring search (`0x2f304`): the centre first when the kind tests it, then rings 1 to the
    /// radius round `centre`, each from its north-west corner round to the east and back,
    /// testing each node after each step. Hits are kept in that order. The list is checked for
    /// being full only after a ring hit, so a centre hit and a ring hit make two even when the
    /// kind keeps one.
    pub(crate) fn search(&self, centre: Node, search: Search) -> Vec<Node> {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let area = self.nodes.get(centre).area;
        let mut hits = Vec::new();
        if search.centre && self.finds(search.find, centre) {
            hits.push(centre);
        }
        let mut at = centre;
        for radius in 1..=search.radius {
            at = at.neighbour(Direction::NorthWest, width, height);
            for direction in RING_WALK {
                for _ in 0..radius {
                    at = at.neighbour(direction, width, height);
                    if search.same_area && self.nodes.get(at).area != area {
                        continue;
                    }
                    if self.finds(search.find, at) {
                        hits.push(at);
                        if hits.len() >= search.hits {
                            return hits;
                        }
                    }
                }
            }
        }
        hits
    }

    /// Whether `node` holds what `find` looks for.
    fn finds(&self, find: Find, node: Node) -> bool {
        match find {
            Find::Tree => self.world.fellable(node),
            Find::Plantable => self.plantable(node),
            Find::Granite => matches!(self.world.object(node), MapObject::Granite(_)),
            Find::Ripe => self.world.field_stage(node) == Some(FieldStage::Ripe),
            Find::Fish => {
                self.nodes.property(node).contains(Property::ALL_WATER)
                    && self.nodes.get(node).resources.fishable()
            }
            Find::Prey => self.prey_at(node).is_some(),
            Find::Water => matches!(self.nodes.get(node).resources, Resources::Water(amount) if amount > 0),
            Find::Surveyable => self.surveyable(node),
            Find::Unseen(player) => !self.nodes.visible(node, player),
            Find::Garrison => self.world.garrison_at(node).is_some(),
            Find::ShipSite => self.ship_site_fits(node),
            Find::Anchorage(sea) => self.anchorage(node, sea),
            Find::Mineral(mineral) => matches!(
                self.nodes.get(node).resources,
                Resources::Mineral(found, amount) if found == mineral && amount > 0
            ),
        }
    }

    /// Whether a tree can be planted on `node`: slot 8 bit 3 is clear (`0x1970f`).
    fn plantable(&self, node: Node) -> bool {
        !self
            .nodes
            .get(node)
            .sites
            .flags
            .contains(crate::nodes::SiteFlags::NO_PLANTING)
    }

    /// Sends worker `job` out of `building` to `target` (`0x334c4`): a cross-country path from
    /// the flag, one draw per step, and a new figure walking out of the door, held until a
    /// step that is a multiple of 7. Returns false, with nothing changed, when there is no path.
    pub(crate) fn send_out(&mut self, building: u16, job: Job, target: Node) -> bool {
        let Some(site) = self.world.building(building).copied() else {
            return false;
        };
        let Some(flag) = self.world.flag(site.flag).map(|flag| flag.node) else {
            return false;
        };
        let Some(path) = self.find_path(flag, target, Crossing::Walkable, job != Job::FISHER) else {
            return false;
        };
        let mut figure = Figure::fresh(FigureKind::Gatherer(Gatherer {
            owner: site.owner,
            job,
            building,
            trip: Trip::Leaving { path },
            prey: None,
            searches: 0,
        }));
        figure.start_walk(Direction::SouthEast, DOOR_SPEED);
        self.world.add_figure(flag, figure);
        if let Some(site) = self.world.building_mut(building) {
            site.state |= BuildingState::DOOR | BuildingState::WORKING;
            site.state -= BuildingState::PRESENT;
        }
        true
    }

    /// One step of gatherer `record` that is not walking.
    pub(crate) fn step_gatherer(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Gatherer(gatherer) = figure.kind else {
            return;
        };
        match gatherer.trip {
            Trip::Leaving { path } => {
                if let Some(FigureKind::Gatherer(gatherer)) = self.figure_kind_mut(record) {
                    if gatherer.job == Job::HUNTER {
                        gatherer.prey = None;
                        gatherer.searches = crate::hunt::SEARCHES;
                    }
                    gatherer.trip = Trip::Walking { path };
                }
            }
            Trip::Walking { path } => self.walk_trip(record, gatherer, path, figure.node()),
            Trip::Working { animation } => {
                if animation.is_some_and(|animation| self.run_animation_at(animation, figure.node())) {
                    return;
                }
                self.end_work(record, gatherer, figure.node());
            }
            Trip::Entering => self.gatherer_home(record, gatherer),
            Trip::Waiting => {}
        }
    }

    fn set_trip(&mut self, record: u16, trip: Trip) {
        if let Some(FigureKind::Gatherer(gatherer)) = self.figure_kind_mut(record) {
            gatherer.trip = trip;
        }
    }

    /// The next step of gatherer `record`'s path (`0x3473c`), or its arrival at the end. A path
    /// used up short of its target, as a full record leaves one, is searched again from here into
    /// the same record and the gatherer starts the new route's first edge in the same step. With
    /// no route the path is freed and it heads back to its flag (`0x33848`).
    fn walk_trip(&mut self, record: u16, gatherer: Gatherer, path: u16, node: Node) {
        let Some(walked) = self.world.paths.get(path).copied() else {
            self.make_lost(record);
            return;
        };
        let target = walked.to;
        if node == target {
            self.world.paths.remove(path);
            self.arrive_on(record, gatherer, node);
            return;
        }
        let mut ahead = walked;
        let direction = if let Some(direction) = ahead.next() {
            direction
        } else {
            if !self.route_into(path, node, Crossing::Walkable) {
                self.world.paths.remove(path);
                match self.gatherer_flag(gatherer) {
                    Some((_, flag)) => self.head_back(record, node, flag),
                    None => self.make_lost(record),
                }
                return;
            }
            ahead = self.world.paths.get(path).copied().unwrap_or(walked);
            let Some(direction) = ahead.next() else {
                self.make_lost(record);
                return;
            };
            direction
        };
        let next = node.neighbour(direction, self.nodes.width(), self.nodes.height());
        if gatherer.job == Job::FISHER && self.nodes.property(next).contains(Property::ALL_WATER) {
            self.world.paths.remove(path);
            self.take_resource(target);
            self.fish(record, node, target);
            return;
        }
        if self.held_up(record, next, path).blocked() {
            return;
        }
        if let Some(stored) = self.world.paths.get_mut(path) {
            *stored = ahead;
        }
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, HOME_SPEED);
        }
    }

    /// Gatherer `record` is at the end of its path on `node` (`0x33b30`). At its flag it puts
    /// down what it carries and walks in. At its target, alone there and with the work still
    /// to do, it starts its animation; otherwise it heads back. A shipwright starts work on its
    /// site whatever is there.
    fn arrive_on(&mut self, record: u16, gatherer: Gatherer, node: Node) {
        let Some(flag) = self.gatherer_flag(gatherer) else {
            self.make_lost(record);
            return;
        };
        if node == flag.1 {
            self.bring_home(record, gatherer, flag);
            return;
        }
        if gatherer.job == Job::FISHER {
            self.fish(record, node, node);
            return;
        }
        if gatherer.job == Job::HUNTER {
            self.hunter_arrives(record, gatherer, node, flag.1);
            return;
        }
        if gatherer.job == Job::SHIPWRIGHT_OUT {
            let animation = self.start_animation_at(crate::shipyard::SHIPBUILDING, node);
            if let Some(FigureKind::Gatherer(gatherer)) = self.figure_kind_mut(record) {
                gatherer.job = Job::SHIPWRIGHT;
                gatherer.trip = Trip::Working { animation };
            }
            self.world.drop_ware(gatherer.owner, WareKind::BOARDS, 1);
            return;
        }
        let alone = self
            .world
            .figures_at(node)
            .filter(|(_, figure)| figure.is_settler())
            .count()
            == 1;
        let work = match gatherer.job {
            Job::WOODCUTTER if alone && self.world.fellable(node) => Some(CHOPPING),
            Job::FORESTER if alone && self.plantable(node) => Some(PLANTING),
            Job::STONEMASON => Some(QUARRYING),
            Job::FARMER if alone && self.world.field_stage(node) == Some(FieldStage::Ripe) => {
                Some(HARVESTING)
            }
            Job::SOWER if alone && self.plantable(node) => Some(SOWING),
            _ => None,
        };
        match work {
            Some(start) => {
                let animation = self.start_animation_at(start, node);
                self.set_trip(record, Trip::Working { animation });
            }
            None => self.head_back(record, node, flag.1),
        }
    }

    /// Fisher `record`, standing on `at`, fishes at `target` (`0x33b30`): whoever else is there
    /// and whatever the water holds, it starts the animation for the way it faces and takes one
    /// fish from `target`.
    fn fish(&mut self, record: u16, at: Node, target: Node) {
        let facing = self
            .world
            .figure(record)
            .map_or(Direction::East, |figure| figure.direction);
        let animation = self.start_animation_at(FISHING[facing.index()], at);
        self.set_trip(record, Trip::Working { animation });
        self.take_resource(target);
    }

    /// Gatherer `record`'s work on `node` is done (`0x37f78`): the woodcutter takes the wood, the
    /// stonemason a stone, the fisher a fish and the farmer the grain, the forester plants a tree and the farmer
    /// sows a field. Slot 8 is recomputed round the node
    /// and the gatherer heads back to its flag.
    fn end_work(&mut self, record: u16, gatherer: Gatherer, node: Node) {
        if gatherer.job == Job::HUNTER {
            let flag = self.gatherer_flag(gatherer).map(|(_, flag)| flag);
            self.hunter_done(record, gatherer, node, flag);
            return;
        }
        let carrying = match gatherer.job {
            Job::WOODCUTTER => Some(Job::CARRYING_WOOD),
            Job::FISHER => Some(Job::CARRYING_FISH),
            Job::STONEMASON => {
                self.world.take_granite(node);
                Some(Job::CARRYING_STONE)
            }
            Job::FORESTER => {
                self.plant(node);
                None
            }
            Job::FARMER => {
                self.world.harvest(node);
                Some(Job::CARRYING_GRAIN)
            }
            Job::SOWER => {
                self.sow(node);
                None
            }
            Job::SHIPWRIGHT => {
                self.add_ship_part(gatherer.building, gatherer.owner, node);
                None
            }
            _ => None,
        };
        if let Some(look) = carrying
            && let Some(FigureKind::Gatherer(gatherer)) = self.figure_kind_mut(record)
        {
            gatherer.job = look;
        }
        if let Some((ware, _)) = carrying.and_then(load_of) {
            self.world.count_ware(gatherer.owner, ware, 1);
        }
        refresh_sites(&mut self.nodes, &self.world, node, 1);
        match self.gatherer_flag(gatherer) {
            Some((_, flag)) => self.head_back(record, node, flag),
            None => self.make_lost(record),
        }
    }

    /// A forester plants on `node` (`0x380a0`, `0x3d6d4`): one roll per plantable tree type, the
    /// strictly highest wins, and if the node can still take a tree a sapling of that type goes
    /// in with three random phase bits.
    fn plant(&mut self, node: Node) {
        let mut best = (0, 0);
        for kind in PLANTED {
            let roll = self.rng.up_to(PLANT_ROLL) + 1;
            if roll > best.1 {
                best = (kind, roll);
            }
        }
        if !self.plantable(node) {
            return;
        }
        let phase = u8::try_from(self.rng.step() % Tree::PHASES).unwrap_or_default();
        self.world
            .set_object(node, MapObject::Tree(Tree::sapling(best.0, phase)));
    }

    /// A farmer sows on `node` if it can still take a field (`0x3849b`, `0x3d90c`): a stage 0
    /// field of a type drawn from 0 and 1.
    fn sow(&mut self, node: Node) {
        if !self.plantable(node) {
            return;
        }
        let kind = u8::try_from(self.rng.up_to(LAST_FIELD_TYPE)).unwrap_or_default();
        self.world.set_object(node, MapObject::Field(Field::sown(kind)));
    }

    /// Sets gatherer `record` off from `node` back to its flag on `flag` (`0x33894`), or loses
    /// it with no path.
    pub(crate) fn head_back(&mut self, record: u16, node: Node, flag: Node) {
        match self.find_path(node, flag, Crossing::Walkable, false) {
            Some(path) => self.set_trip(record, Trip::Walking { path }),
            None => self.make_lost(record),
        }
    }

    /// Gatherer `record` is back at its flag (`0x33b30`): a ware it carries is put out, or lost
    /// when the flag is full, which takes it off the owner's ware count (`0x33cfe`). Then it walks
    /// in through the door.
    fn bring_home(&mut self, record: u16, gatherer: Gatherer, (flag, node): (u16, Node)) {
        if let Some((ware, job)) = load_of(gatherer.job) {
            if self.put_out(flag, ware) {
                if let Some(FigureKind::Gatherer(gatherer)) = self.figure_kind_mut(record) {
                    gatherer.job = job;
                }
            } else {
                self.world.drop_ware(gatherer.owner, ware, 1);
            }
        }
        let door = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        self.set_trip(record, Trip::Entering);
        self.world.move_figure(record, door);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, DOOR_SPEED);
        }
    }

    /// Gatherer `record` has walked in (`0x36134`): its workplace has its worker back and the
    /// figure is gone. With the workplace gone it is lost.
    fn gatherer_home(&mut self, record: u16, gatherer: Gatherer) {
        let Some(site) = self.world.building_mut(gatherer.building) else {
            self.make_lost(record);
            return;
        };
        site.state -= BuildingState::DOOR;
        site.state |= BuildingState::PRESENT;
        self.world.remove_figure(record);
    }

    /// Gatherer's workplace flag record and node (`0x33958`).
    fn gatherer_flag(&self, gatherer: Gatherer) -> Option<(u16, Node)> {
        let flag = self.world.building(gatherer.building)?.flag;
        Some((flag, self.world.flag(flag)?.node))
    }
}

impl World {
    /// Whether `node` holds a grown, standing tree of a type a woodcutter fells (`0x3d814`).
    pub(crate) fn fellable(&self, node: Node) -> bool {
        self.object(node)
            .tree()
            .is_some_and(|tree| tree.grown() && FELLABLE[usize::from(tree.kind)])
    }

    /// A tree on `node` falls to stage `stage` (`0x3d784`, `ROM_ANIM` opcode 3): stages 0 to 3
    /// show it falling, stage 4 leaves the stump. Anything but a tree is left alone, and so is a
    /// stage past 4, which `ROM_ANIM` never passes.
    pub(crate) fn fell(&mut self, nodes: &mut crate::nodes::NodeMap, node: Node, stage: u16) {
        let Some(tree) = self.object(node).tree() else {
            return;
        };
        match u8::try_from(stage) {
            Ok(STUMP_STAGE) => {
                self.set_object(node, MapObject::Remnant(Remnant::STUMP));
                for _ in 0..2 {
                    refresh_sites(nodes, self, node, 1);
                }
            }
            Ok(stage) if stage < STUMP_STAGE => {
                let fallen = Tree {
                    stage,
                    felling: true,
                    ..tree
                };
                self.set_object(node, MapObject::Tree(fallen));
            }
            _ => {}
        }
    }

    /// Granite on `node` loses one of its size, and goes at the last (`0x3d8b0`).
    fn take_granite(&mut self, node: Node) {
        let MapObject::Granite(granite) = self.object(node) else {
            return;
        };
        let left = granite.take_one().map_or(MapObject::Nothing, MapObject::Granite);
        self.set_object(node, left);
    }

    /// The stage of a field on `node` (`0x3d97c`), or `None` with no field there.
    pub(crate) fn field_stage(&self, node: Node) -> Option<FieldStage> {
        match self.object(node) {
            MapObject::Field(field) => Some(field.stage),
            _ => None,
        }
    }

    /// A field on `node`, of any stage, is cut and leaves stubble that fades away
    /// (`0x3d928`). Anything but a field is left alone.
    fn harvest(&mut self, node: Node) {
        if let MapObject::Field(field) = self.object(node) {
            self.set_object(node, MapObject::Remnant(field.harvested()));
        }
    }
}

/// The ware a gatherer carrying a load holds, and the job it goes back to once the ware is out
/// (`0x33b30`).
fn load_of(job: Job) -> Option<(WareKind, Job)> {
    match job {
        Job::CARRYING_WOOD => Some((WareKind::WOOD, Job::WOODCUTTER)),
        Job::CARRYING_STONE => Some((WareKind::STONES, Job::STONEMASON)),
        Job::CARRYING_FISH => Some((WareKind::FISH, Job::FISHER)),
        Job::CARRYING_MEAT => Some((WareKind::MEAT, Job::HUNTER)),
        Job::CARRYING_GRAIN => Some((WareKind::GRAIN, Job::FARMER)),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests;
