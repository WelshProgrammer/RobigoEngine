//! Task 16: building projects (`0x6d065`). A project builds an economy chain one building at a
//! time: each step orders the next building near the last one placed, and that order's result
//! moves the project on.

use super::board::{BEER, COAL, DONKEYS, FOOD, GOLD, STONE, TOOLS, WEAPONS, WOOD};
use super::chain::DONE;
use super::jobs::{Job, JobKind, Payload};
use super::tasks::Think;
use super::{ComputerPlayer, TaskKind};
use crate::grid::Node;
use crate::ware::WareKind;
use crate::world::BuildingKind;

/// The option bit for an extra woodcutter in the food, beer and donkey projects.
pub(super) const WITH_WOODCUTTER: u16 = 1 << 8;

/// The food project's variants, the low byte of its options.
const FARM_AND_MILL: u16 = 0;
const FARM_AND_PIGS: u16 = 1;
const FISH: u16 = 2;

/// The stone project's options.
const QUARRY: u16 = 0;
const GRANITE_MINE: u16 = 1;

/// A weapons project asks for beer while the player has less than this in stock (`0x6da58`).
const BEER_STOCK: u16 = 30;

/// A project's fields as one step reads and writes them.
#[derive(Clone, Copy)]
struct Progress {
    project: u8,
    options: u16,
    step: u16,
    anchor: Node,
    saved: Node,
}

impl ComputerPlayer {
    /// A project's turn (`0x6d094`): the first runs step 0, the rest wait on the children.
    pub(super) fn project_turn(&mut self, index: u16, think: &mut Think<'_>) -> u16 {
        if let Some(Payload::Project { started: false, .. }) = self.job(index).map(|job| job.payload) {
            self.project_step(index, think);
            if let Some(Job {
                payload: Payload::Project { started, .. },
                ..
            }) = self.job_mut(index)
            {
                *started = true;
            }
        }
        DONE
    }

    /// A project hears its build order (`0x6d0fb`). A failure puts the project back on the board
    /// and ends it. A success makes the order's site the anchor and runs the next step at once.
    pub(super) fn project_heard(&mut self, index: u16, result: u16, think: &mut Think<'_>) {
        let Some(Payload::Project { project, node, .. }) = self.job(index).map(|job| job.payload) else {
            return;
        };
        if result == 0 {
            let at = (node.x.cast_signed(), node.y.cast_signed());
            self.tasks.board.add(project, at, think.profile.add_wait, true);
            self.end(index);
            return;
        }
        let placed = self
            .child_of(index, JobKind::BuildOrder)
            .and_then(|order| self.child_of(order, JobKind::Place))
            .and_then(|placement| self.child_of(placement, JobKind::Site))
            .and_then(|site| match self.job(site)?.payload {
                Payload::Site(search) => Some(search.at),
                _ => None,
            });
        if let Some(Job {
            payload: Payload::Project { step, anchor, .. },
            ..
        }) = self.job_mut(index)
        {
            if let Some(placed) = placed {
                *anchor = placed;
            }
            *step += 1;
        }
        self.project_step(index, think);
    }

    /// Runs the project's current step (`0x6d226`).
    fn project_step(&mut self, index: u16, think: &mut Think<'_>) {
        let Some(Payload::Project {
            project,
            options,
            step,
            anchor,
            saved,
            ..
        }) = self.job(index).map(|job| job.payload)
        else {
            return;
        };
        let mut fields = Progress {
            project,
            options,
            step,
            anchor,
            saved,
        };
        let done = match fields.project {
            WOOD => self.wood(index, &mut fields, think),
            STONE => self.stone(index, &fields, think),
            WEAPONS | TOOLS => self.iron(index, &mut fields, think),
            GOLD => self.gold(index, &mut fields, think),
            FOOD => self.food(index, &fields, think),
            BEER => self.beer(index, &fields, think),
            COAL => self.coal(index, &fields, think),
            DONKEYS => self.donkeys(index, &mut fields, think),
            _ => false,
        };
        if let Some(Job {
            payload: Payload::Project { anchor, saved, .. },
            ..
        }) = self.job_mut(index)
        {
            *anchor = fields.anchor;
            *saved = fields.saved;
        }
        if done {
            self.end(index);
        }
    }

    /// The next building of project `index`, which hears how it went.
    fn next(&mut self, index: u16, kind: BuildingKind, at: Node, radius: u16, think: &Think<'_>) {
        self.add_order(think, (kind, at, radius), Some(index), Some(TaskKind::Projects));
    }

