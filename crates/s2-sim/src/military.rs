//! Military buildings and their soldiers: the military settings, the record each finished
//! military building keeps, asking for soldiers and sending extras home, the first soldier's
//! claim on the land round the building, and the military pass that keeps each building staffed.
//!
//! Attacks, fights and captures are in [`crate::combat`].

use crate::figures::{FigureKind, Porter};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::DOOR_SPEED;
use crate::territory::{
    CLEAR_ORDER, Contest, SPREAD_ORDER, begin_claim, border_pass, claim_order, contest, neighbours_on_map,
    settle, spreads_from,
};
use crate::world::{BuildingKind, BuildingState, MapObject, Player};

pub use s2_systems::military::{
    ATTACK, DEFENCE, MILITARY_SETTINGS, MOST_MILITARY, MilitaryTables, MilitaryTablesError, OCCUPATION, RANK,
    RECRUITING,
};

/// The rank setting at or above which buildings ask for the strongest soldier and send the
/// weakest home.
pub(crate) const STRONGEST_FROM: u8 = 50;

/// A military building's coin slot.
const COIN_SLOT: usize = 0;

/// The promotion timer a military building's next visit must find passed to promote.
const PROMOTION_WAIT: u8 = 20;

/// Military records the pass visits per call (`0x39309`).
const PASS_SPAN: u16 = 40;

/// Most military records.
const MAX_MILITARY: usize = 1000;

/// How near an enemy building or headquarters puts a building in the border classes
/// (`0x38f4c`): within 24, and within 18 for the outer class.
const NEAR: u32 = 24;
const NEARER: u32 = 6;

/// The record a finished military building keeps (`0x1fa514`, 13 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Military {
    /// The building (`+0x00`).
    pub building: u16,
    /// How near the enemy it is, 0 to 2 (`+0x02`), as of its last visit.
    pub border: u8,
    /// Owner (`+0x04`).
    pub owner: Player,
    /// Soldiers asked for and not yet arrived (`+0x05`).
    pub requested: u8,
    /// Soldiers inside (`+0x06`).
    pub inside: u8,
    /// Soldiers inside by rank, private first (`+0x07` to `+0x0b`).
    pub ranks: [u8; 5],
    /// How many soldiers it holds (`+0x0c`).
    pub capacity: u8,
}

impl Military {
    /// The lowest rank inside, or `None` when empty (`0x396d9`).
    #[must_use]
    pub fn weakest(&self) -> Option<Job> {
        (0..)
            .zip(self.ranks)
            .find(|&(_, count)| count > 0)
            .map(|(rank, _)| Job::RANKS[rank])
    }

    /// The highest rank inside, or `None` when empty (`0x396b8`).
    #[must_use]
    pub fn strongest(&self) -> Option<Job> {
        (0..)
            .zip(self.ranks)
            .filter(|&(_, count)| count > 0)
            .last()
            .map(|(rank, _)| Job::RANKS[rank])
    }
}

/// A soldier's rank, 0 for a private, from its job.
fn rank_of(job: Job) -> Option<usize> {
    Job::RANKS.iter().position(|&rank| rank == job)
}

impl crate::world::World {
    /// `player`'s military settings.
    #[must_use]
    pub fn military_settings(&self, player: Player) -> [u8; MILITARY_SETTINGS] {
        self.military_settings[player.index()]
    }

    /// The military record of building `building`, if it has one.
    #[must_use]
    pub fn military_of(&self, building: u16) -> Option<(u16, &Military)> {
        self.military
            .iter()
            .find(|(_, record)| record.building == building)
    }

    /// Every military record in use, in record order.
    pub fn military(&self) -> impl Iterator<Item = (u16, &Military)> {
        self.military.iter()
    }

    /// The lowest rank headquarters `building` holds in reserve (`0x30d21`).
    #[must_use]
    pub fn reserve_rank(&self, building: u16) -> Option<Job> {
        let reserve = self.building(building)?.reserve;
        Job::RANKS
            .iter()
            .copied()
            .zip(reserve)
            .find_map(|(job, rank)| (rank.held > 0).then_some(job))
    }

