//! Carriers in the simulation step (`0x34e64`): an idle carrier walks to the middle of its road,
//! turns to face the viewer and rolls for idle animations, which run from `ROM_ANIM.DAT`
//! (`0x23926`). A carrier with a ware for the building at its flag walks it in ([`delivery`]).

mod delivery;

use std::sync::Arc;

use s2_formats::bob::RomAnim;

use crate::figures::{Carrier, CarrierState, Figure, FigureKind, Transport};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::transport::{Ware, WareState};
use crate::world::FLAG_CAPACITY;

/// Idle carriers act on steps where `(record + step counter) mod 3 == 0`.
const POLL_PERIOD: u32 = 3;

/// What an idle carrier that finds no ware takes off its road's busy counter (`0x292d4`).
const IDLE_RELIEF: u8 = 10;

/// The idle roll: a draw from 0 to this (`0x350dd`).
const IDLE_ROLL: u16 = 2000;

/// An idle carrier looks at its start flag first when it is less than this percentage of the way
/// along its road (`0x2a731`).
const NEAR_START: u32 = 30;
const PERCENT: u32 = 100;

/// Carriers whose record number is 3 modulo 4 use the second set of idle animations.
const SECOND_SET: u16 = 3;

/// `ROM_ANIM` ops (`0x23926`); anything above 4 shows a frame.
const OP_CALL: u8 = 0;
const OP_END: u8 = 1;
const OP_RETURN: u8 = 2;
const OP_MAP_OBJECT: u8 = 3;
const OP_SOUND: u8 = 4;

/// The commands of `ROM_ANIM.DAT` the simulation follows: each record's op and argument. Frames
/// and sounds change nothing in the simulation but how long an animation runs.
#[derive(Debug, Clone, Default)]
pub struct AnimationScript(Option<Arc<[(u8, u16)]>>);

impl AnimationScript {
    /// The script in `rom_anim`.
    #[must_use]
    pub fn new(rom_anim: &RomAnim) -> Self {
        Self(Some(
            rom_anim
                .records()
                .iter()
                .map(|record| (record.op, record.arg))
                .collect(),
        ))
    }

    fn get(&self, index: u16) -> Option<(u8, u16)> {
        self.0.as_ref()?.get(usize::from(index)).copied()
    }

    fn is_loaded(&self) -> bool {
        self.0.is_some()
    }
}

impl PartialEq for AnimationScript {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_deref() == other.0.as_deref()
    }
}

impl Eq for AnimationScript {}

impl std::hash::Hash for AnimationScript {
    /// The script is input data, checked by the data hash a replay carries; the state hash leaves
    /// it out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

/// An effect an animation asked for with op 4, and the node it plays on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationSound {
    /// Where the animation runs: the figure's or the building's node.
    pub node: Node,
    /// The argument op 4 passes to `0x23b0e` as an effect id.
    pub effect: u16,
}

/// The effects op 4 asked for since the caller last took them. They are output for the sound
/// side, which plays only those whose node is in view; the state hash and equality leave them
/// out.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sounds(pub(crate) Vec<AnimationSound>);

impl PartialEq for Sounds {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for Sounds {}

impl std::hash::Hash for Sounds {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

/// A running animation (the 4-byte records of the pool at `0x2016e4`): the record it is at and the
/// record to return to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Animation {
    /// The current record (`+0`).
    pub index: u16,
    /// The record of the last call (`+2`).
    pub saved: u16,
}

/// The record an idle roll of `roll` starts, for a carrier in the second set or not.
const fn idle_animation(second_set: bool, roll: u16) -> Option<u16> {
    Some(match (second_set, roll) {
        (true, 0 | 10..=16) => 3095,
        (true, 1..=2) => 2970,
        (true, 3..=6) => 2986,
        (true, 7..=9) => 3005,
        (false, 0 | 10..=15) => 3184,
        (false, 1) => 3024,
        (false, 2..=7) => 3045,
        (false, 8..=9) => 3057,
        _ => return None,
    })
}

impl Game {
    /// Gives the game `ROM_ANIM.DAT`, which idle animations run from. Without it a carrier still
    /// rolls for an animation but never starts one.
    pub fn set_animations(&mut self, script: AnimationScript) {
        self.script = script;
    }