    /// A building no one hears about.
    fn extra(&mut self, kind: BuildingKind, at: Node, radius: u16, think: &Think<'_>) {
        self.add_order(think, (kind, at, radius), None, None);
    }

    /// The wood chain: a sawmill, two woodcutters by it and a forester unless one stands near.
    fn wood(&mut self, index: u16, fields: &mut Progress, think: &Think<'_>) -> bool {
        match fields.step {
            0 => self.next(index, BuildingKind::SAWMILL, fields.anchor, 16, think),
            1 => {
                fields.saved = fields.anchor;
                self.next(index, BuildingKind::WOODCUTTER, fields.anchor, 16, think);
            }
            2 => {
                fields.anchor = fields.saved;
                self.next(index, BuildingKind::WOODCUTTER, fields.anchor, 16, think);
            }
            3 => {
                fields.anchor = fields.saved;
                if !think.any_near(think.player, BuildingKind::FORESTER, fields.anchor, 12) {
                    self.extra(BuildingKind::FORESTER, fields.anchor, 8, think);
                }
                return true;
            }
            _ => {}
        }
        false
    }

    /// The stone chain: a quarry, or a granite mine and a food request by it. Options other than
    /// 0 and 1 leave the project idle for good, as in `S2.EXE`.
    fn stone(&mut self, index: u16, fields: &Progress, think: &Think<'_>) -> bool {
        match (fields.options, fields.step) {
            (QUARRY, 0) => self.next(index, BuildingKind::QUARRY, fields.anchor, 12, think),
            (GRANITE_MINE, 0) => self.next(index, BuildingKind::GRANITE_MINE, fields.anchor, 8, think),
            (GRANITE_MINE, 1) => {
                let at = (fields.anchor.x.cast_signed(), fields.anchor.y.cast_signed());
                self.tasks.board.add(FOOD, at, think.profile.add_wait, false);
                return true;
            }
            (QUARRY, 1) => return true,
            _ => {}
        }
        false
    }

    /// The weapons and tools chains: an iron mine, a smelter, then an armoury or metalworks, a
    /// coal mine by the iron mine and now and then a guardhouse. A weapons chain asks for beer by
    /// the smelter when breweries are short and beer runs low.
    fn iron(&mut self, index: u16, fields: &mut Progress, think: &mut Think<'_>) -> bool {
        let weapons = fields.project == WEAPONS;
        match fields.step {
            0 => self.next(index, BuildingKind::IRON_MINE, fields.anchor, 16, think),
            1 => {
                fields.saved = fields.anchor;
                self.next(index, BuildingKind::IRON_SMELTER, fields.anchor, 16, think);
            }
            2 if weapons => {
                self.next(index, BuildingKind::ARMOURY, fields.anchor, 16, think);
                let smelter = fields.anchor;
                fields.anchor = fields.saved;
                self.extra(BuildingKind::COAL_MINE, fields.anchor, 16, think);
                fields.saved = smelter;
            }
            2 => self.next(index, BuildingKind::METALWORKS, fields.anchor, 16, think),
            3 => {
                let chance = think.profile.guards[usize::from(!weapons)];
                if think.game.rng.up_to(100) < chance {
                    self.extra(BuildingKind::GUARDHOUSE, fields.anchor, 20, think);
                }
                if weapons {
                    self.ask_for_beer(fields.saved, think);
                } else {
                    fields.anchor = fields.saved;
                    self.extra(BuildingKind::COAL_MINE, fields.anchor, 16, think);
                }
                return true;
            }
            _ => {}
        }
        false
    }

    /// Asks for beer at `at` when breweries are no more than half the armouries, rounded up, and
    /// there are none or the stock is under 30 (`0x6d9f6`).
    fn ask_for_beer(&mut self, at: Node, think: &Think<'_>) {
        let breweries = think.built(BuildingKind::BREWERY);
        if breweries > think.built(BuildingKind::ARMOURY).div_ceil(2) {
            return;
        }
        if breweries != 0 && think.game.world.owned_wares(think.player, WareKind::BEER) >= BEER_STOCK {
            return;
        }
        let at = (at.x.cast_signed(), at.y.cast_signed());
        self.tasks.board.add(BEER, at, think.profile.add_wait, false);
    }

