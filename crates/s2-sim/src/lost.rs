//! Settlers that lose their road or their way.
//!
//! A walker with no route first tries to walk home along the roads ([`Game::send_home`]).
//!
//! The skeleton a lost settler leaves ages and goes in the ageing pass (`World::age_row`).
//!
//! A settler that dies comes off its owner's people count ([`crate::people`]).

use crate::figures::{CarrierState, Exploring, FigureKind, PlanerWork, SoldierState, Transport, Trip};
use crate::game::Game;
use crate::grid::Direction;
use crate::homeward::RING;
use crate::job::Job;
use crate::settlers::HOME_MODE;
use crate::transport::WareState;
use crate::world::{MapObject, Remnant};

/// Wandered edges a lost settler lasts at least (`0x370b6`).
const LOST_STEPS: u8 = 30;

/// The most wandered edges a lost settler gets beyond [`LOST_STEPS`].
const LOST_SPREAD: u16 = 100;

/// The countdown of a lost soldier, which draws nothing.
const LOST_SOLDIER_STEPS: u8 = 254;

/// The byte a freed figure record is filled with (`0x788d0`).
const FREED_FILL: u8 = 206;

/// Entries a wander direction gets when it is either side of the way the settler faces, 1
/// otherwise.
const AHEAD_WEIGHT: usize = 7;

/// The job a carrier becomes when it is lost: a pack donkey stays one, anyone else is a helper.
const fn carrier_job(transport: Transport) -> Job {
    match transport {
        Transport::Donkey => Job::PACK_DONKEY,
        Transport::Foot | Transport::Boat => Job::HELPER,
    }
}

impl Game {
    /// Removes road `record` with what is on it (`0x25b9c`). Unless the road is still being
    /// drawn, every settler on its inner nodes that removal does not spare
    /// ([`Figure::spared_by_removal`]), then every carrier and walker of this road at
    /// its two flags and their buildings, drops its job and becomes lost, in that order. A
    /// carrier delivering into one of the two buildings carries on, with no road to come back
    /// to. A helper on its way to the road becomes lost too, and the road's order is freed. Then
    /// the road itself goes.
    pub(crate) fn remove_road_record(&mut self, record: u16) {
        let Some(road) = self.world.road(record).cloned() else {
            return;
        };
        if let Some(end) = road.end {
            let mut stranded = Vec::new();
            for &node in &road.path {
                stranded.extend(
                    self.world
                        .figures_at(node)
                        .filter(|(_, figure)| !figure.spared_by_removal())
                        .map(|(figure, _)| figure),
                );
            }
            let (width, height) = (self.nodes.width(), self.nodes.height());
            for flag in [road.start, end] {
                let Some(node) = self.world.flag(flag).map(|flag| flag.node) else {
                    continue;
                };
                for at in [node, node.neighbour(Direction::NorthWest, width, height)] {
                    let delivering: Vec<u16> = self
                        .world
                        .figures_at(at)
                        .filter(|(_, figure)| {
                            matches!(figure.kind, FigureKind::Delivering(delivery) if delivery.road == Some(record))
                        })
                        .map(|(figure, _)| figure)
                        .collect();
                    for figure in delivering {
                        if let Some(FigureKind::Delivering(delivery)) = self.figure_kind_mut(figure) {
                            delivery.road = None;
                        }
                    }
                    stranded.extend(
                        self.world
                            .figures_at(at)
                            .filter(|(_, figure)| match figure.kind {
                                FigureKind::Carrier(carrier) => carrier.on.road == record,
                                FigureKind::Walker(walker) => walker.on.road == record,
                                _ => false,
                            })
                            .map(|(figure, _)| figure),
                    );
                }
            }
            for figure in stranded {
                self.drop_job(figure);
                self.make_lost(figure);
            }
        }
        if let Some(order) = road.order {
            if let Some(helper) = self
                .world
                .order(order)
                .filter(|order| order.delivering)
                .and_then(|order| order.on_way)
            {
                if self.world.figure(helper).is_some() {
                    self.make_lost(helper);
                } else if !self.world.bug_fixes.on() {
                    self.lose_freed_record(helper);
                }
            }
            self.world.orders.remove(order);
        }
        self.world.remove_road(&mut self.nodes, record);
    }

