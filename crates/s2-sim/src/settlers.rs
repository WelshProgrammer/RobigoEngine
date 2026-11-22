//! Settlers in the simulation step: a storehouse sending someone out for a person order, the
//! walk to its flag, the walk across the road network and the hand-over that makes a helper a
//! road's carrier.
//!
//! A settler with no way on becomes a lost settler ([`crate::lost`]).

use crate::construction::{BUILDER_MODE, PLANER_MODE};
use crate::figures::{Carrier, CarrierState, Figure, FigureKind, OnRoad, Transport, Walker};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::marks::RoadKind;
use crate::supply::EXPEDITION_TAG;
use crate::transport::{ROAD_MODE, SearchOptions, Wanted, next_flag, search};
use crate::workplace::WORKER_MODE;
use crate::world::{BuildingState, FlagKind, MapObject};

/// How far a storehouse search reaches, and a walker's route.
const SEARCH_LIMIT: u16 = 250;

/// Urgency of a person order once it has been tried.
const TRIED_URGENCY: u8 = 40;

/// What a busy storehouse adds to the urgency of an order it could not serve.
const BUSY_BONUS: u8 = 5;

/// What a failed search takes off the urgency.
const FAILED_PENALTY: u8 = 40;

/// Jobs whose orders are dropped when no storehouse can send them: geologist and scout.
const DROPPED_JOBS: [Job; 2] = [Job::GEOLOGIST, Job::SCOUT];

/// The fixed speed of a settler walking between a building and its flag (`0x322dc`).
pub(crate) const DOOR_SPEED: u8 = 14;

/// A walking figure looks for a held node ahead of it on steps that are a multiple of this
/// (`0x32d70`).
const QUEUE_PERIOD: u32 = 4;

/// The door is checked on steps that are a multiple of this, while the figure is this far from
/// the end of its walk (`0x32d48`).
const DOOR_PERIOD: u32 = 5;
const DOOR_DISTANCE: std::ops::RangeInclusive<u8> = 15..=239;

/// The mode of a walker going home to a storehouse (state 1): its order field holds the
/// storehouse's flag.
pub(crate) const HOME_MODE: u8 = 1;

/// Pick-ups (and walkers entering) that turn an ordinary road into a donkey road (`0x2905c`).
const DONKEY_TRAFFIC: u16 = 100;