    /// The gold chain: a gold mine, a mint, a coal mine by the mine and now and then a
    /// guardhouse and a watchtower.
    fn gold(&mut self, index: u16, fields: &mut Progress, think: &mut Think<'_>) -> bool {
        match fields.step {
            0 => self.next(index, BuildingKind::GOLD_MINE, fields.anchor, 16, think),
            1 => {
                if think.game.rng.up_to(100) < think.profile.guards[2] {
                    self.extra(BuildingKind::GUARDHOUSE, fields.anchor, 20, think);
                }
                fields.saved = fields.anchor;
                self.next(index, BuildingKind::MINT, fields.anchor, 24, think);
            }
            2 => {
                if think.game.rng.up_to(100) < think.profile.guards[3] {
                    self.extra(BuildingKind::WATCHTOWER, fields.anchor, 15, think);
                }
                fields.anchor = fields.saved;
                self.extra(BuildingKind::COAL_MINE, fields.anchor, 25, think);
                return true;
            }
            _ => {}
        }
        false
    }

    /// The food chains: a farm with a mill, a bakery and a well; a farm with a pig farm, a well
    /// and a slaughterhouse; or a fishery. Variants past 2 leave the project idle for good.
    fn food(&mut self, index: u16, fields: &Progress, think: &Think<'_>) -> bool {
        let at = fields.anchor;
        let wooded = fields.options & WITH_WOODCUTTER != 0;
        match (fields.options & u16::from(u8::MAX), fields.step) {
            (FARM_AND_MILL | FARM_AND_PIGS, 0) => self.next(index, BuildingKind::FARM, at, 16, think),
            (variant @ (FARM_AND_MILL | FARM_AND_PIGS), 1) => {
                if wooded {
                    self.extra(BuildingKind::WOODCUTTER, at, 8, think);
                }
                let kind = if variant == FARM_AND_MILL {
                    BuildingKind::MILL
                } else {
                    BuildingKind::PIG_FARM
                };
                self.next(index, kind, at, 24, think);
                self.extra(BuildingKind::FARM, at, 16, think);
            }
            (FARM_AND_MILL, 2) => self.next(index, BuildingKind::BAKERY, at, 16, think),
            (FARM_AND_MILL, 3) => {
                self.extra(BuildingKind::WELL, at, 16, think);
                return true;
            }
            (FARM_AND_PIGS, 2) => {
                self.extra(BuildingKind::WELL, at, 16, think);
                self.next(index, BuildingKind::SLAUGHTERHOUSE, at, 16, think);
            }
            (FISH, 0) => self.next(index, BuildingKind::FISHERY, at, 10, think),
            (FARM_AND_PIGS, 3) | (FISH, 1) => return true,
            _ => {}
        }
        false
    }

    /// The beer chain: a farm, a brewery, another farm and a well.
    fn beer(&mut self, index: u16, fields: &Progress, think: &Think<'_>) -> bool {
        let at = fields.anchor;
        match fields.step {
            0 => self.next(index, BuildingKind::FARM, at, 16, think),
            1 => {
                if fields.options & WITH_WOODCUTTER != 0 {
                    self.extra(BuildingKind::WOODCUTTER, at, 8, think);
                }
                self.next(index, BuildingKind::BREWERY, at, 24, think);
            }
            2 => {
                self.extra(BuildingKind::FARM, at, 16, think);
                self.extra(BuildingKind::WELL, at, 12, think);
                return true;
            }
            _ => {}
        }
        false
    }

    /// The coal chain: a coal mine.
    fn coal(&mut self, index: u16, fields: &Progress, think: &Think<'_>) -> bool {
        match fields.step {
            0 => self.next(index, BuildingKind::COAL_MINE, fields.anchor, 20, think),
            1 => return true,
            _ => {}
        }
        false
    }

    /// The donkey chain: a donkey breeder, two farms and a well.
    fn donkeys(&mut self, index: u16, fields: &mut Progress, think: &Think<'_>) -> bool {
        let at = fields.anchor;
        match fields.step {
            0 => self.next(index, BuildingKind::DONKEY_BREEDER, at, 16, think),
            1 => {
                fields.saved = at;
                self.extra(BuildingKind::FARM, at, 16, think);
                if fields.options & WITH_WOODCUTTER != 0 {
                    self.extra(BuildingKind::WOODCUTTER, at, 6, think);
                }
                self.extra(BuildingKind::FARM, at, 16, think);
                self.extra(BuildingKind::WELL, at, 12, think);
                return true;
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests;