    /// Settler `record` gives up what it was doing (`0x32454`). A walker's order becomes wanted
    /// again, keeping its urgency and the figure it last named. A carrier fetching a ware leaves
    /// it waiting again, one carrying a ware loses it with its order, and one playing an idle
    /// animation stops it.
    pub(crate) fn drop_job(&mut self, record: u16) {
        match self.world.figure(record).map(|figure| figure.kind) {
            Some(FigureKind::Walker(walker)) if walker.mode != HOME_MODE => {
                if let Some(order) = self.world.orders.get_mut(walker.order) {
                    order.delivering = false;
                }
            }
            Some(FigureKind::Carrier(carrier)) => {
                match (carrier.state, carrier.ware) {
                    (CarrierState::Fetching, Some(ware)) => {
                        if let Some(ware) = self.world.wares.get_mut(ware)
                            && ware.state == WareState::Taken
                        {
                            ware.state = WareState::Waiting;
                        }
                    }
                    (CarrierState::Carrying, Some(ware)) => {
                        self.world.lose_ware(carrier.owner, ware);
                    }
                    _ => {}
                }
                if let Some(animation) = carrier.animation {
                    self.world.animations.remove(animation);
                }
                if let Some(FigureKind::Carrier(carrier)) = self.figure_kind_mut(record) {
                    carrier.animation = None;
                    carrier.ware = None;
                }
            }
            _ => {}
        }
    }

    /// Makes settler `record` a lost settler where it stands or walks (`0x36ebc`). A carrier
    /// becomes a helper, or a pack donkey if it was one. A carrier delivering into a building is
    /// in its doorway and, with the bug fixes off, becomes a helper even when it is a pack donkey
    /// (`0x36fe6`). A load the job's row names comes off the
    /// owner's ware count (`0x3705d`). The countdown is 30 plus a draw from 0 to
    /// 100, or 254 for a soldier, which draws nothing. One held in a queue stops queueing and walks
    /// on (`0x323d0`). Animals, ships and settlers already lost are left alone. A settler lost in the middle of a work animation leaves the animation record
    /// allocated, as `S2.EXE` does.
    pub(crate) fn make_lost(&mut self, record: u16) {
        let Some(kind) = self.world.figure(record).map(|figure| figure.kind) else {
            return;
        };
        let (owner, job) = match kind {
            FigureKind::Animal { .. }
            | FigureKind::Ship { .. }
            | FigureKind::Lost { .. }
            | FigureKind::Builder(_) => return,
            FigureKind::Carrier(carrier) => {
                if let Some(animation) = carrier.animation {
                    self.world.animations.remove(animation);
                }
                (carrier.owner, carrier_job(carrier.transport))
            }
            FigureKind::Delivering(delivery) if self.world.bug_fixes.on() => {
                (delivery.owner, carrier_job(delivery.transport))
            }
            FigureKind::Delivering(delivery) => (delivery.owner, Job::HELPER),
            FigureKind::Bringing { owner, .. } => (owner, Job::HELPER),
            FigureKind::Walker(walker) => (walker.owner, walker.job),
            FigureKind::Leaving { owner, job, .. }
            | FigureKind::Entering { owner, job }
            | FigureKind::Settling { owner, job, .. } => (owner, job),
            FigureKind::Homeward { owner, job, path, .. } => {
                self.world.paths.remove(path);
                (owner, job)
            }
            FigureKind::Gatherer(gatherer) => {
                if let Trip::Leaving { path } | Trip::Walking { path } = gatherer.trip {
                    self.world.paths.remove(path);
                }
                (gatherer.owner, gatherer.job)
            }
            FigureKind::Porter(porter) => (porter.owner, porter.job),
            FigureKind::Planer(planer) => {
                if let PlanerWork::Walking { path } = planer.work {
                    self.world.paths.remove(path);
                }
                (planer.owner, Job::PLANER)
            }
            FigureKind::Explorer(explorer) => {
                if let Exploring::Walking { path } = explorer.work {
                    self.world.paths.remove(path);
                }
                (explorer.owner, explorer.job)
            }
            FigureKind::Soldier(soldier) => {
                self.forget_target(record);
                self.free_record(record);
                if let SoldierState::Walking { path: Some(path), .. }
                | SoldierState::Leaving { path: Some(path), .. } = soldier.state
                {
                    self.world.paths.remove(path);
                }
                if let SoldierState::Animating {
                    animation: Some(animation),
                } = soldier.state
                {
                    self.world.animations.remove(animation);
                }
                (soldier.owner, soldier.job)
            }
        };
        self.lose_as(record, owner, job);
    }

    /// A carrier delivering into a building is back at its flag with no road to go back to
    /// (`0x363c2`): it turns back into a carrier first, so a pack donkey is lost as a pack donkey.
    pub(crate) fn lose_delivery(&mut self, record: u16, delivery: crate::figures::Delivery) {
        self.lose_as(record, delivery.owner, carrier_job(delivery.transport));
    }