    /// One step of carrier `record`, which is not walking (`0x34e64`). A carrier fetching a ware
    /// that has since been stranded, or that no longer heads along its road, gives up first and
    /// leaves the ware marked as taken.
    pub(crate) fn step_carrier(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Carrier(mut carrier) = figure.kind else {
            return;
        };
        if carrier.state == CarrierState::Animating {
            let at = figure.node();
            let running = carrier
                .animation
                .is_some_and(|animation| self.run_animation(animation, at));
            if !running && let Some(FigureKind::Carrier(carrier)) = self.figure_kind_mut(record) {
                carrier.animation = None;
                carrier.state = CarrierState::Waiting;
            }
            return;
        }
        if carrier.state == CarrierState::Fetching && self.fetch_gone(&carrier) {
            carrier.state = CarrierState::Idle;
            self.set_carrier(record, carrier);
        }
        match carrier.state {
            CarrierState::Idle | CarrierState::Waiting => {
                self.idle_carrier(record, figure.direction, carrier);
            }
            CarrierState::Fetching | CarrierState::Carrying => self.busy_carrier(record, carrier),
            CarrierState::Animating => {}
        }
    }

    /// Whether the ware `carrier` is fetching is stranded, unrouted, gone, or no longer heads for
    /// either end of its road.
    fn fetch_gone(&self, carrier: &Carrier) -> bool {
        let Some(ware) = carrier.ware.and_then(|ware| self.world.ware(ware)) else {
            return true;
        };
        let Some(road) = self.world.road(carrier.on.road) else {
            return true;
        };
        matches!(ware.state, WareState::Stranded | WareState::Unrouted)
            || !(ware.flag == Some(road.start) || ware.flag.is_some() && ware.flag == road.end)
    }

    /// An idle carrier on a polling step: it takes the best ware waiting at the nearer end for
    /// the other end, then at the other end, where nearer means under 30 % of the way along.
    /// With none it walks towards the middle, and there turns to face the viewer and rolls for an
    /// idle animation.
    fn idle_carrier(&mut self, record: u16, facing: Direction, mut carrier: Carrier) {
        if !(u32::from(record) + self.clock.step()).is_multiple_of(POLL_PERIOD) {
            return;
        }
        let Some(road) = self.world.road(carrier.on.road) else {
            return;
        };
        let (start, edges) = (road.start, road.edges());
        let Some(end) = road.end else {
            return;
        };
        let along = u32::from(carrier.on.position) * PERCENT / u32::from(edges);
        let ends = if along < NEAR_START {
            [(start, end, true), (end, start, false)]
        } else {
            [(end, start, false), (start, end, true)]
        };
        for (at, toward, to_start) in ends {
            if let Some(ware) = self.world.best_ware(at, toward) {
                if let Some(record) = self.world.wares.get_mut(ware) {
                    record.state = WareState::Taken;
                }
                carrier.ware = Some(ware);
                carrier.on.to_start = to_start;
                carrier.state = CarrierState::Fetching;
                self.set_carrier(record, carrier);
                if let Some(road) = self.world.road_mut(carrier.on.road) {
                    road.busy = road.busy.saturating_add(1);
                }
                return;
            }
        }
        let Some(road) = self.world.road_mut(carrier.on.road) else {
            return;
        };
        road.busy = road.busy.saturating_sub(IDLE_RELIEF);
        let middle = u8::try_from(road.edges() / 2).unwrap_or(u8::MAX);
        let position = carrier.on.position;
        if position != middle {
            self.carrier_towards(record, carrier, middle);
            return;
        }
        if carrier.transport != Transport::Foot {
            return;
        }
        let turned = match facing {
            Direction::East => Direction::SouthEast,
            Direction::West => Direction::SouthWest,
            Direction::NorthWest => Direction::West,
            Direction::NorthEast => Direction::East,
            Direction::SouthEast | Direction::SouthWest => {
                self.roll_idle(record);
                return;
            }
        };
        if let Some(figure) = self.world.figure_mut(record) {
            figure.direction = turned;
            figure.speed = 0;
        }
    }