    /// Takes one soldier of rank `job` out of headquarters `building`'s reserve (`0x30d44`).
    pub(crate) fn take_reserve(&mut self, building: u16, job: Job) {
        let Some(index) = Job::RANKS.iter().position(|&rank| rank == job) else {
            return;
        };
        if let Some(building) = self.building_mut(building)
            && let Some(rank) = building.reserve.get_mut(index)
        {
            rank.held = rank.held.saturating_sub(1);
        }
    }
}

/// How many of each rank a new headquarters keeps back (the five dwords at `0x1fa594`,
/// raised once each by `0x2c2d1`).
pub(crate) const RESERVE_WANTED: u8 = 1;

/// The most of one rank a reserve holds back (`0x30df1`).
pub(crate) const MOST_RESERVED: u8 = 15;

impl Game {
    /// Headquarters `building` fills its reserve (`0x30d6f`), on its building tick and after the
    /// player raises a rank: each rank below its wanted count takes one soldier of that rank out
    /// of the stock, so at most one per rank per pass.
    pub(crate) fn fill_reserve(&mut self, building: u16) {
        let Some(store) = self.world.storehouse_of(building).map(|(store, _)| store) else {
            return;
        };
        for (index, job) in Job::RANKS.into_iter().enumerate() {
            let Some(rank) = self
                .world
                .building(building)
                .and_then(|b| b.reserve.get(index).copied())
            else {
                continue;
            };
            if rank.held >= rank.wanted {
                continue;
            }
            let in_stock = self
                .world
                .storehouse_of(building)
                .is_some_and(|(_, stock)| stock.jobs[job.index()] > 0);
            if !in_stock {
                continue;
            }
            if let Some(stock) = self.world.storehouse_mut(store) {
                stock.jobs[job.index()] -= 1;
            }
            if let Some(rank) = self
                .world
                .building_mut(building)
                .and_then(|b| b.reserve.get_mut(index))
            {
                rank.held += 1;
            }
        }
    }

    /// Raises the reserve of rank `job` at headquarters `building` by one and fills it
    /// (`0x30de0`): nothing happens at fifteen.
    pub(crate) fn raise_reserve(&mut self, building: u16, job: Job) {
        let Some(index) = Job::RANKS.iter().position(|&rank| rank == job) else {
            return;
        };
        let Some(rank) = self
            .world
            .building_mut(building)
            .and_then(|b| b.reserve.get_mut(index))
        else {
            return;
        };
        if rank.wanted >= MOST_RESERVED {
            return;
        }
        rank.wanted += 1;
        self.fill_reserve(building);
    }

    /// Lowers the reserve of rank `job` at headquarters `building` by one (`0x30e13`): nothing
    /// happens at zero, and a soldier over the new count goes back into the stock.
    pub(crate) fn lower_reserve(&mut self, building: u16, job: Job) {
        let Some(index) = Job::RANKS.iter().position(|&rank| rank == job) else {
            return;
        };
        let Some(store) = self.world.storehouse_of(building).map(|(store, _)| store) else {
            return;
        };
        let Some(rank) = self
            .world
            .building_mut(building)
            .and_then(|b| b.reserve.get_mut(index))
        else {
            return;
        };
        if rank.wanted == 0 {
            return;
        }
        rank.wanted -= 1;
        if rank.held <= rank.wanted {
            return;
        }
        rank.held -= 1;
        if let Some(stock) = self.world.storehouse_mut(store) {
            stock.jobs[job.index()] += 1;
        }
    }

    /// Moves headquarters `building`'s reserve of rank `job` to `wanted`, one step at a time as
    /// the window's buttons do, so that lowering it puts each soldier over the new count back
    /// into the stock.
    pub(crate) fn set_reserve(&mut self, building: u16, job: Job, wanted: u8) {
        for _ in 0..=MOST_RESERVED {
            let Some(rank) = self.reserve_of(building, job) else {
                return;
            };
            match rank.wanted.cmp(&wanted) {
                std::cmp::Ordering::Less => self.raise_reserve(building, job),
                std::cmp::Ordering::Greater => self.lower_reserve(building, job),
                std::cmp::Ordering::Equal => return,
            }
        }
    }