    /// With the bug fixes off, `S2.EXE` makes lost a helper record that has already been freed (`0x36ebc` on a record
    /// `0x788d0` filled with 206). The fill reads as owner 0, job 206 and a node of
    /// (−12594, −12594), which wraps onto the map, and the figure is on no node's list. Job 206
    /// reads on into a later nation's row of the job table, so the settler takes that row's job
    /// and player 0 loses its ware. Below the highest record in use the record becomes a lost
    /// settler the figure loop runs like any other; above it only the draw and the ware count
    /// change.
    fn lose_freed_record(&mut self, record: u16) {
        let fill = i16::from_le_bytes([FREED_FILL; 2]);
        let node = crate::grid::Node::wrapped(
            i32::from(fill),
            i32::from(fill),
            self.nodes.width(),
            self.nodes.height(),
        );
        let owner = crate::world::Player::ALL[0];
        let job = Job(FREED_FILL);
        let mut figure = crate::figures::Figure::new(FigureKind::Lost {
            owner,
            job,
            countdown: 0,
        });
        figure.node = node;
        self.world.place_loose_figure(record, figure);
        self.lose_as(record, owner, job);
    }

    /// The part of [`Self::make_lost`] after the job is known.
    fn lose_as(&mut self, record: u16, owner: crate::world::Player, job: Job) {
        self.clear_queue_marks(record);
        let nation = self.world.nations[owner.index()];
        if let Some(ware) = crate::wares::carried(nation, job) {
            self.world.drop_ware(owner, ware, 1);
        }
        let job = crate::people::counts_as(nation, job);
        let countdown = self.lost_countdown(job);
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Lost {
                owner,
                job,
                countdown,
            };
        }
    }

    /// How long a lost settler of job `job` lasts: 30 plus a draw from 0 to 100, or 254 for a
    /// soldier, which draws nothing.
    fn lost_countdown(&mut self, job: Job) -> u8 {
        if job.is_soldier() {
            LOST_SOLDIER_STEPS
        } else {
            LOST_STEPS + u8::try_from(self.rng.up_to(LOST_SPREAD)).unwrap_or(0)
        }
    }

    /// Makes a new lost settler of `owner` with job `job` on `node` (`0x370d0`), held like every
    /// new record. It skips the job change of [`Self::make_lost`].
    pub(crate) fn lost_at(&mut self, owner: crate::world::Player, node: crate::grid::Node, job: Job) {
        let countdown = self.lost_countdown(job);
        self.world.add_figure(
            node,
            crate::figures::Figure::fresh(FigureKind::Lost {
                owner,
                job,
                countdown,
            }),
        );
    }

    /// One step of lost settler `record` that is not walking (`0x37300`). The countdown runs
    /// down, and at 0 the settler dies, leaving a skeleton on an empty node. Otherwise, once per
    /// 24-step window, the first lost settler to get here looks for a way home. A settler that
    /// finds none wanders to a neighbour.
    pub(crate) fn step_lost(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Lost {
            owner,
            job,
            countdown,
        } = figure.kind
        else {
            return;
        };
        let countdown = countdown.saturating_sub(1);
        if let Some(FigureKind::Lost { countdown: left, .. }) = self.figure_kind_mut(record) {
            *left = countdown;
        }
        let node = figure.node();
        if countdown == 0 {
            if self.world.object(node) == MapObject::Nothing {
                self.world.set_object(node, MapObject::Remnant(Remnant::SKELETON));
            }
            self.world.drop_people(owner, job, 1);
            self.world.remove_figure(record);
            return;
        }
        if !self.world.rescue_tried {
            self.world.rescue_tried = true;
            if self.try_rescue(record, owner, job, node, countdown) {
                return;
            }
        }
        self.wander(record, (owner, job), node, figure.direction);
    }

    /// A wander step (`0x37300` step 3): each walkable neighbour not held, in the order north-east,
    /// east round to north-west, gets one entry, or seven when it is either side of `facing`. A
    /// draw picks the entry. With no neighbour to go to the settler disappears and comes off the
    /// owner's people count (`0x3758a`).
    fn wander(
        &mut self,
        record: u16,
        (owner, job): (crate::world::Player, Job),
        node: crate::grid::Node,
        facing: Direction,
    ) {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let beside = [
            Direction::from_index(facing.index() + 5),
            Direction::from_index(facing.index() + 1),
        ];
        let mut entries = Vec::new();
        for direction in RING {
            let data = self.nodes.get(node.neighbour(direction, width, height));
            if data.flags.contains(crate::nodes::NodeFlags::HELD) || !data.sites.passable() {
                continue;
            }
            let weight = if beside.contains(&direction) {
                AHEAD_WEIGHT
            } else {
                1
            };
            entries.extend(std::iter::repeat_n(direction, weight));
        }
        if entries.is_empty() {
            self.world.drop_people(owner, job, 1);
            self.world.remove_figure(record);
            return;
        }
        let pick = self.rng.up_to(u16::try_from(entries.len() - 1).unwrap_or(0));
        let direction = entries[usize::from(pick)];
        let next = node.neighbour(direction, width, height);
        let rise = i16::from(self.nodes.get(next).altitude) - i16::from(self.nodes.get(node).altitude);
        let speed = self.walking_speed(rise);
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, speed);
        }
    }
}

#[cfg(test)]
mod tests;