    /// A carrier fetching or carrying a ware, every step. At the target flag it picks the ware
    /// up, or puts it down, and stays there this step; a ware destined for that flag it walks into
    /// the building. A carrier with a ware for a full flag that is not the ware's destination
    /// swaps it for a ware heading back along the road; with none to swap it waits beside the
    /// flag. Otherwise it walks one node towards the target flag.
    fn busy_carrier(&mut self, record: u16, mut carrier: Carrier) {
        let Some(road) = self.world.road(carrier.on.road) else {
            return;
        };
        let Some(end) = road.end else {
            return;
        };
        let edges = u8::try_from(road.edges()).unwrap_or(u8::MAX);
        let (target, flag, other) = if carrier.on.to_start {
            (0, road.start, end)
        } else {
            (edges, end, road.start)
        };
        let beside = if target == 0 { 1 } else { edges - 1 };
        let Some(ware) = carrier.ware else {
            carrier.state = CarrierState::Idle;
            self.set_carrier(record, carrier);
            return;
        };
        let position = carrier.on.position;
        let mut goal = target;
        if carrier.state == CarrierState::Fetching {
            if position == target {
                if self.world.take_ware(flag, ware) {
                    self.carry(record, &mut carrier, ware, true);
                    self.count_traffic(carrier.on.road);
                } else {
                    carrier.state = CarrierState::Idle;
                    carrier.ware = None;
                    self.set_carrier(record, carrier);
                }
                return;
            }
        } else {
            let destined = self.world.destination(ware) == Some(flag);
            let full = self
                .world
                .flag(flag)
                .is_some_and(|flag| flag.waiting >= FLAG_CAPACITY);
            if full && !destined {
                let swap = self.world.best_ware(flag, other);
                if position == target {
                    if let Some(swap) = swap {
                        self.world.take_ware(flag, swap);
                        self.world.put_down(ware, flag);
                        self.carry(record, &mut carrier, swap, false);
                        return;
                    }
                    goal = beside;
                } else if position == beside && swap.is_none() {
                    return;
                }
            } else if position == target {
                if destined {
                    self.start_delivery(record, &carrier, ware);
                    return;
                }
                if self.world.put_down(ware, flag) {
                    carrier.state = CarrierState::Idle;
                    carrier.ware = None;
                    carrier.shows = None;
                    self.set_carrier(record, carrier);
                }
                return;
            }
        }
        if position != goal {
            self.carrier_towards(record, carrier, goal);
        }
    }

    /// Carrier `record` takes `ware` on and turns for the other end. A ware picked up normally is
    /// marked as carried by it; one taken in a swap keeps its waiting state and its old link, as
    /// in `S2.EXE`. A pack donkey shows no ware bitmap after a normal pick-up (`0x3537b`), but a
    /// swap writes it for a donkey too (`0x35744`).
    fn carry(&mut self, record: u16, carrier: &mut Carrier, ware: u16, mark: bool) {
        let bitmap = self.world.ware(ware).and_then(Ware::bitmap);
        if mark && let Some(taken) = self.world.wares.get_mut(ware) {
            taken.state = WareState::Carried;
            taken.link = Some(record);
        }
        carrier.ware = Some(ware);
        carrier.on.to_start = !carrier.on.to_start;
        carrier.state = CarrierState::Carrying;
        carrier.shows = if carrier.transport == Transport::Donkey && mark {
            None
        } else {
            bitmap
        };
        self.set_carrier(record, *carrier);
    }

    /// Starts carrier `record` one node along its road towards position `goal`. A next node that
    /// is no longer beside it removes the carrier.
    fn carrier_towards(&mut self, record: u16, carrier: Carrier, goal: u8) {
        let position = carrier.on.position;
        let next = if position < goal {
            position + 1
        } else {
            position - 1
        };
        if !self.walk_on(record, carrier.on.road, next) {
            self.remove_carrier(record);
            return;
        }
        if let Some(FigureKind::Carrier(carrier)) = self.figure_kind_mut(record) {
            carrier.on.position = next;
        }
    }

    fn set_carrier(&mut self, record: u16, carrier: Carrier) {
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Carrier(carrier);
        }
    }

    /// The idle roll (`0x350b9`): the carrier waits, draws, and on a few draws starts an animation
    /// and runs its first step.
    fn roll_idle(&mut self, record: u16) {
        if let Some(FigureKind::Carrier(carrier)) = self.figure_kind_mut(record) {
            carrier.state = CarrierState::Waiting;
        }
        let roll = self.rng.up_to(IDLE_ROLL);
        let Some(start) = idle_animation(record % 4 == SECOND_SET, roll) else {
            return;
        };
        let Some(at) = self.world.figure(record).map(Figure::node) else {
            return;
        };
        let Some(animation) = self.start_animation(start, at) else {
            return;
        };
        if let Some(FigureKind::Carrier(carrier)) = self.figure_kind_mut(record) {
            carrier.animation = Some(animation);
            carrier.state = CarrierState::Animating;
        }
    }