    /// Headquarters `building`'s reserve of rank `job`.
    fn reserve_of(&self, building: u16, job: Job) -> Option<crate::world::Reserve> {
        let index = Job::RANKS.iter().position(|&rank| rank == job)?;
        self.world.building(building)?.reserve.get(index).copied()
    }

    /// A military building `record` is finished (`0x2e548`): it gets its record and asks for
    /// its first soldier.
    pub(crate) fn start_military(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let capacity = self
            .world
            .building_types()
            .get(building.kind)
            .map_or(0, |row| row.soldiers);
        let military = Military {
            building: record,
            border: 0,
            owner: building.owner,
            requested: 0,
            inside: 0,
            ranks: [0; 5],
            capacity,
        };
        if let Some(military) = self.world.military.insert(military, MAX_MILITARY) {
            self.ask_for_soldier(military);
        }
    }

    /// Military record `military` asks for one soldier (`0x393f5`): the count asked for rises
    /// first, then an order goes to the building's flag for the strongest soldier when the
    /// owner's rank setting is 50 or more, the weakest otherwise.
    fn ask_for_soldier(&mut self, military: u16) {
        let Some(record) = self.world.military.get_mut(military) else {
            return;
        };
        record.requested = record.requested.saturating_add(1);
        let building = record.building;
        let Some(flag) = self.world.building(building).map(|building| building.flag) else {
            return;
        };
        let Some(owner) = self.world.flag(flag).map(|flag| flag.owner) else {
            return;
        };
        let job = if self.world.military_settings[owner.index()][RANK] >= STRONGEST_FROM {
            Job::GENERAL
        } else {
            Job::PRIVATE
        };
        self.world
            .order_person(flag, job, crate::workplace::WORKER_MODE, 0);
    }

    /// Soldier `record` of rank `job` walks into military building `building` (`0x35cf8`). The
    /// first to arrive claims the land round it and dates the building's occupation; every one
    /// counts in the record. The building is occupied with its door shut, the order is freed
    /// and the figure is gone.
    pub(crate) fn soldier_arrives(&mut self, record: u16, building: u16, job: Job, order: u16) {
        let Some(site) = self.world.building(building).copied() else {
            return;
        };
        if !site.state.contains(BuildingState::MANNED) {
            self.claim_land(building);
            let occupied = self.world.next_occupation();
            if let Some(site) = self.world.building_mut(building) {
                site.occupied = occupied;
            }
        }
        if let Some((military, _)) = self.world.military_of(building)
            && let Some(military) = self.world.military.get_mut(military)
        {
            military.inside = military.inside.saturating_add(1);
            military.requested = military.requested.saturating_sub(1);
            if let Some(rank) = rank_of(job) {
                military.ranks[rank] = military.ranks[rank].saturating_add(1);
            }
        }
        if let Some(site) = self.world.building_mut(building) {
            site.state -= BuildingState::DOOR;
            site.state |= BuildingState::MANNED | BuildingState::PRESENT;
        }
        self.world.orders.remove(order);
        self.world.remove_figure(record);
    }

    /// Claims the land round building `record` for its owner (`0x3e0fc`): the strength of its
    /// type on its node, its ten nodes protected, then the spread from the node and every ring
    /// out to the strength. Land taken from another player first loses whatever stands on its
    /// unprotected neighbours. Then the border pass and the owner's view round it.
    pub(crate) fn claim_land(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let Some(row) = self.world.building_types().get(building.kind) else {
            return;
        };
        let destroy = building.kind != BuildingKind::HARBOUR;
        let owner = building.owner;
        let size = (self.nodes.width(), self.nodes.height());
        begin_claim(&mut self.nodes, building.node, row.strength, owner);
        for from in claim_order(size, building.node, row.strength) {
            let Some(value) = spreads_from(&self.nodes, from, owner) else {
                continue;
            };
            for node in neighbours_on_map(size, from, SPREAD_ORDER) {
                match contest(&self.nodes, node, owner, value - 1) {
                    Contest::Gain | Contest::Raise => settle(&mut self.nodes, node, owner, value - 1),
                    Contest::Take => {
                        if destroy {
                            for around in neighbours_on_map(size, node, CLEAR_ORDER) {
                                if !self.nodes.get(around).protected {
                                    self.clear_for_claim(around);
                                }
                            }
                        }
                        settle(&mut self.nodes, node, owner, value - 1);
                    }
                    Contest::Keep => {}
                }
            }
        }
        border_pass(
            &mut self.nodes,
            &self.world,
            building.node,
            u16::from(row.strength),
        );
        self.nodes.reveal(building.node, owner, u16::from(row.sight));
    }

