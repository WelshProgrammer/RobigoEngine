//! Attacks, defence and fights: the soldier record, sending attackers, defenders called out or
//! sent from nearby buildings, the fights between them, and capturing a building.
//!
//! Not yet modelled: the per-player job statistics.

mod capture;
mod fight;

use crate::figures::{Entry, Figure, FigureKind, Soldier, SoldierState};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::homeward::{Crossing, HOME_SPEED, Hold, Path};
use crate::job::Job;
use crate::military::{ATTACK, DEFENCE, RANK, STRONGEST_FROM, straight_distance};
use crate::players::Controller;
use crate::world::{BuildingKind, BuildingState, Category, Player};

pub use fight::{FightAnimations, FightAnimationsError, FighterAnimations};

/// Most soldier records.
const MAX_SOLDIERS: usize = 65_534;

/// What a soldier is doing outside a building (`+0x00` bits 10–12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Role {
    /// Its role was cleared by a fatal hit.
    None,
    /// Attacking (1).
    Attacker,
    /// Sent out to defend (2).
    Defender,
    /// Going home (3).
    Home,
}

/// Where a soldier is in a fight (`+0x00` bits 0–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FightStep {
    /// Not fighting (0).
    None,
    /// Paired, waiting for the opponent to come (1).
    Waiting,
    /// Paired and at the opponent's node (2).
    Facing,
    /// Walking up to the opponent (3).
    Approaching,
    /// Paired, about to walk up (4).
    WillApproach,
    /// Its death animation is playing (5).
    Dying,
    /// Took the fatal hit while waiting (6).
    FellWaiting,
    /// Took the fatal hit after walking up (7).
    FellFacing,
}

/// A soldier record (`0x1fa532`, 12 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SoldierRecord {
    /// Its role.
    pub role: Role,
    /// Its fight step.
    pub step: FightStep,
    /// Hit points left (`+0x02`): rank + 3 to start with, never restored.
    pub hit_points: u8,
    /// The soldier's figure (`+0x03`).
    pub figure: u16,
    /// The opponent's figure (`+0x05`).
    pub opponent: Option<u16>,
    /// Rounds fought in the current fight (`+0x07`).
    pub rounds: u8,
    /// An attacker's target building, or the attacker figure a defender was sent for (`+0x08`).
    pub target: Option<u16>,
    /// Whether an attacker has asked for defenders (`+0x0a`).
    pub asked: bool,
}

/// A soldier's rank, 0 for a private, from its job.
fn rank(job: Job) -> usize {
    Job::RANKS.iter().position(|&rank| rank == job).unwrap_or(0)
}

/// How near its target's flag an attacker asks for defenders (`0x3afdb`).
const ASK_WITHIN: u32 = 10;

/// The rings round a target the attackers and the nearby defenders come from.
const DISPATCH_RINGS: std::ops::RangeInclusive<u16> = 2..=47;
const DEFENDER_RINGS: std::ops::RangeInclusive<u16> = 2..=11;

/// The attack window counts buildings up to this far away, at a full share up to 13 away.
const COUNT_REACH: u32 = 42;
const COUNT_FALLOFF: u32 = 55;

/// The rings round a target searched for a border edge (`0x38ee1`).
const BORDER_SEARCH: u16 = 19;

/// How far an attacker or a soldier going home looks for an opponent before each edge.
const OPPONENT_REACH: u16 = 1;

/// How far a defender that lost its attacker looks for another.
const CHASE_REACH: u16 = 8;

/// How much an attacker or a soldier going home sees round itself (`0x3a7de`).
const SOLDIER_SIGHT: u16 = 4;

/// The draw a chasing defender with no free direction makes: 0 to 5 is a direction, 6 none.
const CHASE_ROLL: u16 = 6;

/// How a new game's alliances are set (`0x1f7a3c`, the setting the free-play screen cycles with
/// `0x5e7f7`): `S2.EXE` applies it once as the game starts (`0x17070`), and nothing in the game
/// makes an alliance after that. A campaign mission starts with none and its script can ally
/// one player to another (`0x24e57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alliances {
    /// Nobody allied (setting 0).
    #[default]
    None,
    /// The players people play allied with each other and the players the computer plays allied
    /// with each other (setting 1), by each slot's [`Controller`]. A player not in the game
    /// counts as the computer's, as its record says. A beaten player is allied with nobody,
    /// which never happens as a game starts.
    Controllers,
    /// Players of the same nation allied (setting 2), which is why a person cannot end up allied
    /// with another nation's computer.
    Nations,
    /// Every player but the first allied with every other, itself included, so they play
    /// together against the first: the World Campaign's start (`0x5efcd`). It goes by slot
    /// number, whoever plays each slot, and the first player is not even its own ally.
    AgainstFirst,
}

impl crate::world::World {
    /// Whether `player` counts `other` as an ally (`0x1f79eb`). Every setting but
    /// [`Alliances::None`] marks a player its own ally, bar the first player under
    /// [`Alliances::AgainstFirst`]; the game's own tests compare the two owners before they ask.
    #[must_use]
    pub fn allied(&self, player: Player, other: Player) -> bool {
        self.allies[player.index()][other.index()]
    }

    /// Sets every pair of players from `alliances` (`0x17070`), which `S2.EXE` does as the game
    /// starts, after the setup screen has given each player its nation. Only the rows of players
    /// in the game are kept: the pass clears the rest afterwards (`0x171af`).
    pub(crate) fn set_alliances(&mut self, alliances: Alliances) {
        let side = |player: Player| match self.slots.get(player).map(|slot| slot.controller) {
            Some(Controller::Beaten) => None,
            Some(controller) => Some(controller.is_person()),
            None => Some(false),
        };
        for player in Player::ALL {
            for other in Player::ALL {
                self.allies[player.index()][other.index()] = self.slots.in_game(player)
                    && match alliances {
                        Alliances::None => false,
                        Alliances::Controllers => side(player).is_some() && side(player) == side(other),
                        Alliances::Nations => self.nations[player.index()] == self.nations[other.index()],
                        Alliances::AgainstFirst => player.index() > 0 && other.index() > 0,
                    };
            }
        }
    }