impl Game {
    /// One simulation step of settler `record`: the walk step while it walks, otherwise what its
    /// kind does.
    pub(crate) fn step_settler(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record) else {
            return;
        };
        if figure.walking() {
            self.advance_settler(record);
            return;
        }
        if figure.leaving_door() {
            let node = figure.node();
            self.release_door(node);
        }
        let Some(figure) = self.world.figure(record) else {
            return;
        };
        match figure.kind {
            FigureKind::Animal { .. } | FigureKind::Ship { .. } => {}
            FigureKind::Leaving { .. } => self.reach_flag(record),
            FigureKind::Walker(_) => self.step_walker(record),
            FigureKind::Carrier(_) => self.step_carrier(record),
            FigureKind::Lost { .. } => self.step_lost(record),
            FigureKind::Homeward { .. } => self.step_homeward(record),
            FigureKind::Entering { .. } => self.step_entering(record),
            FigureKind::Settling { .. } => self.step_settling(record),
            FigureKind::Gatherer(_) => self.step_gatherer(record),
            FigureKind::Explorer(_) => self.step_explorer(record),
            FigureKind::Porter(_) => self.step_porter(record),
            FigureKind::Delivering(_) => self.step_delivery(record),
            FigureKind::Bringing { .. } => self.step_bringing(record),
            FigureKind::Builder(_) => self.step_builder(record),
            FigureKind::Planer(_) => self.step_planer(record),
            FigureKind::Soldier(_) => self.step_soldier(record),
        }
    }

    /// Walking figure `record` stops where it is when the node ahead of it is held
    /// (`0x32d70`, `0x3232c`), which the figure loop checks every [`QUEUE_PERIOD`] steps.
    /// Returns whether it stopped, in which case it neither walks on nor works the door.
    /// A figure with a soldier record is never stopped this way
    /// ([`Game::fighting_soldier`]). The node it came from takes the held bit too unless it
    /// already has it, so the figure behind it stops in turn and a queue forms;
    /// [`Game::release`] gives that bit back.
    fn queue_stop(&mut self, record: u16) -> bool {
        let Some(figure) = self.world.figure(record).copied() else {
            return false;
        };
        if !figure.queues()
            || !self.clock.step().is_multiple_of(QUEUE_PERIOD)
            || !self
                .nodes
                .get(figure.node())
                .flags
                .contains(crate::nodes::NodeFlags::HELD)
            || self.fighting_soldier(record)
        {
            return false;
        }
        let behind = figure.node().neighbour(
            figure.direction.opposite(),
            self.nodes.width(),
            self.nodes.height(),
        );
        let mut data = self.nodes.get(behind);
        let takes = !data.flags.contains(crate::nodes::NodeFlags::HELD);
        if takes {
            data.flags |= crate::nodes::NodeFlags::HELD;
            self.nodes.set(behind, data);
        }
        if let Some(figure) = self.world.figure_mut(record) {
            figure.held = true;
            figure.holds_behind |= takes;
        }
        true
    }

    /// The walk step (`0x32d48`). A settler walking out of or into a building works the door on
    /// every fifth step while it is between 15 and 239 short of the end: walking in (north-west)
    /// opens it, and for a storehouse frees it to send someone; any other way closes it.
    fn advance_settler(&mut self, record: u16) {
        if self.queue_stop(record) {
            return;
        }
        let Some(figure) = self.world.figure_mut(record) else {
            return;
        };
        figure.advance_walk();
        let (node, direction, remaining) = (figure.node(), figure.direction, figure.remaining);
        if !figure.in_door()
            || !self.clock.step().is_multiple_of(DOOR_PERIOD)
            || !DOOR_DISTANCE.contains(&remaining)
        {
            return;
        }
        let at = if direction == Direction::SouthEast {
            node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height())
        } else {
            node
        };
        let MapObject::Building(building) = self.world.object(at) else {
            return;
        };
        let Some(building) = self.world.building_mut(building) else {
            return;
        };
        if !building.state.contains(BuildingState::FINISHED) {
            return;
        }
        if direction == Direction::NorthWest {
            building.state |= BuildingState::DOOR;
            if building.is_storehouse() {
                building.state |= BuildingState::PRESENT;
            }
        } else {
            building.state -= BuildingState::DOOR;
        }
    }

    /// Serves person order `record` (`0x2b8d4`): an expedition's builder comes from its
    /// harbour's own stock if it can; otherwise the nearest finished storehouse that can supply
    /// the job sends someone. A search that finds none links the order to the first harbour it
    /// passed, frees a geologist's or scout's order that has no harbour to turn to, moves an
    /// order at a harbour's flag to the next harbour's, and a water road's order to the road's
    /// other end, so both shores get searched in turn, and drops the urgency to 0. A storehouse
    /// that is busy sending someone else adds 5.
    pub(crate) fn serve_person(&mut self, record: u16) {
        let Some(order) = self.world.orders.get_mut(record) else {
            return;
        };
        if order.tag == EXPEDITION_TAG && order.job() == Some(Job::BUILDER) && self.expedition_builder(record)
        {
            return;
        }
        let Some(order) = self.world.orders.get_mut(record) else {
            return;
        };
        order.urgency = TRIED_URGENCY;
        let (flag, mode) = (order.flag, order.mode);
        let Some(mut job) = order.job() else {
            return;
        };
        let options = if mode == ROAD_MODE {
            SearchOptions::empty()
        } else {
            SearchOptions::SKIP_START
        };
        let at_harbour = self.world.harbour_at_flag(flag).is_some();
        let mut passed = None;
        let found = search(&self.world, flag, SEARCH_LIMIT, options, |reached| {
            if passed.is_none() && !at_harbour {
                passed = self.world.harbour_at_flag(reached.flag);
            }
            let Some((_, store)) = self.storehouse_at_flag(reached.flag) else {
                return false;
            };
            match store.supplies(job) {
                Some(supplied) => {
                    job = supplied;
                    true
                }
                None => false,
            }
        });
        let Some(found) = found else {
            self.cross_water(record, passed, false);
            let linked = self
                .world
                .order(record)
                .is_some_and(|order| order.harbour.is_some());
            if DROPPED_JOBS.contains(&job) && !linked && !at_harbour {
                self.world.orders.remove(record);
                return;
            }
            if mode != ROAD_MODE {
                self.cross_water(record, None, true);
            }
            let other_end = (mode == ROAD_MODE && job == Job::WATER_ROAD)
                .then(|| self.ordered_road(flag, record))
                .flatten()
                .and_then(|road| self.world.road(road))
                .and_then(|road| {
                    if road.start == flag {
                        road.end
                    } else {
                        Some(road.start)
                    }
                });
            if let Some(order) = self.world.orders.get_mut(record) {
                if let Some(other_end) = other_end {
                    order.flag = other_end;
                }
                order.urgency = order.urgency.saturating_sub(FAILED_PENALTY);
            }
            return;
        };
        if let Some(order) = self.world.orders.get_mut(record) {
            order.wanted = Wanted::Person(job);
        }
        let Some((building, _)) = self.storehouse_at_flag(found.flag) else {
            return;
        };
        let (figure, busy) = self.send_person(record, building, found.flag);
        self.order_site_wares(record);
        let Some(order) = self.world.orders.get_mut(record) else {
            return;
        };
        if busy {
            order.urgency = order
                .urgency
                .saturating_add(BUSY_BONUS)
                .min(crate::transport::MOST_URGENT);
        }
        if let Some(figure) = figure {
            order.on_way = Some(figure);
            order.delivering = true;
            self.world.release_link(record);
        }
    }

    /// The finished storehouse whose flag is `flag`, with its stock.
    pub(crate) fn storehouse_at_flag(&self, flag: u16) -> Option<(u16, &crate::world::Storehouse)> {
        let node = self.world.flag(flag)?.node;
        let at = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        let MapObject::Building(building) = self.world.object(at) else {
            return None;
        };
        let record = self.world.building(building)?;
        if !record.is_storehouse() || !record.state.contains(BuildingState::FINISHED) {
            return None;
        }
        let (_, store) = self.world.storehouse_of(building)?;
        Some((building, store))
    }

    /// Storehouse `building` sends someone for order `order` (`0x2cf5f`, `0x36b00`). Returns the
    /// new figure, and whether the storehouse was busy: someone it sent has not reached the flag.
    fn send_person(&mut self, order: u16, building: u16, flag: u16) -> (Option<u16>, bool) {
        let Some(record) = self.world.building(building).copied() else {
            return (None, false);
        };
        if !record.state.contains(BuildingState::PRESENT) {
            return (None, true);
        }
        let Some((store, stock)) = self.world.storehouse_of(building) else {
            return (None, false);
        };
        let Some(order_record) = self.world.order(order) else {
            return (None, false);
        };
        let Some(job) = order_record.job().and_then(|wanted| stock.supplies(wanted)) else {
            return (None, false);
        };
        let Some(owner) = self.world.flag(flag).map(|flag| flag.owner) else {
            return (None, false);
        };
        let Some(flag_node) = self.world.flag(flag).map(|flag| flag.node) else {
            return (None, false);
        };
        let mut figure = Figure::fresh(FigureKind::Leaving {
            owner,
            job,
            order: Some(order),
        });
        figure.start_walk(Direction::SouthEast, DOOR_SPEED);
        let record = self.world.add_figure(flag_node, figure);
        self.world.hand_over_person(store, job);
        if let Some(building) = self.world.building_mut(building) {
            building.state |= BuildingState::DOOR;
            building.state -= BuildingState::PRESENT;
        }
        (Some(record), false)
    }

    /// A settler that has walked out to its storehouse's flag (`0x366d5`): it frees the door,
    /// becomes a walker and either arrives at once or sets off along the roads. One sent out
    /// with no order first finds a storehouse that takes its job and orders itself there; with
    /// none, or no flag, it is lost. One with no route from the flag gives up its job and is lost
    /// on the spot, without the walk home a walker further on gets.
    fn reach_flag(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Leaving { owner, order, job } = figure.kind else {
            return;
        };
        let order = if order.is_some() {
            order
        } else {
            let Some(home) = self
                .world
                .flag_at(figure.node())
                .and_then(|here| self.home_flag(here, job))
            else {
                self.make_lost(record);
                return;
            };
            let order = self.world.order_person(home, job, WORKER_MODE, 0);
            if let Some(placed) = order.and_then(|order| self.world.orders.get_mut(order)) {
                placed.delivering = true;
            }
            order
        };
        let node = figure.node();
        self.world.move_figure(record, node);
        let door = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        if let MapObject::Building(building) = self.world.object(door)
            && let Some(building) = self.world.building_mut(building)
        {
            building.state |= BuildingState::PRESENT;
        }
        let Some((order, order_record, job)) = order.and_then(|o| {
            let order_record = *self.world.order(o)?;
            Some((o, order_record, order_record.job()?))
        }) else {
            self.world.remove_figure(record);
            return;
        };
        let Some(here) = self.world.flag_at(node) else {
            self.lose(record);
            return;
        };
        if let Some(figure) = self.world.figure_mut(record) {
            figure.kind = FigureKind::Walker(Walker {
                owner,
                job,
                order,
                mode: order_record.mode,
                on: OnRoad {
                    road: 0,
                    position: 0,
                    to_start: false,
                },
            });
        }
        if here == order_record.flag {
            self.arrive(record);
        } else if !self.route(record, here) {
            self.drop_job(record);
            self.make_lost(record);
        }
    }

    /// One step of a walker that is not walking (`0x38898`): the next node along its road, or at
    /// the flag it was heading for, the hand-over or the next road. A freed order reads as no
    /// flag: a walker sent for a road is lost, any other finds no route to it and goes home.
    fn step_walker(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Walker(walker) = figure.kind else {
            return;
        };
        let Some(road) = self.world.road(walker.on.road) else {
            self.lose(record);
            return;
        };
        let (end, end_position) = if walker.on.to_start {
            (Some(road.start), 0)
        } else {
            (road.end, u8::try_from(road.edges()).unwrap_or(u8::MAX))
        };
        if walker.on.position != end_position {
            let position = if walker.on.position < end_position {
                walker.on.position + 1
            } else {
                walker.on.position - 1
            };
            if !self.walk_on(record, walker.on.road, position) {
                self.lose(record);
                return;
            }
            if let Some(figure) = self.world.figure_mut(record)
                && let FigureKind::Walker(walker) = &mut figure.kind
            {
                walker.on.position = position;
            }
            return;
        }
        let Some(end) = end else {
            self.lose(record);
            return;
        };
        let Some(target) = self.walker_target(&walker) else {
            if walker.mode == HOME_MODE {
                if !self.send_home(record, end) {
                    self.make_lost(record);
                }
            } else if walker.mode == ROAD_MODE {
                self.lose(record);
            } else {
                self.no_route(record, end);
            }
            return;
        };
        let arrived = if walker.mode == ROAD_MODE {
            self.ordered_road(target, walker.order)
                .and_then(|road| self.world.road(road))
                .is_some_and(|road| road.start == end || road.end == Some(end))
        } else {
            end == target
        };
        if arrived {
            self.arrive(record);
        } else if !self.route(record, end) {
            self.no_route(record, end);
        }
    }

    /// The flag walker `walker` heads for (`0x3886c`): its order's flag, or for a walker going
    /// home the storehouse flag it holds.
    fn walker_target(&self, walker: &Walker) -> Option<u16> {
        if walker.mode == HOME_MODE {
            return self.world.flag(walker.order).map(|_| walker.order);
        }
        self.world.order(walker.order).map(|order| order.flag)
    }

    /// Walker `record` at flag `flag` has no route on (`0x387a0` failed): it gives up its job and
    /// goes home instead, or with no storehouse to go to becomes lost.
    fn no_route(&mut self, record: u16, flag: u16) {
        self.drop_job(record);
        if !self.send_home(record, flag) {
            self.make_lost(record);
        }
    }

    /// Turns walker `record` at flag `flag` for home and sets it off: [`Self::aim_home`], then
    /// with the storehouse at `flag` it stands for a step and walks in on the next, as the
    /// original's second routing attempt fails first; otherwise it routes towards it. Returns
    /// false when there is no storehouse or no route to it.
    pub(crate) fn send_home(&mut self, record: u16, flag: u16) -> bool {
        let Some(home) = self.aim_home(record, flag) else {
            return false;
        };
        if home == flag {
            match self.at_flag_end(flag) {
                Some(on) => {
                    if let Some(FigureKind::Walker(walker)) = self.figure_kind_mut(record) {
                        walker.on = on;
                    }
                }
                None => self.arrive(record),
            }
            return true;
        }
        self.route(record, flag)
    }

    /// A place on the first road at flag `flag`, in direction order, at the flag's end: where a
    /// walker standing at the flag looks for its next step.
    fn at_flag_end(&self, flag: u16) -> Option<OnRoad> {
        let link = self.world.flag(flag)?.links.into_iter().flatten().next()?;
        let road = self.world.road(link.road)?;
        Some(if road.start == flag {
            OnRoad {
                road: link.road,
                position: 0,
                to_start: true,
            }
        } else {
            OnRoad {
                road: link.road,
                position: u8::try_from(road.edges()).unwrap_or(u8::MAX),
                to_start: false,
            }
        })
    }

    /// Aims walker `record` at home (`0x38b48`): the nearest finished storehouse to `flag` by
    /// plain road search that has not stopped the walker's job becomes its target, which it heads
    /// for from the next flag it reaches. Returns that storehouse's flag, or `None` when there is
    /// none.
    pub(crate) fn aim_home(&mut self, record: u16, flag: u16) -> Option<u16> {
        let FigureKind::Walker(walker) = self.world.figure(record)?.kind else {
            return None;
        };
        let home = self.home_flag(flag, walker.job)?;
        if let Some(FigureKind::Walker(walker)) = self.figure_kind_mut(record) {
            walker.mode = HOME_MODE;
            walker.order = home;
        }
        Some(home)
    }

    /// The flag of the first finished storehouse that takes job `job` in, by plain road search
    /// from `flag` (`0x3c40c`, limit 250, options 0, predicate `0x3c380`).
    pub(crate) fn home_flag(&self, flag: u16, job: Job) -> Option<u16> {
        search(
            &self.world,
            flag,
            SEARCH_LIMIT,
            SearchOptions::empty(),
            |reached| {
                self.storehouse_at_flag(reached.flag)
                    .is_some_and(|(_, store)| store.takes_job(job))
            },
        )
        .map(|reached| reached.flag)
    }

    /// Sends a new settler of `owner` with job `job` home from flag `flag` (`0x377d8`): a walker
    /// held like every new record, aimed at the nearest storehouse. With none it is made lost on
    /// the flag instead (`0x370d0`).
    pub(crate) fn walk_home(&mut self, owner: crate::world::Player, flag: u16, job: Job) {
        let Some(node) = self.world.flag(flag).map(|flag| flag.node) else {
            return;
        };
        let Some(home) = self.home_flag(flag, job) else {
            self.lost_at(owner, node, job);
            return;
        };
        let record = self.world.add_figure(
            node,
            Figure::fresh(FigureKind::Walker(Walker {
                owner,
                job,
                order: home,
                mode: HOME_MODE,
                on: OnRoad {
                    road: 0,
                    position: 0,
                    to_start: false,
                },
            })),
        );
        if home == flag {
            self.arrive(record);
        } else if !self.route(record, flag) {
            self.make_lost(record);
        }
    }

    /// The first road at flag `flag`, in direction order, whose open order is `order`.
    fn ordered_road(&self, flag: u16, order: u16) -> Option<u16> {
        let flag = self.world.flag(flag)?;
        flag.links
            .iter()
            .flatten()
            .filter(|link| link.far.is_some())
            .map(|link| link.road)
            .find(|&road| {
                self.world
                    .road(road)
                    .is_some_and(|road| road.order == Some(order))
            })
    }

    /// Starts figure `record` walking to position `position` of road `road`, at the speed for
    /// the slope (`0x3308c`). Returns false when that node is not next to the figure.
    pub(crate) fn walk_on(&mut self, record: u16, road: u16, position: u8) -> bool {
        let Some(here) = self.world.figure(record).map(Figure::node) else {
            return false;
        };
        let Some(there) = self.world.road_node(road, position) else {
            return false;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let Some(direction) = here.direction_to(there, width, height) else {
            return false;
        };
        let rise = i16::from(self.nodes.get(there).altitude) - i16::from(self.nodes.get(here).altitude);
        let speed = self.walking_speed(rise);
        self.world.move_figure(record, there);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, speed);
        }
        true
    }

    /// Puts walker `record`, standing at flag `here`, on the first road towards its order's flag
    /// (`0x387a0`), counting the road towards a donkey. Returns false when there is no route.
    fn route(&mut self, record: u16, here: u16) -> bool {
        let Some(FigureKind::Walker(walker)) = self.world.figure(record).map(|figure| figure.kind) else {
            return false;
        };
        let Some(target) = self.walker_target(&walker) else {
            return false;
        };
        let Some(next) = next_flag(&self.world, here, target, SEARCH_LIMIT, SearchOptions::empty()) else {
            return false;
        };
        let Some(link) = self.world.flag(here).and_then(|flag| {
            flag.links
                .iter()
                .flatten()
                .find(|link| link.far == Some(next))
                .copied()
        }) else {
            return false;
        };
        let Some(road) = self.world.road(link.road) else {
            return false;
        };
        let on = if road.start == here {
            OnRoad {
                road: link.road,
                position: 0,
                to_start: false,
            }
        } else {
            OnRoad {
                road: link.road,
                position: u8::try_from(road.edges()).unwrap_or(u8::MAX),
                to_start: true,
            }
        };
        if let Some(figure) = self.world.figure_mut(record)
            && let FigureKind::Walker(walker) = &mut figure.kind
        {
            walker.on = on;
        }
        self.count_traffic(link.road);
        true
    }

    /// Counts a pick-up or a walker entering road `record` towards a donkey (`0x26518`). An
    /// ordinary road with no open order counts; at 100 it becomes a donkey road, its ordinary
    /// end flags become donkey flags, and it orders a pack donkey to its start flag.
    pub(crate) fn count_traffic(&mut self, record: u16) {
        let Some(road) = self.world.road_mut(record) else {
            return;
        };
        if road.order.is_some() || road.kind != RoadKind::Normal {
            return;
        }
        road.traffic = road.traffic.wrapping_add(1);
        if road.traffic < DONKEY_TRAFFIC {
            return;
        }
        road.kind = RoadKind::Donkey;
        let road = road.clone();
        self.world.paint(&mut self.nodes, &road, Some(RoadKind::Donkey));
        for flag in [Some(road.start), road.end].into_iter().flatten() {
            if let Some(flag) = self.world.flag_mut(flag)
                && flag.kind == FlagKind::Normal
            {
                flag.kind = FlagKind::Donkey;
            }
        }
        let order = self
            .world
            .order_person(road.start, Job::PACK_DONKEY, ROAD_MODE, 0);
        if let Some(road) = self.world.road_mut(record) {
            road.order = order;
        }
    }

    /// Walker `record` has reached the flag it heads for (`0x378cc`). A walker going home turns
    /// in at the storehouse's door (`0x37d3d`), a builder starts work on its site (`0x37dca`),
    /// a planer starts levelling it (`0x37e0a`), and a workplace's worker walks in (`0x37cb4`). For a road's carrier order this is the hand-over (`0x37940`): the order is
    /// freed and the walker becomes the carrier of the first road at this flag, in direction
    /// order, that holds the order. Other walkers are not modelled yet and leave the game.
    fn arrive(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Walker(walker) = figure.kind else {
            return;
        };
        match walker.mode {
            HOME_MODE => {
                self.enter_storehouse(record, walker.owner, walker.job);
                return;
            }
            BUILDER_MODE => {
                self.builder_arrives(record, walker.order);
                return;
            }
            PLANER_MODE => {
                self.planer_arrives(record, walker.order);
                return;
            }
            WORKER_MODE => {
                let sent = self
                    .world
                    .order(walker.order)
                    .is_some_and(|order| order.tag == crate::explore::SENT_TAG);
                if sent && (walker.job == Job::GEOLOGIST || walker.job == Job::SCOUT) {
                    self.explorer_arrives(record, walker);
                } else {
                    self.settle_worker(record, walker.owner, walker.job, walker.order);
                }
                return;
            }
            ROAD_MODE => {}
            _ => {
                self.world.remove_figure(record);
                return;
            }
        }
        let Some(flag) = self.world.flag_at(figure.node()) else {
            self.world.remove_figure(record);
            return;
        };
        let Some(road_record) = self.ordered_road(flag, walker.order) else {
            self.world.remove_figure(record);
            return;
        };
        self.world.orders.remove(walker.order);
        let Some(road) = self.world.road_mut(road_record) else {
            return;
        };
        road.order = None;
        road.served = true;
        road.carriers = road.carriers.saturating_add(1);
        let transport = if walker.job == Job::PACK_DONKEY {
            Transport::Donkey
        } else if road.kind == RoadKind::Water {
            Transport::Boat
        } else {
            Transport::Foot
        };
        let position = if road.start == flag {
            0
        } else {
            u8::try_from(road.edges()).unwrap_or(u8::MAX)
        };
        if let Some(figure) = self.world.figure_mut(record) {
            figure.kind = FigureKind::Carrier(Carrier {
                owner: walker.owner,
                on: OnRoad {
                    road: road_record,
                    position,
                    to_start: false,
                },
                state: CarrierState::Idle,
                transport,
                ware: None,
                shows: None,
                animation: None,
            });
        }
    }

    /// Walker `record` turns in at the door north-west of its flag (`0x37d3d`) and walks in to
    /// be added to the storehouse's stock.
    fn enter_storehouse(&mut self, record: u16, owner: crate::world::Player, job: Job) {
        let Some(node) = self.world.figure(record).map(Figure::node) else {
            return;
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Entering { owner, job };
        }
        let door = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        self.world.move_figure(record, door);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, DOOR_SPEED);
        }
    }

    /// Settler `record` finds no way on: it drops its job and becomes lost (`0x32454`,
    /// `0x36ebc`). `S2.EXE` first sends a walker that is not at its storehouse's flag back to the
    /// nearest storehouse that takes its job; Robigo does not model that walk yet.
    pub(crate) fn lose(&mut self, record: u16) {
        self.drop_job(record);
        self.make_lost(record);
    }
}

impl crate::world::World {
    /// The node at `position` of road `record` (`0x29098`): the start flag at 0, the end flag at
    /// the road's edge count, inner nodes in between.
    #[must_use]
    pub fn road_node(&self, record: u16, position: u8) -> Option<Node> {
        let road = self.road(record)?;
        let position = u16::from(position);
        if position == 0 {
            return self.flag(road.start).map(|flag| flag.node);
        }
        if position == road.edges() {
            return road.end.and_then(|end| self.flag(end)).map(|flag| flag.node);
        }
        road.path.get(usize::from(position - 1)).copied()
    }
}

#[cfg(test)]
pub(crate) mod tests;
