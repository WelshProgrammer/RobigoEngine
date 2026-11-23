//! The builder on a site: its arrival, its walk between the site's waypoints with the hammering
//! animations, the building tick that uses the site's boards and stones, and the finished
//! building.
//!
//! Not yet modelled: military buildings' soldiers, the statistics and the completion message.

use crate::figures::{Builder, Figure, FigureKind};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::work::WORK_DOORS;
use crate::workplace::WORKER_MODE;
use crate::world::Storehouse;
use crate::world::{BuildingKind, BuildingState, Category, WAITING};

/// Where a new builder starts, in pixels from the building: on the flag, on flat ground.
const START: i8 = 28;

/// The hammering animation a builder plays at each waypoint (`0x331d4`): records in
/// `ROM_ANIM.DAT`.
const HAMMERING: [u16; 6] = [3424, 3424, 3386, 3386, 3402, 3402];

/// The last waypoint index.
const LAST_WAYPOINT: u8 = 5;

/// A building tick runs the construction step when the timer is 0 or past this.
const TIMER_DONE: u8 = 8;

/// Steps between productivity ratings, on the building tick's offset.
const RATING_PERIOD: u16 = 960;

/// A military building's promotion timer stops counting at this (`0x2df75`).
const PROMOTION_FULL: u8 = 100;

impl Game {
    /// Builder walker `record` has reached its site's flag (`0x37dca`, `0x2fad1`): the site has
    /// its builder present and heading for its first waypoint, a builder figure takes the
    /// walker's place, and the construction step runs at once. The order is freed.
    pub(crate) fn builder_arrives(&mut self, record: u16, order: u16) {
        let site = self
            .world
            .order(order)
            .and_then(|order| self.world.building_at_flag(order.flag));
        if let Some(building) = site {
            self.start_builder(building);
        }
        self.world.orders.remove(order);
        self.world.remove_figure(record);
    }

    /// Site `building` gets its builder (`0x2fad1`): present and heading for its first waypoint,
    /// with a builder figure at work, and the construction step runs at once.
    pub(crate) fn start_builder(&mut self, building: u16) {
        if let Some(site) = self.world.building(building).copied() {
            let first = self
                .world
                .waypoints(site.owner)
                .get(site.kind)
                .map_or(0, |waypoints| waypoints.first);
            let builder = self
                .world
                .add_loose_figure(Figure::new(FigureKind::Builder(Builder {
                    building,
                    facing: Direction::NorthWest,
                    leg: None,
                    x: START,
                    y: START,
                })));
            if let Some(site) = self.world.building_mut(building) {
                site.state |= BuildingState::PRESENT;
                site.waypoint = first;
                site.builder = Some(builder);
            }
            self.construct(building);
        }
    }