    /// Breaks the alliance between `player` and `other`, and between `player` and every ally of
    /// `other` (`0x16fe5`), as starting an attack does.
    pub(crate) fn break_alliances(&mut self, player: Player, other: Player) {
        self.allies[player.index()][other.index()] = false;
        self.allies[other.index()][player.index()] = false;
        for third in 0..Player::COUNT {
            if self.allies[other.index()][third] {
                self.allies[player.index()][third] = false;
                self.allies[third][player.index()] = false;
            }
        }
    }

    /// Soldier record `record`.
    #[must_use]
    pub fn soldier(&self, record: u16) -> Option<&SoldierRecord> {
        self.soldiers.get(record)
    }

    /// Every soldier record in use, in record order.
    pub fn soldier_records(&self) -> impl Iterator<Item = (u16, &SoldierRecord)> {
        self.soldiers.iter()
    }

    /// Soldier figure `figure`'s record number and a copy of it: the test for a fighting soldier
    /// (`0x32918`).
    pub(crate) fn soldier_of(&self, figure: u16) -> Option<(u16, SoldierRecord)> {
        let FigureKind::Soldier(soldier) = self.figure(figure)?.kind else {
            return None;
        };
        let record = soldier.record?;
        Some((record, *self.soldiers.get(record)?))
    }

    /// The owner of building `record`: its flag's owner, which a capture changes.
    pub(crate) fn building_owner(&self, record: u16) -> Option<Player> {
        let building = self.building(record)?;
        Some(self.flag(building.flag).map_or(building.owner, |flag| flag.owner))
    }
}

impl Game {
    /// How many soldiers `player` can send against building `target`, as the attack window
    /// counts them when it opens (`0x3946d`). Nothing unless a border edge lies within the
    /// target type's claim radius. Each of the player's occupied military buildings in the
    /// target's area and within 42 offers all but one soldier times the attack share, less for
    /// those further than 13.
    #[must_use]
    pub fn attack_count(&self, player: Player, target: u16) -> u8 {
        let Some(building) = self.world.building(target) else {
            return 0;
        };
        let strength = self
            .world
            .building_types()
            .get(building.kind)
            .map_or(0, |row| u16::from(row.strength));
        if self.border_rings(building.node) > strength {
            return 0;
        }
        let share = u32::from(self.world.military_settings(player)[ATTACK]);
        let area = self.nodes.get(building.node).area;
        let mut total = 0_u32;
        for (_, military) in self.world.military() {
            let Some(home) = self.world.building(military.building) else {
                continue;
            };
            if self.world.building_owner(military.building) != Some(player)
                || self.nodes.get(home.node).area != area
            {
                continue;
            }
            let distance = straight_distance(home.node, building.node);
            if distance > COUNT_REACH || military.inside < 2 {
                continue;
            }
            let full = if distance.is_multiple_of(256) {
                COUNT_REACH
            } else {
                (COUNT_FALLOFF - distance).min(COUNT_REACH)
            };
            total += (u32::from(military.inside) - 1) * share * full / COUNT_REACH / 100;
        }
        total.to_le_bytes()[0]
    }

    /// The first ring from 1 to 19 round `node` holding a node with a border edge, or 20
    /// (`0x38ee1`).
    fn border_rings(&self, node: Node) -> u16 {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        (1..=BORDER_SEARCH)
            .find(|&ring| {
                node.ring(ring, width, height)
                    .any(|at| !self.nodes.get(at).borders.is_empty())
            })
            .unwrap_or(BORDER_SEARCH + 1)
    }

    /// Sets the alliances `alliances` describes, as the start of a game does (`0x17070`). Call
    /// it after the players have their nations, which [`Alliances::Nations`] reads.
    pub fn set_alliances(&mut self, alliances: Alliances) {
        self.world.set_alliances(alliances);
    }

    /// Sends up to `wanted` of `player`'s soldiers against building `target` (`0x396fa`): the
    /// alliance with the target's owner, its flag's, and with that owner's allies is broken, then
    /// from each
    /// occupied military building of the player's met on a walk round rings 2 to 47 of it, its
    /// share of all but one soldier, strongest or weakest first. Returns how many set off. The
    /// walk keeps its place in `S2.EXE`'s shared position, which each attacker's path search
    /// moves, so after a building sends one it goes on from where the search left it, mostly
    /// the target's flag, and not from where it was.
    pub(crate) fn launch_attack(&mut self, player: Player, target: u16, wanted: u8, strongest: bool) -> u8 {
        let Some(building) = self.world.building(target).copied() else {
            return 0;
        };
        if let Some(owner) = self.world.building_owner(target) {
            self.world.break_alliances(player, owner);
        }
        let Some(flag) = self.world.flag(building.flag).map(|flag| flag.node) else {
            return 0;
        };
        let share = u16::from(self.world.military_settings(player)[ATTACK]);
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let mut sent = 0;
        let mut node = building.node.neighbour(Direction::NorthWest, width, height);
        for ring in DISPATCH_RINGS {
            node = node.neighbour(Direction::NorthWest, width, height);
            for side in Direction::ALL {
                for _ in 0..ring {
                    node = node.neighbour(side, width, height);
                    if self.nodes.get(node).owner != Some(player) {
                        continue;
                    }
                    let Some(home) = self.world.object(node).building() else {
                        continue;
                    };
                    let Some((military, record)) = self.world.military_of(home).map(|(m, r)| (m, *r)) else {
                        continue;
                    };
                    let occupied = self
                        .world
                        .building(home)
                        .is_some_and(|home| home.state.contains(BuildingState::MANNED));
                    if !occupied || record.inside < 2 {
                        continue;
                    }
                    let mut available = (u16::from(record.inside) - 1) * share / 100;
                    while available > 0 {
                        let Some(held) = self.world.military.get(military).copied() else {
                            break;
                        };
                        let Some(job) = (if strongest {
                            held.strongest()
                        } else {
                            held.weakest()
                        }) else {
                            break;
                        };
                        let (left, cursor) = self.send_attacker(military, home, job, target, flag);
                        if let Some(cursor) = cursor {
                            node = cursor;
                        }
                        if !left {
                            break;
                        }
                        sent += 1;
                        if sent == wanted {
                            return sent;
                        }
                        available -= 1;
                    }
                }
            }
        }
        sent
    }

