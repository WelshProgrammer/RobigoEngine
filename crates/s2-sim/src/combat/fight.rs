//! Two soldiers fighting: pairing, walking up, the rounds with their animations, deaths and
//! the winner's next move.

use std::borrow::Cow;

use super::{FightStep, OPPONENT_REACH, Role, rank};
use crate::figures::{Figure, FigureKind, Soldier, SoldierState};
use crate::game::Game;
use crate::grid::Direction;
use crate::homeward::{Crossing, HOME_SPEED};
use crate::world::{MapObject, Remnant};

/// The nations in the table.
const NATIONS: usize = 4;

/// The ranks in the table, private first.
const RANKS: usize = 5;

/// One fighter's animations for a nation, rank and side: records in `ROM_ANIM.DAT`, and the
/// offset an attacker of this rank and side adds to its opponent's hit animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FighterAnimations {
    /// Attacking (`+0x00`).
    pub attack: u16,
    /// Added to the opponent's hit animation (`+0x02`).
    pub hit_offset: u16,
    /// Blocking (`+0x04`).
    pub block: u16,
    /// Dodging, one or the other (`+0x06`, `+0x08`).
    pub dodge: [u16; 2],
    /// Being hit, before the attacker's offset (`+0x0a`).
    pub hit: u16,
    /// Getting ready in the first round (`+0x0c`).
    pub ready: u16,
}

const fn fighter(words: [u16; 7]) -> FighterAnimations {
    FighterAnimations {
        attack: words[0],
        hit_offset: words[1],
        block: words[2],
        dodge: [words[3], words[4]],
        hit: words[5],
        ready: words[6],
    }
}

/// Per nation, rank and side (0 for the soldier that walked up, 1 for the one that waited).
type Table = [[[FighterAnimations; 2]; RANKS]; NATIONS];

/// The fight animations (`0x1fb6b8`) and the death animations of each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FightAnimations {
    table: Cow<'static, Table>,
    death: [u16; 2],
}

/// Why custom fight animations were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FightAnimationsError {
    /// A hit animation plus an attacker's offset runs past the last `ROM_ANIM.DAT` record.
    #[error("a hit animation plus its offset is past the last animation record")]
    HitOverflow,
}

#[rustfmt::skip]
const ORIGINAL_TABLE: Table = [
    [
        [fighter([3639, 0, 5481, 5492, 5503, 615, 5370]), fighter([3694, 0, 460, 471, 482, 631, 5420])],
        [fighter([3650, 2, 5513, 5524, 5535, 647, 5380]), fighter([3705, 2, 492, 502, 512, 663, 5430])],
        [fighter([3661, 4, 5545, 5556, 388, 679, 5390]), fighter([3716, 4, 522, 533, 543, 695, 5440])],
        [fighter([3672, 2, 398, 409, 419, 711, 5400]), fighter([3727, 2, 553, 564, 574, 727, 5450])],
        [fighter([3683, 0, 429, 440, 450, 773, 5410]), fighter([3924, 0, 584, 595, 605, 789, 5460])],
    ],
    [
        [fighter([4589, 0, 1442, 1453, 1464, 1867, 5370]), fighter([4769, 0, 1599, 1610, 1679, 1883, 5420])],
        [fighter([4652, 0, 1474, 1485, 1496, 1899, 5380]), fighter([4779, 0, 1689, 1700, 1711, 1915, 5430])],
        [fighter([4663, 3, 1506, 1517, 1527, 1931, 5390]), fighter([4790, 3, 1721, 1732, 1742, 1947, 5440])],
        [fighter([4747, 2, 1537, 1548, 1558, 1963, 5400]), fighter([4801, 2, 1752, 1763, 1773, 1979, 5450])],
        [fighter([4758, 0, 1568, 1579, 1589, 1995, 5410]), fighter([4998, 0, 1836, 1846, 1857, 2011, 5460])],
    ],
    [
        [fighter([4023, 0, 820, 831, 842, 1224, 5370]), fighter([4203, 0, 1005, 1015, 1059, 1240, 5420])],
        [fighter([4086, 0, 852, 863, 874, 1256, 5380]), fighter([4213, 0, 1069, 1080, 1091, 1272, 5430])],
        [fighter([4097, 3, 884, 923, 933, 1288, 5390]), fighter([4224, 3, 1101, 1112, 1122, 1304, 5440])],
        [fighter([4181, 2, 943, 954, 964, 1320, 5400]), fighter([4235, 2, 1132, 1143, 1153, 1394, 5450])],
        [fighter([4192, 0, 974, 985, 995, 1410, 5410]), fighter([4432, 0, 1193, 1204, 1214, 1426, 5460])],
    ],
    [
        [fighter([5115, 0, 2027, 2038, 2049, 2341, 5370]), fighter([5326, 0, 2184, 2195, 2206, 2357, 5420])],
        [fighter([5179, 0, 2059, 2070, 2081, 2373, 5380]), fighter([5337, 0, 2216, 2227, 2238, 2389, 5430])],
        [fighter([5220, 3, 2091, 2102, 2112, 2405, 5390]), fighter([5348, 3, 2248, 2259, 2269, 2421, 5440])],
        [fighter([5304, 2, 2122, 2133, 2143, 2437, 5400]), fighter([5359, 2, 2279, 2290, 2300, 2470, 5450])],
        [fighter([5315, 0, 2153, 2164, 2174, 2486, 5410]), fighter([5470, 0, 2310, 2321, 2331, 2502, 5460])],
    ],
];