    /// Clears node `node` for a claim (`0x3f1e4`): a road passing through it goes; else a flag
    /// goes with its building; else a building is demolished.
    fn clear_for_claim(&mut self, node: Node) {
        if let Some(road) = self.world.road_through(&self.nodes, node) {
            self.remove_road_record(road);
            return;
        }
        match self.world.object(node) {
            MapObject::Flag(flag) => self.remove_flag_record(flag),
            MapObject::Building(building) => self.demolish(building),
            _ => {}
        }
    }

    /// The military pass (`0x39309`), every 32 steps: from the cursor, up to 40 records, each
    /// finished building's staffing is checked. The cursor moves on, back to 0 once it has
    /// passed the highest record in use.
    pub(crate) fn military_pass(&mut self) {
        let start = self.world.military_cursor;
        let last = u16::try_from(self.world.military.slots().saturating_sub(1)).unwrap_or(u16::MAX);
        let mut at = start;
        while at < start.saturating_add(PASS_SPAN) && at <= last {
            let finished = self
                .world
                .military
                .get(at)
                .and_then(|military| self.world.building(military.building))
                .is_some_and(|building| building.state.contains(BuildingState::FINISHED));
            if finished {
                self.promote_after_wait(at);
                self.staff(at);
            }
            at += 1;
        }
        self.world.military_cursor = if at > last { 0 } else { at };
    }

    /// Military record `military`'s building promotes once its timer has passed 20
    /// (`0x39309`), and the timer starts again whether or not a coin was spent.
    fn promote_after_wait(&mut self, military: u16) {
        let Some(building) = self.world.military.get(military).map(|record| record.building) else {
            return;
        };
        let waited = self
            .world
            .building(building)
            .is_some_and(|site| site.productivity > PROMOTION_WAIT);
        if !waited {
            return;
        }
        self.promote(military);
        if let Some(site) = self.world.building_mut(building) {
            site.productivity = 0;
        }
    }

    /// One promotion round (`0x395de`): with a coin in stock, one soldier of each rank from 3
    /// down to 0 moves up a rank, a soldier moved up never moving again in the same round, and
    /// one coin is spent if anyone was promoted, coming off the owner's ware count as well
    /// (`0x396ae`). Generals are never touched.
    fn promote(&mut self, military: u16) {
        let Some(record) = self.world.military.get(military).copied() else {
            return;
        };
        if self
            .world
            .building(record.building)
            .is_none_or(|site| site.stock[COIN_SLOT] == 0)
        {
            return;
        }
        let mut ranks = record.ranks;
        let mut promoted = false;
        let owner = self.world.building(record.building).map(|site| site.owner);
        for rank in (0..ranks.len() - 1).rev() {
            if ranks[rank] > 0 {
                ranks[rank] -= 1;
                ranks[rank + 1] += 1;
                promoted = true;
                if let Some(owner) = owner {
                    self.world.drop_people(owner, Job::RANKS[rank], 1);
                    self.world.count_people(owner, Job::RANKS[rank + 1], 1);
                }
            }
        }
        if !promoted {
            return;
        }
        if let Some(record) = self.world.military.get_mut(military) {
            record.ranks = ranks;
        }
        if let Some(site) = self.world.building_mut(record.building) {
            site.stock[COIN_SLOT] -= 1;
        }
        if let Some(owner) = owner {
            self.world.drop_ware(owner, crate::ware::WareKind::COINS, 1);
        }
    }