    /// An attacker of rank `job` leaves building `home` for `target`, whose flag stands on
    /// `flag` (`0x334c4`, `0x38ebd`, `0x39bb2`): a path from its own flag, the figure walking out
    /// of the door or waiting in it while another figure walks out, the building counting it
    /// as on its way back, and its record. Returns whether it left, and where its path search
    /// left the shared position.
    fn send_attacker(
        &mut self,
        military: u16,
        home: u16,
        job: Job,
        target: u16,
        flag: Node,
    ) -> (bool, Option<Node>) {
        let Some(building) = self.world.building(home).copied() else {
            return (false, None);
        };
        let Some(from) = self.world.flag(building.flag).map(|flag| flag.node) else {
            return (false, None);
        };
        let (path, cursor) = self.find_path_leaving(from, flag, Crossing::Walkable, true);
        let Some(path) = path else {
            return (false, cursor);
        };
        let owner = self.world.building_owner(home).unwrap_or(building.owner);
        let waiting = self.door_busy(from);
        let mut figure = Figure::fresh(FigureKind::Soldier(Soldier {
            owner,
            job,
            record: None,
            home: Some(home),
            held: false,
            state: SoldierState::Leaving {
                door: home,
                path: Some(path),
                waiting,
            },
        }));
        if !waiting {
            figure.start_walk(Direction::SouthEast, HOME_SPEED);
        }
        let figure = self.world.add_figure(from, figure);
        if let Some(site) = self.world.building_mut(home) {
            site.state |= BuildingState::DOOR;
        }
        self.leave_military(military, job);
        self.give_record(figure, job, Role::Attacker, Some(target));
        (true, cursor)
    }

    /// A soldier of rank `job` leaves military record `military`, which counts it as on its way
    /// back (`0x38ebd`).
    fn leave_military(&mut self, military: u16, job: Job) {
        if let Some(held) = self.world.military.get_mut(military) {
            let count = &mut held.ranks[rank(job)];
            *count = count.saturating_sub(1);
            held.inside = held.inside.saturating_sub(1);
            held.requested = held.requested.saturating_add(1);
        }
    }

    /// Whether a figure is walking through a door onto flag node `node` (`0x36a90`).
    fn door_busy(&self, node: Node) -> bool {
        self.world.figures_at(node).any(|(_, figure)| figure.in_door())
    }

    /// Gives soldier figure `figure` of rank `job` a new record (`0x39bb2`, `0x39b36`).
    fn give_record(&mut self, figure: u16, job: Job, role: Role, target: Option<u16>) {
        let record = SoldierRecord {
            role,
            step: FightStep::None,
            hit_points: u8::try_from(rank(job)).unwrap_or(0) + 3,
            figure,
            opponent: None,
            rounds: 0,
            target,
            asked: false,
        };
        let made = self.world.soldiers.insert(record, MAX_SOLDIERS);
        if let Some(FigureKind::Soldier(soldier)) = self.figure_kind_mut(figure) {
            soldier.record = made;
        }
    }

    /// Frees soldier figure `figure`'s record (`0x39c34`).
    pub(crate) fn free_record(&mut self, figure: u16) {
        if let Some((record, _)) = self.world.soldier_of(figure) {
            self.world.soldiers.remove(record);
        }
        if let Some(FigureKind::Soldier(soldier)) = self.figure_kind_mut(figure) {
            soldier.record = None;
        }
    }

    /// Changes soldier figure `figure`'s record, if it has one.
    fn update_record(&mut self, figure: u16, change: impl FnOnce(&mut SoldierRecord)) {
        if let Some((record, _)) = self.world.soldier_of(figure)
            && let Some(held) = self.world.soldiers.get_mut(record)
        {
            change(held);
        }
    }

    /// Changes soldier figure `figure`'s state.
    fn set_soldier_state(&mut self, figure: u16, state: SoldierState) {
        if let Some(FigureKind::Soldier(soldier)) = self.figure_kind_mut(figure) {
            soldier.state = state;
        }
    }