impl FightAnimations {
    /// The animations of `S2.EXE` (Gold has the same table). The soldier that walked up
    /// dies with 805, the one that waited with 5566.
    pub const ORIGINAL: Self = Self {
        table: Cow::Borrowed(&ORIGINAL_TABLE),
        death: [805, 5566],
    };

    /// Custom animations: per nation, rank and side, and the death animation of each side.
    ///
    /// # Errors
    ///
    /// [`FightAnimationsError::HitOverflow`] when a hit animation plus an offset of the same
    /// nation and side passes `u16::MAX`.
    pub fn new(table: Table, death: [u16; 2]) -> Result<Self, FightAnimationsError> {
        for nation in &table {
            for defender in nation.iter().flatten() {
                for attacker in nation.iter().flatten() {
                    if defender.hit.checked_add(attacker.hit_offset).is_none() {
                        return Err(FightAnimationsError::HitOverflow);
                    }
                }
            }
        }
        Ok(Self {
            table: Cow::Owned(table),
            death,
        })
    }

    /// The animations of a fighter of `nation` and `rank` on `side`.
    #[must_use]
    pub fn fighter(&self, nation: u8, rank: usize, side: usize) -> FighterAnimations {
        self.table[usize::from(nation) % NATIONS][rank.min(RANKS - 1)][side.min(1)]
    }

    /// The death animation of a fighter on `side`.
    #[must_use]
    pub fn death(&self, side: usize) -> u16 {
        self.death[side.min(1)]
    }
}

impl Default for FightAnimations {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl std::hash::Hash for FightAnimations {
    /// The tables are input data, checked by the data hash a replay carries; the state hash
    /// leaves them out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

/// The attack roll is a draw from 0 to this, and misses at or above 70 + 10 · rank.
const ATTACK_ROLL: u16 = 130;

/// The defence roll is a draw from 0 to this, and blocks below 70 + 10 · rank.
const DEFENCE_ROLL: u16 = 160;

/// Both rolls start from this and add [`RANK_SKILL`] per rank.
const BASE_SKILL: u16 = 70;
const RANK_SKILL: u16 = 10;

/// A fighter takes its fatal hit with fewer hit points than this.
const FATAL_BELOW: u8 = 2;

/// What one round's attack does.
enum Outcome {
    Miss,
    Block,
    Hit,
}

/// A fighter in a round: its figure, owner's nation, rank and side.
#[derive(Clone, Copy)]
struct Fighter {
    figure: u16,
    nation: u8,
    rank: usize,
    side: usize,
}

impl Game {
    /// Pairs soldier figures `a`, which looked or chased, and `b`, which it found
    /// (`0x3a388`): `a` waits, `b` walks up, and another player's carriers on either node are
    /// chased off.
    pub(super) fn pair(&mut self, a: u16, b: u16) {
        self.drop_path(a);
        self.drop_path(b);
        self.update_record(b, |held| {
            held.opponent = Some(a);
            held.rounds = 0;
            held.step = FightStep::WillApproach;
        });
        self.update_record(a, |held| {
            held.opponent = Some(b);
            held.rounds = 0;
            held.step = FightStep::Waiting;
        });
        self.set_soldier_state(a, SoldierState::Waiting);
        for figure in [a, b] {
            if let Some(found) = self.world.figure(figure).copied()
                && let FigureKind::Soldier(soldier) = found.kind
            {
                self.chase_off_carriers(found.node(), soldier.owner, Some(figure), false);
            }
        }
    }