    /// Stops coins coming to military building `record` (`0x30e7e`): every coin order at its
    /// flag not yet served is freed, in order, and its slot counts one fewer on its way.
    pub(crate) fn stop_coins(&mut self, record: u16) {
        let Some(flag) = self.world.building(record).map(|building| building.flag) else {
            return;
        };
        let orders: Vec<u16> = self
            .world
            .orders()
            .filter(|(_, order)| {
                order.flag == flag
                    && order.mode == crate::transport::WARE_MODE
                    && order.kind() == crate::transport::OrderKind::Ware
                    && order.ware() == Some(crate::ware::WareKind::COINS)
            })
            .map(|(order, _)| order)
            .collect();
        for order in orders {
            self.world.release_link(order);
            self.world.orders.remove(order);
            if let Some(site) = self.world.building_mut(record) {
                site.on_way[COIN_SLOT] = site.on_way[COIN_SLOT].saturating_sub(1);
                site.requested = site.requested.saturating_sub(1);
            }
        }
    }

    /// Keeps military record `military` staffed (`0x391a2`): its border class is worked out
    /// again, then soldiers beyond what the owner's occupation setting wants go home, at least
    /// one staying, or more are asked for. Soldiers on their way are never called off.
    pub(crate) fn staff(&mut self, military: u16) {
        let border = self.border_class(military);
        let Some(record) = self.world.military.get_mut(military) else {
            return;
        };
        record.border = border;
        let record = *record;
        let setting = self.world.military_settings[record.owner.index()][OCCUPATION + usize::from(border)];
        let want = (u16::from(setting) * u16::from(record.capacity) / 100).max(1);
        let total = u16::from(record.requested) + u16::from(record.inside);
        if want < u16::from(record.inside) {
            let leaving = (total - want).min(u16::from(record.inside) - 1);
            for _ in 0..leaving {
                if !self.send_soldier_home(military) {
                    break;
                }
            }
        } else if want > total {
            for _ in 0..want - total {
                self.ask_for_soldier(military);
            }
        }
    }

    /// How near the enemy military record `military`'s building stands (`0x38f4c`): 2 within
    /// 18 of another player's military building or headquarters on the same island, 1 within
    /// 24, 0 otherwise, by the straight-line distance between the buildings' map positions.
    fn border_class(&self, military: u16) -> u8 {
        let Some(record) = self.world.military.get(military) else {
            return 0;
        };
        let Some(here) = self.world.building(record.building).map(|building| building.node) else {
            return 0;
        };
        let area = self.nodes.get(here).area;
        let others = self
            .world
            .military
            .iter()
            .filter(|(_, other)| other.owner != record.owner)
            .filter_map(|(_, other)| self.world.building(other.building).map(|building| building.node))
            .chain(
                self.world
                    .buildings()
                    .filter(|(_, building)| {
                        building.kind == BuildingKind::HEADQUARTERS && building.owner != record.owner
                    })
                    .map(|(_, building)| building.node),
            );
        let nearest = others
            .filter(|&there| self.nodes.get(there).area == area)
            .map(|there| straight_distance(here, there))
            .filter(|&distance| distance < NEAR)
            .map(|distance| NEAR - distance)
            .max()
            .unwrap_or(0);
        match nearest {
            0 => 0,
            1..=NEARER => 1,
            _ => 2,
        }
    }

    /// Sends one soldier of military record `military` home (`0x391a2` step 4): the weakest
    /// when the owner's rank setting is 50 or more, the strongest otherwise, if a storehouse by
    /// road takes it. It walks out of the door to the flag. Returns false with no storehouse.
    fn send_soldier_home(&mut self, military: u16) -> bool {
        let Some(record) = self.world.military.get(military).copied() else {
            return false;
        };
        let Some(building) = self.world.building(record.building).copied() else {
            return false;
        };
        let strongest_kept = self.world.military_settings[record.owner.index()][RANK] >= STRONGEST_FROM;
        let job = if strongest_kept {
            record.weakest()
        } else {
            record.strongest()
        };
        let Some(job) = job else {
            return false;
        };
        if self.home_flag(building.flag, job).is_none() {
            return false;
        }
        let Some(flag) = self.world.flag(building.flag).map(|flag| flag.node) else {
            return false;
        };
        let mut figure = crate::figures::Figure::fresh(FigureKind::Porter(Porter {
            owner: record.owner,
            job,
            building: record.building,
            ware: None,
            inside: false,
        }));
        figure.start_walk(Direction::SouthEast, DOOR_SPEED);
        self.world.add_figure(flag, figure);
        if let Some(site) = self.world.building_mut(record.building) {
            site.state |= BuildingState::DOOR;
            site.state -= BuildingState::PRESENT;
        }
        if let Some(record) = self.world.military.get_mut(military)
            && let Some(rank) = rank_of(job)
        {
            record.ranks[rank] = record.ranks[rank].saturating_sub(1);
            record.inside = record.inside.saturating_sub(1);
        }
        true
    }
}