    /// One step of soldier figure `record` that is not walking.
    pub(crate) fn step_soldier(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return;
        };
        match soldier.state {
            SoldierState::Leaving { door, path, waiting } => {
                if !waiting {
                    self.soldier_out(record, door, path);
                }
            }
            SoldierState::Entering { door, entry } => self.soldier_in(record, soldier, door, entry),
            SoldierState::Walking { .. } => self.soldier_walks(record),
            SoldierState::Waiting => {}
            SoldierState::Animating { animation } => {
                if animation.is_some_and(|animation| self.run_animation_at(animation, figure.node())) {
                    return;
                }
                self.set_soldier_state(record, SoldierState::Animating { animation: None });
                self.animation_ended(record);
            }
        }
    }

    /// The first soldier waiting in a door onto flag node `node`, in list order, walks out
    /// (`0x36a10`).
    pub(crate) fn release_door(&mut self, node: Node) {
        let waiting = self.world.figures_at(node).find_map(|(record, figure)| {
            matches!(
                figure.kind,
                FigureKind::Soldier(Soldier {
                    state: SoldierState::Leaving { waiting: true, .. },
                    ..
                })
            )
            .then_some(record)
        });
        let Some(record) = waiting else {
            return;
        };
        if let Some(FigureKind::Soldier(Soldier {
            state: SoldierState::Leaving { waiting, .. },
            ..
        })) = self.figure_kind_mut(record)
        {
            *waiting = false;
        }
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::SouthEast, HOME_SPEED);
        }
    }

    /// Soldier figure `record` has walked out of building `door` to its flag (`0x368d5`,
    /// `0x3617f`). An attacker sets off along its path, or with its target gone walks back in.
    /// With the bug fixes off it then disappears and its home keeps asking for it; with them on
    /// it joins its home again. A defender goes for its attacker, or walks back in when it has
    /// none.
    fn soldier_out(&mut self, record: u16, door: u16, path: Option<u16>) {
        let Some((_, held)) = self.world.soldier_of(record) else {
            if let Some(path) = path {
                self.world.paths.remove(path);
            }
            self.walk_in(record, door, Entry::Vanish);
            return;
        };
        if held.target.is_none() {
            if let Some(path) = path {
                self.world.paths.remove(path);
            }
            if held.role == Role::Defender {
                self.walk_in(record, door, Entry::Defender);
            } else if self.world.bug_fixes.on() {
                self.walk_in(record, door, Entry::Home);
            } else {
                self.free_record(record);
                self.walk_in(record, door, Entry::Vanish);
            }
            return;
        }
        self.set_soldier_state(record, SoldierState::Walking { path });
        if held.role == Role::Defender
            && let Some(attacker) = held.target
        {
            self.defender_out(record, attacker);
        }
    }

    /// Soldier figure `record` turns in from its flag at the door of `building` and walks in.
    fn walk_in(&mut self, record: u16, building: u16, entry: Entry) {
        let Some(node) = self.world.building(building).map(|building| building.node) else {
            self.make_lost(record);
            return;
        };
        self.set_soldier_state(
            record,
            SoldierState::Entering {
                door: building,
                entry,
            },
        );
        self.world.move_figure(record, node);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, HOME_SPEED);
        }
    }

    /// The cross-country step of soldier figure `record` (`0x3473c`). The fight comes first;
    /// then an attacker or a soldier going home looks round itself, and an attacker near its
    /// target asks for defenders, while a defender chases. Otherwise it walks its path.
    fn soldier_walks(&mut self, record: u16) {
        if self.world.soldier_of(record).is_some() {
            if self.fight_step(record) {
                return;
            }
            let Some((_, held)) = self.world.soldier_of(record) else {
                return;
            };
            if held.opponent.is_none() {
                match held.role {
                    Role::Attacker | Role::Home => {
                        if self.look_round(record) {
                            return;
                        }
                        if held.role == Role::Attacker && !held.asked {
                            self.ask_for_defenders(record);
                        }
                    }
                    _ => {
                        self.chase(record);
                        return;
                    }
                }
            }
        }
        self.walk_soldier_path(record);
    }

    /// Soldier figure `record` looks round itself before an edge (`0x3a7de`): it sees round its
    /// node, and pairs with an enemy soldier within one step. Returns whether it paired.
    fn look_round(&mut self, record: u16) -> bool {
        let Some(figure) = self.world.figure(record).copied() else {
            return false;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return false;
        };
        if !self.nodes.fully_seen(figure.node(), soldier.owner) {
            self.nodes.reveal(figure.node(), soldier.owner, SOLDIER_SIGHT);
        }
        let Some(found) = self.find_opponent(figure.node(), soldier.owner, OPPONENT_REACH) else {
            return false;
        };
        self.drop_path(record);
        self.pair(record, found);
        true
    }

    /// Frees soldier figure `record`'s cross-country path, if it is walking one.
    fn drop_path(&mut self, record: u16) {
        let path = match self.figure_kind_mut(record) {
            Some(FigureKind::Soldier(Soldier {
                state: SoldierState::Walking { path, .. },
                ..
            })) => path.take(),
            _ => None,
        };
        if let Some(path) = path {
            self.world.paths.remove(path);
        }
    }

    /// The first enemy soldier free to fight on `node` or within `reach` rings of it
    /// (`0x3a0e0`, `0x3a32d`). An ally's soldier is passed over. One in a door counts on the
    /// rings but not on the node itself.
    fn find_opponent(&self, node: Node, owner: Player, reach: u16) -> Option<u16> {
        let free = |record: u16, figure: &Figure, doors: bool| match figure.kind {
            FigureKind::Soldier(soldier) => {
                soldier.owner != owner
                    && !self.world.allied(owner, soldier.owner)
                    && (doors || !figure.in_door())
                    && self
                        .world
                        .soldier_of(record)
                        .is_some_and(|(_, held)| held.opponent.is_none())
            }
            _ => false,
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        self.world
            .figures_at(node)
            .find(|&(record, figure)| free(record, figure, false))
            .or_else(|| {
                (1..=reach)
                    .flat_map(|ring| node.stepped_ring(ring, width, height))
                    .find_map(|at| {
                        self.world
                            .figures_at(at)
                            .find(|&(record, figure)| free(record, figure, true))
                    })
            })
            .map(|(record, _)| record)
    }

    /// An attacker within 10 of its target's flag asks once for defenders (`0x3afdb`): from the
    /// target first, then from the first building of the target's owner on rings 2 to 11 that
    /// sends one.
    fn ask_for_defenders(&mut self, record: u16) {
        let Some((_, held)) = self.world.soldier_of(record) else {
            return;
        };
        let Some(target) = held.target else {
            self.update_record(record, |held| held.asked = true);
            return;
        };
        let Some(building) = self.world.building(target).copied() else {
            return;
        };
        let (Some(here), Some(flag)) = (
            self.world.figure(record).map(Figure::node),
            self.world.flag(building.flag).map(|flag| flag.node),
        ) else {
            return;
        };
        if self.wrapped_distance(here, flag) >= ASK_WITHIN {
            return;
        }
        self.update_record(record, |held| held.asked = true);
        if self.send_defender(target, record) {
            return;
        }
        let owner = self.world.building_owner(target);
        let (width, height) = (self.nodes.width(), self.nodes.height());
        for ring in DEFENDER_RINGS {
            for node in building.node.stepped_ring(ring, width, height) {
                if self.nodes.get(node).owner != owner {
                    continue;
                }
                if let Some(other) = self.world.object(node).building()
                    && self.send_defender(other, record)
                {
                    return;
                }
            }
        }
    }

    /// Whether building `building` defends: a headquarters, or a finished or occupied military
    /// building (`0x30ce5`).
    fn defends(&self, building: u16) -> bool {
        self.world.building(building).is_some_and(|site| {
            site.kind == BuildingKind::HEADQUARTERS
                || (self
                    .world
                    .building_types()
                    .get(site.kind)
                    .is_some_and(|row| row.category == Category::Military)
                    && site
                        .state
                        .intersects(BuildingState::FINISHED | BuildingState::MANNED))
        })
    }

    /// Building `building` may send a defender against attacker figure `attacker` (`0x3af36`).
    /// A military building needs two soldiers inside, and the owner's defence chance decides.
    /// Returns whether one came out.
    fn send_defender(&mut self, building: u16, attacker: u16) -> bool {
        if !self.defends(building) {
            return false;
        }
        if self
            .world
            .military_of(building)
            .is_some_and(|(_, held)| held.inside < 2)
        {
            return false;
        }
        let Some(owner) = self.world.building_owner(building) else {
            return false;
        };
        let Some(job) = self.choose_defender(building, owner, true) else {
            return false;
        };
        self.make_defender(owner, job, building, attacker).is_some()
    }

    /// The soldier building `building` sends out (`0x3ae03`). With `chance`, a draw from 0 to
    /// 100 above the owner's defence setting sends nobody. A military building gives its
    /// strongest or weakest by the rank setting, and counts it as on its way back; a
    /// headquarters the same from its stock, or with none in stock and no chance drawn, the
    /// lowest rank in its reserve whatever the rank setting.
    fn choose_defender(&mut self, building: u16, owner: Player, chance: bool) -> Option<Job> {
        let settings = self.world.military_settings(owner);
        if chance && self.rng.up_to(100) > u16::from(settings[DEFENCE]) {
            return None;
        }
        let strongest = settings[RANK] >= STRONGEST_FROM;
        if let Some((military, held)) = self.world.military_of(building).map(|(m, held)| (m, *held)) {
            let job = if strongest {
                held.strongest()
            } else {
                held.weakest()
            }?;
            self.leave_military(military, job);
            return Some(job);
        }
        let (store, stock) = self.world.storehouse_of(building)?;
        let mut present = Job::RANKS
            .iter()
            .copied()
            .filter(|&job| stock.jobs[job.index()] > 0);
        let job = if strongest {
            present.next_back()
        } else {
            present.next()
        };
        let Some(job) = job else {
            if chance {
                return None;
            }
            let job = self.world.reserve_rank(building)?;
            self.world.take_reserve(building, job);
            return Some(job);
        };
        if let Some(stock) = self.world.storehouse_mut(store) {
            stock.jobs[job.index()] -= 1;
        }
        Some(job)
    }

    /// A defender of rank `job` walks out of `building` for attacker figure `attacker`
    /// (`0x36dd4`). Returns its figure.
    fn make_defender(&mut self, owner: Player, job: Job, building: u16, attacker: u16) -> Option<u16> {
        let site = self.world.building(building).copied()?;
        let flag = self.world.flag(site.flag)?.node;
        let mut figure = Figure::new(FigureKind::Soldier(Soldier {
            owner,
            job,
            record: None,
            home: Some(building),
            held: false,
            state: SoldierState::Leaving {
                door: building,
                path: None,
                waiting: false,
            },
        }));
        figure.start_walk(Direction::SouthEast, HOME_SPEED);
        let record = self.world.add_figure(flag, figure);
        if let Some(site) = self.world.building_mut(building) {
            site.state |= BuildingState::DOOR;
        }
        self.give_record(record, job, Role::Defender, Some(attacker));
        Some(record)
    }

    /// A defender at its flag (`0x3617f`): with its attacker on the flag the fight at the door
    /// starts at once, the defender waiting for it; otherwise it takes a first step towards it.
    fn defender_out(&mut self, record: u16, attacker: u16) {
        let Some(here) = self.world.figure(record).map(Figure::node) else {
            return;
        };
        let Some(there) = self.world.figure(attacker).map(Figure::node) else {
            return;
        };
        if here == there {
            self.update_record(record, |held| {
                held.step = FightStep::Waiting;
                held.opponent.get_or_insert(attacker);
            });
            self.round(record, attacker);
            return;
        }
        self.step_towards(record, there);
    }

    /// The distance `|dx| + |dy|` between two nodes, each the shorter way round the map
    /// (`0x22720`).
    fn wrapped_distance(&self, a: Node, b: Node) -> u32 {
        let (width, height) = (u32::from(self.nodes.width()), u32::from(self.nodes.height()));
        let dx = u32::from(a.x.abs_diff(b.x));
        let dy = u32::from(a.y.abs_diff(b.y));
        dx.min(width - dx) + dy.min(height - dy)
    }

    /// A defender with no opponent chases (`0x3a5c2`). It keeps its attacker while that one is
    /// free or paired with it; otherwise it pairs with an enemy within one step, or takes the
    /// first within eight as its new target, or goes home. Next to its target it pairs, else
    /// it steps towards it.
    fn chase(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return;
        };
        let Some((_, held)) = self.world.soldier_of(record) else {
            return;
        };
        let valid = held.target.filter(|&target| {
            self.world
                .soldier_of(target)
                .is_some_and(|(_, other)| other.opponent.is_none_or(|opponent| opponent == record))
        });
        let target = if let Some(target) = valid {
            target
        } else {
            if let Some(found) = self.find_opponent(figure.node(), soldier.owner, OPPONENT_REACH) {
                self.update_record(record, |held| held.target = Some(found));
                self.pair(record, found);
                return;
            }
            let Some(found) = self.find_opponent(figure.node(), soldier.owner, CHASE_REACH) else {
                self.update_record(record, |held| held.target = None);
                self.go_home(record);
                return;
            };
            self.update_record(record, |held| held.target = Some(found));
            found
        };
        let Some(there) = self.world.figure(target).map(Figure::node) else {
            return;
        };
        if self.wrapped_distance(figure.node(), there) < 2 {
            self.pair(record, target);
            return;
        }
        self.step_towards(record, there);
    }

    /// Soldier figure `record` steps one node towards `there` (`0x3a4dd`): the way
    /// [`Self::chase_direction`] picks, or a draw from 0 to 6 when there is none, where 6 leaves
    /// it walking on the spot.
    fn step_towards(&mut self, record: u16, there: Node) {
        let Some(here) = self.world.figure(record).map(Figure::node) else {
            return;
        };
        let direction = match self.chase_direction(here, there) {
            Some(direction) => Some(direction),
            None => Direction::ALL
                .get(usize::from(self.rng.up_to(CHASE_ROLL)))
                .copied(),
        };
        if let Some(direction) = direction {
            let next = here.neighbour(direction, self.nodes.width(), self.nodes.height());
            self.world.move_figure(record, next);
        }
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction.unwrap_or(figure.direction), HOME_SPEED);
        }
    }

    /// The way from `from` towards `to` (`0x3a4dd`): of the direction straight there and those
    /// one, two and three off it, in the order 0, +1, −2, +3, +2, −1, the first walkable one
    /// with no settler on its node, else the last walkable one, else none.
    fn chase_direction(&self, from: Node, to: Node) -> Option<Direction> {
        let straight = crate::catapult::facing(from, to).index();
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let mut fallback = None;
        for turn in [0, 1, 4, 3, 2, 5] {
            let direction = Direction::ALL[(straight + turn) % 6];
            let next = from.neighbour(direction, width, height);
            if !self.nodes.edge_walkable(from, direction) || !self.nodes.get(next).sites.passable() {
                continue;
            }
            if !self.world.figures_at(next).any(|(_, figure)| figure.is_settler()) {
                return Some(direction);
            }
            fallback = Some(direction);
        }
        fallback
    }

    /// Soldier figure `record` walks its cross-country path one edge (`0x3473c`). At the end it
    /// arrives: its home's flag takes it in, any other flag is its target's. A path used up
    /// short of its end is searched again into the same record, and the soldier starts the new
    /// route's first edge in the same step; with no route it is sent home. A soldier on the next
    /// node holds it up ([`Game::held_up`]). With no path it is lost.
    fn walk_soldier_path(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Soldier(Soldier {
            state: SoldierState::Walking { path: Some(path) },
            home,
            held,
            ..
        }) = figure.kind
        else {
            self.make_lost(record);
            return;
        };
        let Some(walked) = self.world.paths.get(path).copied() else {
            self.make_lost(record);
            return;
        };
        let node = figure.node();
        if held && self.walk_held(record, &figure, path, walked) {
            return;
        }
        if node == walked.to {
            self.world.paths.remove(path);
            self.set_soldier_state(record, SoldierState::Walking { path: None });
            let home_flag = home.and_then(|home| self.home_flag_node(home));
            if home_flag == Some(node) {
                self.arrive_home(record);
            } else {
                self.at_target_flag(record);
            }
            return;
        }
        let mut ahead = walked;
        let direction = if let Some(direction) = ahead.next() {
            direction
        } else {
            if !self.route_into(path, node, Crossing::Walkable) {
                self.world.paths.remove(path);
                self.set_soldier_state(record, SoldierState::Walking { path: None });
                self.go_home(record);
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
        if self.held_up(record, next, path).blocked() {
            self.set_held(record, true);
            if let Some(path) = self.world.paths.get_mut(path) {
                *path = ahead;
            }
            if let Some(figure) = self.world.figure_mut(record) {
                figure.direction = direction;
            }
            return;
        }
        if let Some(path) = self.world.paths.get_mut(path) {
            *path = ahead;
        }
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, HOME_SPEED);
        }
    }

    /// A step of soldier figure `record` held before the edge in its direction (`0x3473c`,
    /// sub-state `0x10`). Still held, it waits, and drops the edge when the held search found
    /// it a route, even the one its path already held. Clear, it walks that edge, leaving its path as it is. Returns false only
    /// when it is clear and already at its path's end, which then counts as arriving.
    fn walk_held(&mut self, record: u16, figure: &Figure, path: u16, walked: Path) -> bool {
        let node = figure.node();
        let direction = figure.direction;
        let next = node.neighbour(direction, self.nodes.width(), self.nodes.height());
        match self.held_up(record, next, path) {
            Hold::Clear => {}
            Hold::Waiting => return true,
            Hold::Rerouted => {
                self.set_held(record, false);
                return true;
            }
        }
        self.set_held(record, false);
        if node == walked.to {
            return false;
        }
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, HOME_SPEED);
        }
        true
    }

    /// Soldier figure `record` is or is no longer held before the edge in its direction.
    fn set_held(&mut self, record: u16, held: bool) {
        if let Some(FigureKind::Soldier(soldier)) = self.figure_kind_mut(record) {
            soldier.held = held;
        }
    }

    /// The node of building `building`'s flag.
    fn home_flag_node(&self, building: u16) -> Option<Node> {
        let building = self.world.building(building)?;
        self.world.flag(building.flag).map(|flag| flag.node)
    }

    /// Whether a figure with a soldier record must wait before stepping onto `next`
    /// (`0x345f8`, the branch [`Game::fighting_soldier`] picks). Another player's carriers
    /// there are chased off, and a soldier there holds it back unless it is walking up to its
    /// own opponent.
    pub(crate) fn held_back(&mut self, record: u16, next: Node) -> bool {
        let Some((_, held)) = self.world.soldier_of(record) else {
            return false;
        };
        let Some(FigureKind::Soldier(soldier)) = self.world.figure(record).map(|figure| figure.kind) else {
            return false;
        };
        let blocking = self
            .world
            .figures_at(next)
            .any(|(other, _)| other != record && self.world.soldier_of(other).is_some());
        let first = self
            .world
            .figures_at(next)
            .find(|&(other, figure)| {
                other != record && !matches!(figure.kind, FigureKind::Animal { .. } | FigureKind::Ship { .. })
            })
            .map(|(other, _)| other);
        if let Some(first) = first {
            self.chase_off_carriers(next, soldier.owner, Some(first), true);
        }
        blocking && !matches!(held.step, FightStep::Approaching | FightStep::WillApproach)
    }

    /// The fighting soldier test (`0x32918`): a figure with a soldier record. One tests the node
    /// ahead of it by looking for soldiers standing there rather than by the held bit, and a
    /// held node never stops it part way along an edge.
    pub(crate) fn fighting_soldier(&self, record: u16) -> bool {
        self.world.soldier_of(record).is_some()
    }

    /// Every carrier of an owner other than `owner`, boats aside, on `node` from figure `from`
    /// on in the node's list, or from its head, is chased off its road and lost (`0x34498`).
    /// With `stop_at_soldier` a fighting soldier met ends the walk. The carrier's road frees an
    /// open order still waiting for its person and orders a helper when it has none, becomes an
    /// ordinary road whatever it was and is painted again, which recomputes the sites along it,
    /// and counts one carrier fewer.
    fn chase_off_carriers(&mut self, node: Node, owner: Player, from: Option<u16>, stop_at_soldier: bool) {
        let list: Vec<(u16, Figure)> = self
            .world
            .figures_at(node)
            .map(|(record, figure)| (record, *figure))
            .skip_while(|&(record, _)| from.is_some_and(|from| from != record))
            .collect();
        for (record, figure) in list {
            match figure.kind {
                FigureKind::Animal { .. } | FigureKind::Ship { .. } => {}
                FigureKind::Carrier(carrier) => {
                    if carrier.owner != owner && carrier.transport != crate::figures::Transport::Boat {
                        self.chase_off(record, carrier.on.road);
                    }
                }
                _ => {
                    if stop_at_soldier && self.fighting_soldier(record) {
                        return;
                    }
                }
            }
        }
    }

    /// Carrier figure `record` is chased off road `road` (`0x34505` to `0x345bb`).
    fn chase_off(&mut self, record: u16, road: u16) {
        if let Some(open) = self.world.road(road).and_then(|road| road.order)
            && self.world.order(open).is_some_and(|order| !order.delivering)
        {
            self.world.orders.remove(open);
            if let Some(road) = self.world.road_mut(road) {
                road.order = None;
            }
        }
        if let Some(record) = self.world.road(road).cloned() {
            if record.order.is_none() {
                let order =
                    self.world
                        .order_person(record.start, Job::HELPER, crate::transport::ROAD_MODE, 0);
                if let Some(road) = self.world.road_mut(road) {
                    road.order = order;
                }
            }
            if let Some(road) = self.world.road_mut(road) {
                road.kind = crate::marks::RoadKind::Normal;
            }
            self.world
                .paint(&mut self.nodes, &record, Some(crate::marks::RoadKind::Normal));
        }
        self.lose(record);
        if let Some(road) = self.world.road_mut(road) {
            road.carriers = road.carriers.saturating_sub(1);
        }
    }

    /// Soldier figure `record` is on the flag of a building it may attack (`0x3b253`). With
    /// soldiers inside, one comes out to fight it at the flag; with none it walks in to take the
    /// building. Elsewhere nothing happens, and an attacker that came by path is lost on its
    /// next step.
    fn at_target_flag(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return;
        };
        let Some(building) = self
            .world
            .flag_at(figure.node())
            .and_then(|flag| self.world.building_at_flag(flag))
        else {
            return;
        };
        let Some(owner) = self.world.building_owner(building) else {
            return;
        };
        if !self.defends(building) || owner == soldier.owner {
            return;
        }
        let available = match self.world.military_of(building) {
            Some((_, held)) => held.inside > 0,
            None => {
                self.world
                    .storehouse_of(building)
                    .is_some_and(|(_, stock)| Job::RANKS.iter().any(|job| stock.jobs[job.index()] > 0))
                    || self.world.reserve_rank(building).is_some()
            }
        };
        if !available {
            if let Some(home) = soldier.home
                && let Some((military, _)) = self.world.military_of(home)
                && let Some(held) = self.world.military.get_mut(military)
            {
                held.requested = held.requested.saturating_sub(1);
            }
            self.walk_in(record, building, Entry::Home);
            return;
        }
        if let Some(target) = self.world.building(building).copied() {
            self.post_message(
                crate::messages::MessageId::UNDER_ATTACK,
                owner,
                Some(target.node),
                Some(target.kind),
                0,
            );
        }
        let Some(job) = self.choose_defender(building, owner, false) else {
            return;
        };
        self.update_record(record, |held| {
            held.rounds = 0;
            held.step = FightStep::Facing;
        });
        let Some(defender) = self.make_defender(owner, job, building, record) else {
            return;
        };
        self.update_record(record, |held| held.opponent = Some(defender));
        self.set_soldier_state(record, SoldierState::Waiting);
        self.update_record(defender, |held| held.opponent = Some(record));
        self.hold_node(figure.node(), true);
    }

    /// Sets or clears the held bit on `node` (slot 0 bit 7, `0x401c8`, `0x401f0`).
    fn hold_node(&mut self, node: Node, on: bool) {
        let mut data = self.nodes.get(node);
        data.flags.set(crate::nodes::NodeFlags::HELD, on);
        self.nodes.set(node, data);
    }

    /// Soldier figure `record` goes home (`0x3b52f`). One still in a doorway, walking out or in,
    /// only forgets its target if it is an attacker. Otherwise it takes the role of going home and takes a path to its home's flag,
    /// or is lost with no home or no way there.
    fn go_home(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let Some((_, held)) = self.world.soldier_of(record) else {
            return;
        };
        if !matches!(figure.kind, FigureKind::Soldier(_)) {
            return;
        }
        if figure.in_door() {
            if held.role == Role::Attacker {
                self.update_record(record, |held| held.target = None);
            }
            return;
        }
        self.update_record(record, |held| held.role = Role::Home);
        self.forget_target(record);
        self.head_home(record);
    }

    /// Soldier figure `record` takes a path to its home's flag, arriving at once when it is on
    /// it. With no home, or no path, it is lost.
    fn head_home(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return;
        };
        let Some(home_flag) = soldier.home.and_then(|home| self.home_flag_node(home)) else {
            self.make_lost(record);
            return;
        };
        self.drop_path(record);
        self.set_soldier_state(record, SoldierState::Walking { path: None });
        if figure.node() == home_flag {
            self.arrive_home(record);
            return;
        }
        match self.find_path(figure.node(), home_flag, Crossing::Walkable, false) {
            Some(path) => self.set_soldier_state(record, SoldierState::Walking { path: Some(path) }),
            None => self.make_lost(record),
        }
    }

    /// Clears the target of every defender's record that names `figure` (`0x3b165`). With the
    /// bug fixes off the test meant for defenders lets every record through, so an attacker
    /// whose target building has the same number loses its target too.
    pub(crate) fn forget_target(&mut self, figure: u16) {
        let every = !self.world.bug_fixes.on();
        for (_, held) in self.world.soldiers.iter_mut() {
            if held.target == Some(figure) && (every || held.role == Role::Defender) {
                held.target = None;
            }
        }
    }

    /// Soldier figure `record` is at its home's flag (`0x33b30`): it lets go of any opponent and
    /// walks in.
    fn arrive_home(&mut self, record: u16) {
        let Some(FigureKind::Soldier(soldier)) = self.world.figure(record).map(|figure| figure.kind) else {
            return;
        };
        self.let_go(record);
        let entry = match self.world.soldier_of(record).map(|(_, held)| held.role) {
            Some(Role::Attacker | Role::Home) => Entry::Home,
            Some(Role::Defender) => Entry::Defender,
            _ => Entry::Vanish,
        };
        match soldier.home {
            Some(home) => self.walk_in(record, home, entry),
            None => self.make_lost(record),
        }
    }

    /// Soldier figure `record` lets go of its opponent (`0x3b4dd`); an opponent still paired
    /// with it goes home.
    fn let_go(&mut self, record: u16) {
        let Some((_, held)) = self.world.soldier_of(record) else {
            return;
        };
        self.update_record(record, |held| held.opponent = None);
        let Some(opponent) = held.opponent else {
            return;
        };
        if self
            .world
            .soldier_of(opponent)
            .is_some_and(|(_, other)| other.opponent == Some(record))
        {
            self.update_record(opponent, |other| other.opponent = None);
            self.go_home(opponent);
        }
    }

    /// Soldier figure `record` has walked in through the door of `door` (`0x35a15`, `0x35c38`,
    /// `0x36134`). A soldier coming home joins a military building, capturing it first when it
    /// is another player's; an enemy headquarters is demolished with the soldier in its door; an
    /// own storehouse counts it in. A defender joins its building. One with nothing to do is
    /// gone.
    fn soldier_in(&mut self, record: u16, soldier: Soldier, door: u16, entry: Entry) {
        let Some(site) = self.world.building(door).copied() else {
            self.make_lost(record);
            return;
        };
        if entry == Entry::Vanish {
            if let Some(site) = self.world.building_mut(door) {
                site.state -= BuildingState::DOOR;
                site.state |= BuildingState::PRESENT;
            }
            self.world.remove_figure(record);
            return;
        }
        let owner = self.world.building_owner(door).unwrap_or(site.owner);
        if self.world.military_of(door).is_some() {
            self.join_military(door, soldier.job);
            self.retire(record);
            if entry == Entry::Home && owner != soldier.owner {
                self.capture(door, soldier.owner);
                self.join_military(door, soldier.job);
                if let Some((military, _)) = self.world.military_of(door) {
                    self.staff(military);
                }
            }
            if let Some(site) = self.world.building_mut(door) {
                site.state -= BuildingState::DOOR;
            }
            return;
        }
        if entry == Entry::Home && owner != soldier.owner {
            self.demolish(door);
            return;
        }
        let finished = site.state.contains(BuildingState::FINISHED);
        self.retire(record);
        if finished
            && let Some((store, _)) = self.world.storehouse_of(door)
            && let fixes = self.world.bug_fixes
            && let Some(store) = self.world.storehouse_mut(store)
        {
            store.take_in(soldier.job, fixes);
        }
    }

    /// A soldier of rank `job` joins military building `building` (`0x38e96`).
    fn join_military(&mut self, building: u16, job: Job) {
        if let Some((military, _)) = self.world.military_of(building)
            && let Some(held) = self.world.military.get_mut(military)
        {
            held.inside = held.inside.saturating_add(1);
            held.requested = held.requested.saturating_sub(1);
            let count = &mut held.ranks[rank(job)];
            *count = count.saturating_add(1);
        }
    }

    /// Soldier figure `record` has gone inside: it lets go of its opponent, records naming it
    /// forget it, its record is freed and the figure is gone.
    fn retire(&mut self, record: u16) {
        self.let_go(record);
        self.forget_target(record);
        self.free_record(record);
        self.world.remove_figure(record);
    }
}

#[cfg(test)]
mod tests;