    /// The fight step of soldier figure `record` (`0x3a8c4`), run as it stands on a node or as
    /// its animation ends. Returns false when the fight did not take over: with no opponent its
    /// step goes back to none.
    pub(super) fn fight_step(&mut self, record: u16) -> bool {
        let Some((_, held)) = self.world.soldier_of(record) else {
            return false;
        };
        let Some(opponent) = held.opponent else {
            self.update_record(record, |held| held.step = FightStep::None);
            return false;
        };
        match held.step {
            FightStep::WillApproach | FightStep::Approaching => {
                self.approach(record, opponent, held.step);
                true
            }
            FightStep::FellWaiting | FightStep::FellFacing => {
                let side = usize::from(held.step == FightStep::FellWaiting);
                let animation = self.world.fight_animations.death(side);
                self.animate(record, animation);
                self.update_record(record, |held| held.step = FightStep::Dying);
                true
            }
            FightStep::Dying => {
                self.die(record);
                true
            }
            FightStep::Waiting | FightStep::Facing => {
                let beaten = self.world.soldier_of(opponent).is_none_or(|(_, other)| {
                    matches!(
                        other.step,
                        FightStep::Dying | FightStep::FellWaiting | FightStep::FellFacing
                    )
                });
                if beaten {
                    self.won(record)
                } else {
                    self.round(record, opponent);
                    true
                }
            }
            FightStep::None => false,
        }
    }

    /// Soldier figure `record` walks up to its opponent: on the opponent's node it holds the
    /// node and the first round starts; otherwise it steps towards it at walking speed, east
    /// when both stand on one node, whatever the ground.
    fn approach(&mut self, record: u16, opponent: u16, step: FightStep) {
        let (Some(here), Some(there)) = (
            self.world.figure(record).map(Figure::node),
            self.world.figure(opponent).map(Figure::node),
        ) else {
            return;
        };
        if step == FightStep::Approaching && here == there {
            self.update_record(record, |held| held.step = FightStep::Facing);
            self.hold_node(here, true);
            self.round(record, opponent);
            return;
        }
        self.update_record(record, |held| held.step = FightStep::Approaching);
        let direction = if here == there {
            Direction::East
        } else {
            crate::catapult::facing(here, there)
        };
        let next = here.neighbour(direction, self.nodes.width(), self.nodes.height());
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, HOME_SPEED);
        }
    }