impl Game {
    /// Military record `military` holds `capacity` soldiers from now on. Those inside beyond it
    /// leave, one always staying: each walks home as [`Self::staff`] sends one, and with no
    /// storehouse by road is gone, weakest first. Returns how many left.
    pub(crate) fn cap_soldiers(&mut self, military: u16, capacity: u8) -> u8 {
        let Some(record) = self.world.military.get_mut(military) else {
            return 0;
        };
        record.capacity = capacity;
        let over = record.inside.saturating_sub(capacity.max(1));
        for _ in 0..over {
            if self.send_soldier_home(military) {
                continue;
            }
            let Some(record) = self.world.military.get_mut(military) else {
                break;
            };
            if let Some(rank) = record.weakest().and_then(rank_of) {
                record.ranks[rank] = record.ranks[rank].saturating_sub(1);
                record.inside = record.inside.saturating_sub(1);
            }
        }
        over
    }

    /// Soldier `record` sent out of a military building is at its flag `flag` (`0x3648c`): it
    /// walks home by road to the nearest storehouse that takes its rank, or is a lost soldier
    /// with none.
    pub(crate) fn soldier_leaves(&mut self, record: u16, owner: Player, job: Job, flag: u16) {
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Walker(crate::figures::Walker {
                owner,
                job,
                order: flag,
                mode: crate::settlers::HOME_MODE,
                on: crate::figures::OnRoad {
                    road: 0,
                    position: 0,
                    to_start: false,
                },
            });
        }
        if !self.send_home(record, flag) {
            self.make_lost(record);
        }
    }
}

/// What a release noted on land it took back, to remove if the land is now someone else's.
#[derive(Default)]
pub(crate) struct Noted {
    pub(crate) roads: Vec<u16>,
    pub(crate) flags: Vec<u16>,
    pub(crate) buildings: Vec<u16>,
}

impl Game {
    /// Releases the land a headquarters, occupied military building or harbour `record` held as
    /// it is demolished (`0x3e838`): its protection goes, its owner's unprotected land within
    /// one ring past its reach goes back to nobody, and the occupied military buildings and
    /// headquarters of every player near it spread again, oldest occupation first, taking
    /// nothing down. Then the border pass, and the flags, buildings and roads noted on the freed
    /// land go where the land is no longer the owner's or lies on a border.
    pub(crate) fn release_land(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let Some(strength) = self
            .world
            .building_types()
            .get(building.kind)
            .map(|row| row.strength)
        else {
            return;
        };
        let owner = self
            .world
            .flag(building.flag)
            .map_or(building.owner, |flag| flag.owner);
        let size = (self.nodes.width(), self.nodes.height());
        for node in crate::territory::protected_nodes(size, building.node) {
            let mut data = self.nodes.get(node);
            data.protected = false;
            self.nodes.set(node, data);
        }
        let mut data = self.nodes.get(building.node);
        data.strength = 0;
        data.owner = None;
        self.nodes.set(building.node, data);
        let mut noted = Noted::default();
        let (width, height) = size;
        let rings =
            (1..=u16::from(strength) + 1).flat_map(|ring| building.node.stepped_ring(ring, width, height));
        for node in rings {
            let mut data = self.nodes.get(node);
            if data.protected || data.owner != Some(owner) {
                continue;
            }
            data.strength = 0;
            data.owner = None;
            self.nodes.set(node, data);
            self.note(node, &mut noted);
        }
        let mut holders: Vec<u16> = claim_order(size, building.node, strength + RECLAIM_REACH)
            .into_iter()
            .skip(1)
            .filter_map(|node| self.world.object(node).building())
            .filter(|&other| other != record && self.holds_land(other))
            .collect();
        holders.sort_by_key(|&other| self.world.building(other).map_or(u16::MAX, |b| b.occupied));
        for holder in holders {
            self.spread_again(holder);
        }
        border_pass(
            &mut self.nodes,
            &self.world,
            building.node,
            u16::from(strength) + RELEASE_BORDER,
        );
        let lost = |game: &Self, node: Node| {
            let data = game.nodes.get(node);
            data.owner != Some(owner) || data.flags.contains(crate::nodes::NodeFlags::BORDER)
        };
        for flag in noted.flags {
            if let Some(node) = self.world.flag(flag).map(|flag| flag.node)
                && lost(self, node)
            {
                self.remove_flag_record(flag);
            }
        }
        for other in noted.buildings {
            if let Some(node) = self.world.building(other).map(|building| building.node)
                && lost(self, node)
            {
                self.demolish(other);
            }
        }
        for road in noted.roads {
            let doomed = self
                .world
                .road(road)
                .is_some_and(|road| road.path.iter().any(|&node| lost(self, node)));
            if doomed {
                self.remove_road_record(road);
            }
        }
    }