    /// Starts the animation at record `start` on `node`, whose trees its map-object commands
    /// fell (`0x339b8`), and runs its first step.
    pub(crate) fn start_animation_at(&mut self, start: u16, node: Node) -> Option<u16> {
        if !self.script.is_loaded() {
            return None;
        }
        let animation = self.world.animations.insert(
            Animation {
                index: start,
                saved: 0,
            },
            usize::from(u16::MAX),
        )?;
        self.run_animation_at(animation, node).then_some(animation)
    }

    /// Starts the `ROM_ANIM` animation at record `start` (`0x321b0`) for something at `at` and
    /// runs its first step. Returns its state record while it runs; `None` without a script,
    /// with a full pool, or when the first step already ends it.
    pub(crate) fn start_animation(&mut self, start: u16, at: Node) -> Option<u16> {
        if !self.script.is_loaded() {
            return None;
        }
        let animation = self.world.animations.insert(
            Animation {
                index: start,
                saved: 0,
            },
            usize::from(u16::MAX),
        )?;
        self.run_animation(animation, at).then_some(animation)
    }

    /// One `ROM_ANIM` step of animation `record` for something at `at` (`0x23926`): commands up
    /// to the next frame. Returns false when the animation ends, which frees it.
    pub(crate) fn run_animation(&mut self, record: u16, at: Node) -> bool {
        self.animation_step(record, None, at)
    }

    /// One `ROM_ANIM` step of animation `record` running on `node`: a map-object command fells
    /// the tree there by one stage, its argument less one (`0x23926`, `0x3d784`).
    pub(crate) fn run_animation_at(&mut self, record: u16, node: Node) -> bool {
        self.animation_step(record, Some(node), node)
    }

    fn animation_step(&mut self, record: u16, node: Option<Node>, at: Node) -> bool {
        loop {
            let Some(animation) = self.world.animations.get_mut(record) else {
                return false;
            };
            animation.index = animation.index.wrapping_add(1);
            let Some((op, arg)) = self.script.get(animation.index) else {
                self.world.animations.remove(record);
                return false;
            };
            match op {
                OP_CALL => {
                    animation.saved = animation.index;
                    animation.index = arg.wrapping_sub(1);
                }
                OP_END => {
                    self.world.animations.remove(record);
                    return false;
                }
                OP_RETURN => animation.index = animation.saved,
                OP_MAP_OBJECT => {
                    if let Some(node) = node {
                        self.world.fell(&mut self.nodes, node, arg.wrapping_sub(1));
                    }
                }
                OP_SOUND => self.sounds.0.push(AnimationSound {
                    node: at,
                    effect: arg,
                }),
                _ => return true,
            }
        }
    }

    /// Takes carrier `record` off its road and out of the game (`0x33104`). One fetching or
    /// carrying a ware also takes one off the road's busy counter.
    fn remove_carrier(&mut self, record: u16) {
        if let Some(FigureKind::Carrier(carrier)) = self.world.figure(record).map(|figure| figure.kind)
            && let Some(road) = self.world.road_mut(carrier.on.road)
        {
            road.carriers = road.carriers.saturating_sub(1);
            if matches!(carrier.state, CarrierState::Fetching | CarrierState::Carrying) {
                road.busy = road.busy.saturating_sub(1);
            }
        }
        self.world.remove_figure(record);
    }

    pub(crate) fn figure_kind_mut(&mut self, record: u16) -> Option<&mut FigureKind> {
        self.world.figure_mut(record).map(|figure| &mut figure.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_rolls_pick_animations_as_the_table_says() {
        assert_eq!(idle_animation(true, 0), Some(3095));
        assert_eq!(idle_animation(true, 16), Some(3095));
        assert_eq!(idle_animation(true, 17), None);
        assert_eq!(idle_animation(false, 15), Some(3184));
        assert_eq!(idle_animation(false, 16), None);
        let hits = |second| {
            (0..=IDLE_ROLL)
                .filter(|&roll| idle_animation(second, roll).is_some())
                .count()
        };
        assert_eq!((hits(true), hits(false)), (17, 16));
    }
}