    /// A round between soldier figure `me` and its opponent `other` (`0x39c6b`). The first only
    /// plays both ready animations. In later rounds the soldier that walked up attacks in the
    /// even ones and the one that waited in the odd ones: a miss, a block or a hit, which costs
    /// a hit point and with the last one is fatal.
    pub(super) fn round(&mut self, me: u16, other: u16) {
        let (Some((_, mine)), Some(_)) = (self.world.soldier_of(me), self.world.soldier_of(other)) else {
            return;
        };
        let (waiting, walked) = if mine.step == FightStep::Waiting {
            (me, other)
        } else {
            (other, me)
        };
        let (Some(waiting_side), Some(walked_side)) = (self.fighter(waiting, 1), self.fighter(walked, 0))
        else {
            return;
        };
        let (attacker, defender) = if mine.rounds % 2 == 1 {
            (walked_side, waiting_side)
        } else {
            (waiting_side, walked_side)
        };
        self.stop_animation(me);
        self.stop_animation(other);
        let Some(node) = self.world.figure(me).map(Figure::node) else {
            return;
        };
        let first = self
            .world
            .soldier_of(attacker.figure)
            .is_some_and(|(_, held)| held.rounds == 0);
        let animations = self.world.fight_animations.clone();
        let of = |fighter: Fighter| animations.fighter(fighter.nation, fighter.rank, fighter.side);
        if first {
            self.animate(attacker.figure, of(attacker).ready);
            self.animate(defender.figure, of(defender).ready);
            for figure in [me, other] {
                self.update_record(figure, |held| held.rounds = 1);
            }
            return;
        }
        for figure in [me, other] {
            self.update_record(figure, |held| held.rounds = held.rounds.wrapping_add(1));
        }
        match self.outcome(attacker.rank, defender.rank) {
            Outcome::Miss => {
                self.animate(attacker.figure, of(attacker).attack);
                let dodge = usize::from(self.rng.up_to(1));
                self.animate(defender.figure, of(defender).dodge[dodge]);
            }
            Outcome::Block => {
                self.animate(attacker.figure, of(attacker).attack);
                self.animate(defender.figure, of(defender).block);
            }
            Outcome::Hit => {
                let offset = animations
                    .fighter(defender.nation, attacker.rank, attacker.side)
                    .hit_offset;
                self.animate(defender.figure, of(defender).hit.wrapping_add(offset));
                self.animate(attacker.figure, of(attacker).attack);
                let fatal = self
                    .world
                    .soldier_of(defender.figure)
                    .is_some_and(|(_, held)| held.hit_points < FATAL_BELOW);
                if fatal {
                    let step = if defender.figure == waiting {
                        FightStep::FellWaiting
                    } else {
                        FightStep::FellFacing
                    };
                    self.update_record(defender.figure, |held| {
                        held.step = step;
                        held.role = Role::None;
                    });
                    self.hold_node(node, false);
                } else {
                    self.update_record(defender.figure, |held| held.hit_points -= 1);
                }
            }
        }
    }

    /// Soldier figure `figure` as a fighter on `side`.
    fn fighter(&self, figure: u16, side: usize) -> Option<Fighter> {
        let FigureKind::Soldier(soldier) = self.world.figure(figure)?.kind else {
            return None;
        };
        Some(Fighter {
            figure,
            nation: self.world.nations[soldier.owner.index()],
            rank: rank(soldier.job),
            side,
        })
    }

    /// One round's attack by a soldier of `attacker` rank on one of `defender` rank
    /// (`0x39ffc`): a draw from 0 to 130 at or above 70 + 10 · rank misses; otherwise a draw
    /// from 0 to 160 below the defender's 70 + 10 · rank blocks, and anything else hits.
    fn outcome(&mut self, attacker: usize, defender: usize) -> Outcome {
        let skill = |rank: usize| BASE_SKILL + RANK_SKILL * u16::try_from(rank).unwrap_or(0);
        if self.rng.up_to(ATTACK_ROLL) >= skill(attacker) {
            return Outcome::Miss;
        }
        if self.rng.up_to(DEFENCE_ROLL) < skill(defender) {
            Outcome::Block
        } else {
            Outcome::Hit
        }
    }

    /// Frees soldier figure `figure`'s running animation (`0x321e9`).
    fn stop_animation(&mut self, figure: u16) {
        let animation = match self.figure_kind_mut(figure) {
            Some(FigureKind::Soldier(Soldier {
                state: SoldierState::Animating { animation },
                ..
            })) => animation.take(),
            _ => None,
        };
        if let Some(animation) = animation {
            self.world.animations.remove(animation);
        }
    }