    /// One step of builder `record` (`0x331d4`). While an animation runs it waits. Otherwise it
    /// moves one pixel towards its waypoint, facing the way it set off on this leg. On arriving
    /// it starts the waypoint's hammering, unless the site waits for materials, and picks the
    /// next waypoint: from the ends it turns back, elsewhere a draw sends it either way.
    pub(crate) fn step_builder(&mut self, record: u16) {
        let Some(FigureKind::Builder(mut builder)) = self.world.figure(record).map(|figure| figure.kind)
        else {
            return;
        };
        let Some(site) = self.world.building(builder.building).copied() else {
            return;
        };
        if site.animation.is_some() {
            return;
        }
        let Some(waypoints) = self.world.waypoints(site.owner).get(site.kind).copied() else {
            return;
        };
        let index = site.waypoint;
        let target = (waypoints.x[usize::from(index)], waypoints.y[usize::from(index)]);
        let (dx, dy) = (
            i16::from(target.0) - i16::from(builder.x),
            i16::from(target.1) - i16::from(builder.y),
        );
        let direction = match (dx > 0, dy.signum()) {
            (true, 1) => Direction::SouthEast,
            (true, -1) => Direction::NorthEast,
            (true, _) => Direction::East,
            (false, 1) => Direction::SouthWest,
            (false, -1) => Direction::NorthWest,
            (false, _) => Direction::West,
        };
        if builder.leg != Some(index) {
            builder.facing = direction;
            builder.leg = Some(index);
        }
        let (step_x, step_y) = match direction {
            Direction::East => (1, 0),
            Direction::SouthEast => (1, 1),
            Direction::SouthWest => (-1, 1),
            Direction::West => (-1, 0),
            Direction::NorthWest => (-1, -1),
            Direction::NorthEast => (1, -1),
        };
        builder.x = builder.x.wrapping_add(step_x);
        builder.y = builder.y.wrapping_add(step_y);
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Builder(builder);
        }
        if (builder.x, builder.y) != target {
            return;
        }
        if site.timer != WAITING {
            let animation = self.start_animation(HAMMERING[usize::from(index)], site.node);
            if let Some(site) = self.world.building_mut(builder.building) {
                site.animation = animation;
            }
        }
        let next = match index {
            0 => 1,
            LAST_WAYPOINT => LAST_WAYPOINT - 1,
            _ if self.rng.up_to(1) != 0 => index - 1,
            _ => index + 1,
        };
        if let Some(site) = self.world.building_mut(builder.building) {
            site.waypoint = next;
        }
    }

    /// The building animation pass (`0x2fcf3`), every other step after the figure loop: each
    /// building's running animation, in record order, takes one step. One that ends closes the
    /// door of a few types.
    pub(crate) fn animate_buildings(&mut self) {
        let running: Vec<(u16, u16, Node)> = self
            .world
            .buildings()
            .filter_map(|(record, building)| {
                building
                    .animation
                    .map(|animation| (record, animation, building.node))
            })
            .collect();
        for (record, animation, at) in running {
            if self.run_animation(animation, at) {
                continue;
            }
            if let Some(building) = self.world.building_mut(record) {
                building.animation = None;
                if WORK_DOORS.contains(&building.kind) {
                    building.state -= BuildingState::DOOR;
                }
            }
        }
    }

    /// The building tick (`0x2dede`), every 48 steps, in record order: each site with its
    /// builder present runs the construction step when its timer is 0 or past 8, which includes
    /// a site waiting for materials, and any other timer counts up. Every 960 steps each manned
    /// workplace has its productivity rated. Each finished storehouse takes its send-out turn,
    /// and each occupied military building's promotion timer counts up by its size class, 1 to
    /// 4, until it reaches 100 (`0x2df75`).
    pub(crate) fn building_tick(&mut self) {
        let types = self.world.building_types();
        let ticked: Vec<(u16, Option<Category>, u8)> = self
            .world
            .buildings()
            .filter_map(|(record, building)| {
                let row = types.get(building.kind);
                let category = row.map(|row| row.category);
                let class = row.map_or(0, |row| row.size.class());
                let site = building
                    .state
                    .contains(BuildingState::BUILDING | BuildingState::PRESENT);
                let finished = building.state.contains(BuildingState::MANNED) && !building.is_site();
                let ticks = site
                    || (finished
                        && matches!(
                            category,
                            Some(Category::Workplace | Category::Storehouse | Category::Military)
                        ));
                ticks.then_some((record, category, class))
            })
            .collect();
        let rate = self.clock.every(RATING_PERIOD, 2);
        for (record, category, class) in ticked {
            let Some(building) = self.world.building_mut(record) else {
                continue;
            };
            if category == Some(Category::Storehouse) && !building.is_site() {
                if building.kind == BuildingKind::HEADQUARTERS {
                    self.fill_reserve(record);
                }
                self.send_stock_out(record);
            } else if category == Some(Category::Military) && !building.is_site() {
                if building.productivity < PROMOTION_FULL {
                    building.productivity += class;
                }
            } else if !building.is_site() {
                if rate {
                    self.rate_productivity(record);
                }
                self.work_tick(record);
            } else if building.timer == 0 || building.timer > TIMER_DONE {
                self.construct(record);
            } else {
                building.timer += 1;
            }
        }
    }

    /// The construction step (`0x2e2f4`): the site uses one board, or once the boards are all
    /// used one stone, and starts its timer. With none in stock it waits for materials and its
    /// hammering stops. With every board and stone used the building is finished.
    fn construct(&mut self, record: u16) {
        let Some(site) = self.world.building(record).copied() else {
            return;
        };
        let Some(row) = self.world.building_types().get(site.kind) else {
            return;
        };
        let wanted = (0..2).find(|&index| site.used[index] < row.cost(index));
        let Some(index) = wanted else {
            self.finish(record);
            return;
        };
        let waiting = site.stock[index] == 0;
        if let Some(site) = self.world.building_mut(record) {
            if waiting {
                site.timer = WAITING;
            } else {
                site.stock[index] -= 1;
                site.used[index] += 1;
                site.timer = 1;
            }
        }
        if waiting {
            self.stop_building_animation(record);
        }
    }

    /// Frees building `record`'s running animation (`0x2e2a7`).
    pub(crate) fn stop_building_animation(&mut self, record: u16) {
        if let Some(animation) = self
            .world
            .building_mut(record)
            .and_then(|site| site.animation.take())
        {
            self.world.animations.remove(animation);
        }
    }

    /// Site `record` is finished (`0x2e2f4`, `0x2e488`). Its builder figure is freed and, unless
    /// the building is a storehouse, a builder walks home from its flag. The building is finished
    /// with its door shown. A storehouse gets its stock record with the builder in it, and shows
    /// the door no more; a harbour gets its harbour record too.
    pub(crate) fn finish(&mut self, record: u16) {
        let Some(site) = self.world.building(record).copied() else {
            return;
        };
        if let Some(builder) = site.builder {
            self.world.free_loose_figure(builder);
        }
        self.stop_building_animation(record);
        if let Some(owner) = self.world.building_owner(record) {
            self.world.uncount_site(owner, site.kind);
            self.world.count_building(owner, site.kind);
        }
        let category = self.world.building_types().get(site.kind).map(|row| row.category);
        if category != Some(Category::Storehouse) {
            self.walk_home(site.owner, site.flag, Job::BUILDER);
        }
        if let Some(building) = self.world.building_mut(record) {
            building.builder = None;
            building.waypoint = 0;
            building.requested = 0;
            building.on_way = [0; 4];
            building.stock = [0; 4];
            building.outputs = [None; 4];
            building.state -= BuildingState::BUILDING | BuildingState::PRESENT;
            building.state |= BuildingState::DOOR | BuildingState::FINISHED;
        }
        let worker = self
            .world
            .building_types()
            .get(site.kind)
            .and_then(|row| row.worker);
        if category == Some(Category::Workplace)
            && let Some(job) = worker
        {
            self.world.order_person(site.flag, job, WORKER_MODE, 0);
        }
        if category == Some(Category::Military) {
            self.start_military(record);
        }
        if category == Some(Category::Storehouse)
            && let Some(store) = self.world.add_storehouse(Storehouse::new(record))
        {
            if let Some(store) = self.world.storehouse_mut(store) {
                store.jobs[Job::BUILDER.index()] += 1;
            }
            if site.kind == BuildingKind::HARBOUR {
                self.add_harbour(record);
            }
            if let Some(building) = self.world.building_mut(record) {
                building.state |= BuildingState::MANNED | BuildingState::PRESENT;
                building.state -= BuildingState::DOOR;
            }
        }
    }
}

#[cfg(test)]
mod tests;