    /// Notes what stands on freed node `node` (`0x3e838` step 3): a road through it, once, else
    /// a flag, else a building that holds no land itself.
    pub(crate) fn note(&self, node: Node, noted: &mut Noted) {
        if let Some(road) = self.world.road_through(&self.nodes, node) {
            if !noted.roads.contains(&road) {
                noted.roads.push(road);
            }
            return;
        }
        match self.world.object(node) {
            MapObject::Flag(flag) => noted.flags.push(flag),
            MapObject::Building(building) if !self.holds_land(building) => noted.buildings.push(building),
            _ => {}
        }
    }

    /// Whether building `record` holds land: a headquarters, or an occupied military building.
    pub(crate) fn holds_land(&self, record: u16) -> bool {
        self.world.building(record).is_some_and(|building| {
            building.kind == BuildingKind::HEADQUARTERS
                || (building.state.contains(BuildingState::MANNED)
                    && self
                        .world
                        .building_types()
                        .get(building.kind)
                        .is_some_and(|row| row.category == crate::world::Category::Military))
        })
    }

    /// Building `record` spreads its land again round itself after a release (`0x3e838` step
    /// 6): from rings 1 to one less than its strength, taking down nothing.
    fn spread_again(&mut self, record: u16) {
        let strength = self
            .world
            .building(record)
            .and_then(|building| self.world.building_types().get(building.kind))
            .map_or(0, |row| row.strength);
        if strength > 1 {
            self.spread_rings(record, strength - 1);
        }
    }

    /// Building `record` spreads its owner's land from rings 1 to `last` round itself, nodes
    /// past a map edge left out, taking down nothing (`0x3e358` with destroy off).
    pub(crate) fn spread_rings(&mut self, record: u16, last: u8) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let Some(owner) = self.world.building_owner(record) else {
            return;
        };
        let size = (self.nodes.width(), self.nodes.height());
        for from in claim_order(size, building.node, last).into_iter().skip(1) {
            let Some(value) = spreads_from(&self.nodes, from, owner) else {
                continue;
            };
            for node in neighbours_on_map(size, from, SPREAD_ORDER) {
                if contest(&self.nodes, node, owner, value - 1) != Contest::Keep {
                    settle(&mut self.nodes, node, owner, value - 1);
                }
            }
        }
    }
}

/// How far past a released building's strength its neighbours' buildings are looked for.
pub(crate) const RECLAIM_REACH: u8 = 14;

/// How far past a released building's strength the border pass after a release reaches.
pub(crate) const RELEASE_BORDER: u16 = 15;

/// `⌊√(dx² + dy²)⌋` between the map positions of two nodes, with no wrapping and no allowance
/// for the row offset (`0x227b0`).
pub(crate) fn straight_distance(a: Node, b: Node) -> u32 {
    let dx = u32::from(a.x.abs_diff(b.x));
    let dy = u32::from(a.y.abs_diff(b.y));
    (dx * dx + dy * dy).isqrt()
}

#[cfg(test)]
mod tests;