    /// Soldier figure `figure` plays animation `start` on its node (`0x339b8`).
    fn animate(&mut self, figure: u16, start: u16) {
        let Some(node) = self.world.figure(figure).map(Figure::node) else {
            return;
        };
        let animation = self.start_animation_at(start, node);
        self.set_soldier_state(figure, SoldierState::Animating { animation });
    }

    /// Soldier figure `record`'s animation has ended (`0x37f78`). Outside a fight, as when it
    /// won one and is going home, it recomputes the sites round its node and searches a path to
    /// its home's flag (`0x33894`). With no home or no path it is lost. With the bug fixes off a
    /// soldier already standing on that flag finds none, since a search never ends where it
    /// starts; with them on it walks in.
    pub(super) fn animation_ended(&mut self, record: u16) {
        if self.fight_step(record) {
            return;
        }
        self.set_soldier_state(record, SoldierState::Walking { path: None });
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        crate::sites::refresh_sites(&mut self.nodes, &self.world, figure.node(), 1);
        let home = match figure.kind {
            FigureKind::Soldier(soldier) => soldier.home,
            _ => None,
        };
        let flag = home.and_then(|home| self.home_flag_node(home));
        if self.world.bug_fixes.on() && flag == Some(figure.node()) {
            self.arrive_home(record);
            return;
        }
        let path = flag.and_then(|flag| self.find_path(figure.node(), flag, Crossing::Walkable, false));
        match path {
            Some(path) => self.set_soldier_state(record, SoldierState::Walking { path: Some(path) }),
            None => self.make_lost(record),
        }
    }

    /// Soldier figure `record`'s death animation has ended (`0x3b1bc`): its home counts it no
    /// longer, records naming it forget it, its record is freed, the figure is gone, and an
    /// empty node gets a skeleton.
    fn die(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return;
        };
        if let Some(home) = soldier.home
            && let Some((military, _)) = self.world.military_of(home)
            && let Some(held) = self.world.military.get_mut(military)
        {
            held.requested = held.requested.saturating_sub(1);
        }
        self.forget_target(record);
        self.free_record(record);
        self.world.drop_people(soldier.owner, soldier.job, 1);
        self.world.remove_figure(record);
        if self.world.object(figure.node()) == MapObject::Nothing {
            self.world
                .set_object(figure.node(), MapObject::Remnant(Remnant::SKELETON));
        }
    }

    /// Soldier figure `record` has won its fight (`0x3ab95`). An attacker fights the next enemy
    /// within one step, or takes on its target's flag, or walks on to it, or goes home with
    /// its target gone. A defender fights the next enemy within one step or goes home. Returns
    /// false for a soldier going home, which walks on once its animation has ended.
    fn won(&mut self, record: u16) -> bool {
        self.update_record(record, |held| {
            held.opponent = None;
            held.rounds = 0;
        });
        self.set_soldier_state(record, SoldierState::Walking { path: None });
        let Some(figure) = self.world.figure(record).copied() else {
            return true;
        };
        let FigureKind::Soldier(soldier) = figure.kind else {
            return true;
        };
        self.world.count_fight_won(soldier.owner);
        let Some((_, held)) = self.world.soldier_of(record) else {
            return true;
        };
        match held.role {
            Role::Attacker => {
                let Some(target) = held.target else {
                    self.go_home(record);
                    return true;
                };
                if let Some(found) = self.find_opponent(figure.node(), soldier.owner, OPPONENT_REACH) {
                    self.pair(record, found);
                    return true;
                }
                let flag = self.home_flag_node(target);
                if flag == Some(figure.node()) {
                    self.at_target_flag(record);
                    return true;
                }
                let path =
                    flag.and_then(|flag| self.find_path(figure.node(), flag, Crossing::Walkable, false));
                match path {
                    Some(path) => self.set_soldier_state(record, SoldierState::Walking { path: Some(path) }),
                    None => self.make_lost(record),
                }
                true
            }
            Role::Defender => {
                match self.find_opponent(figure.node(), soldier.owner, OPPONENT_REACH) {
                    Some(found) => self.pair(record, found),
                    None => self.go_home(record),
                }
                true
            }
            Role::Home | Role::None => false,
        }
    }
}
