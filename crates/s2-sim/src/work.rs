//! A manned workplace's work cycle on the building tick: starting work, counting its ticks,
//! finishing, and carrying the finished ware out to the flag.

use crate::figures::{Figure, FigureKind, Porter};
use crate::game::Game;
use crate::grid::Direction;
use crate::job::Job;
use crate::settlers::DOOR_SPEED;
use crate::transport::{SearchOptions, Ware, search};
use crate::ware::WareKind;
use crate::workplace::WORKER_MODE;
use crate::world::{BuildingKind, BuildingState, Category};

/// How far along the roads a new donkey's storehouse may be (`0x2d563`).
const STOREHOUSE_REACH: u16 = 250;

/// Types whose door opens when they start work (`0x2ddbe`) and closes when their animation
/// ends (`0x2fcf3`).
pub(crate) const WORK_DOORS: [BuildingKind; 4] = [
    BuildingKind::PIG_FARM,
    BuildingKind::DONKEY_BREEDER,
    BuildingKind::MILL,
    BuildingKind::BAKERY,
];

/// The building animation each nation's building of each type plays while it works
/// (`0x1fa7a8`, 64 bytes per nation); `None` for types without one.
const WORK_ANIMATIONS: [[u16; 32]; 4] = [
    [
        NONE, NONE, NONE, NONE, NONE, 147, NONE, 1026, 3446, 1163, 1336, 1621, 345, 1783, 49, 273, NONE, 0,
        304, 304, 304, 304, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, 3544, NONE,
    ],
    [
        NONE, NONE, NONE, NONE, NONE, 3778, NONE, 1026, 3446, 3935, 1336, 3965, 3881, 4033, 3738, 3809, NONE,
        0, 3840, 3840, 3840, 3840, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, 4108, NONE,
    ],
    [
        NONE, NONE, NONE, NONE, NONE, 4286, NONE, 1026, 3446, 4443, 4473, 4531, 4389, 4599, 4246, 4317, NONE,
        0, 4348, 4348, 4348, 4348, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, 4674, NONE,
    ],
    [
        NONE, NONE, NONE, NONE, NONE, 4852, NONE, 1026, 5190, 5009, 1336, 5039, 4955, 5126, 4812, 4883, NONE,
        0, 4914, 4914, 4914, 4914, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, 5231, NONE,
    ],
];

/// The table's "no animation" word.
const NONE: u16 = u16::MAX;

/// How a worker looks carrying each type's ware out (`0x2d6de`), by building type. The
/// metalworks and the armoury look by the ware instead (see [`carry_look`]); types that never
/// carry a ware out have 0.
const CARRY_LOOKS: [u8; 32] = [
    0, 0, 0, 0, 0, 39, 0, 50, 50, 51, 52, 53, 46, 0, 0, 41, 68, 31, 42, 43, 44, 45, 0, 0, 0, 0, 0, 0, 0, 0,
    71, 0,
];

/// The metalworks' look for each tool from ware 16 up (`0x2d6b6`), and for the two after them.
const TOOL_LOOKS: [u8; 10] = [55, 57, 66, 58, 56, 59, 60, 61, 62, 64];
const LAST_TOOLS_LOOK: Job = Job(65);

/// The armoury's looks carrying a sword and a shield.
const SWORD_LOOK: Job = Job(33);
const SHIELD_LOOK: Job = Job(34);

/// How the worker of a building of type `kind` looks carrying `ware` out (`0x2d808`).
fn carry_look(kind: BuildingKind, ware: WareKind) -> Job {
    match kind {
        BuildingKind::METALWORKS => ware
            .tool()
            .and_then(|tool| TOOL_LOOKS.get(tool))
            .map_or(LAST_TOOLS_LOOK, |&look| Job(look)),
        BuildingKind::ARMOURY if ware == WareKind::SWORD => SWORD_LOOK,
        BuildingKind::ARMOURY if ware == WareKind::SHIELD => SHIELD_LOOK,
        _ => Job(CARRY_LOOKS.get(kind.index()).copied().unwrap_or(0)),
    }
}

impl Game {
    /// The work cycle of manned workplace `record` on a building tick (`0x2d35e`): carry a
    /// finished ware out; or count a tick of work and finish once the worker is in; or start.
    /// Each adds to the productivity tally.
    pub(crate) fn work_tick(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let state = building.state;
        let added = if state.contains(BuildingState::OUTPUT) {
            self.carry_out(record);
            1
        } else if state.contains(BuildingState::WORKING) {
            let duration = self
                .world
                .building_types()
                .get(building.kind)
                .map_or(0, |row| row.duration);
            let ticks = building.timer.saturating_add(1);
            if ticks >= duration && state.contains(BuildingState::PRESENT) {
                self.set_timer(record, ticks);
                self.finish_work(record);
            } else if ticks >= duration {
                self.set_timer(record, ticks - 1);
            } else {
                self.set_timer(record, ticks);
            }
            2
        } else if !state.contains(BuildingState::STOPPED)
            && state.contains(BuildingState::PRESENT)
            && self.start_work(record)
        {
            2
        } else {
            0
        };
        if let Some(building) = self.world.building_mut(record) {
            building.tally = building.tally.saturating_add(added);
        }
    }

    fn set_timer(&mut self, record: u16, ticks: u8) {
        if let Some(building) = self.world.building_mut(record) {
            building.timer = ticks;
        }
    }

    /// Starts work at `record` (`0x2dc9a`). Returns whether it started.
    fn start_work(&mut self, record: u16) -> bool {
        self.set_timer(record, 0);
        let Some(kind) = self.world.building(record).map(|building| building.kind) else {
            return false;
        };
        match kind {
            BuildingKind::WOODCUTTER => self.start_woodcutter(record),
            BuildingKind::FORESTER => self.start_forester(record),
            BuildingKind::QUARRY => self.start_quarry(record),
            BuildingKind::FISHERY => self.start_fishery(record),
            BuildingKind::WELL => self.start_well(record),
            BuildingKind::HUNTER => self.start_hunter(record),
            _ if kind.is_mine() => self.start_mine(record),
            BuildingKind::FARM => self.start_farm(record),
            BuildingKind::CATAPULT => self.start_catapult(record),
            BuildingKind::SHIPYARD => self.start_shipyard(record),
            _ => self.start_generic(record),
        }
    }

    /// The generic start (`0x2ddbe`): with one of each input in stock, uses them, takes each off
    /// the owner's ware count (`0x2de26`), starts working and starts the building's animation for
    /// its owner's nation.
    pub(crate) fn start_generic(&mut self, record: u16) -> bool {
        let Some(building) = self.world.building(record).copied() else {
            return false;
        };
        let Some(row) = self.world.building_types().get(building.kind) else {
            return false;
        };
        let used: Vec<WareKind> = row.inputs.iter().map_while(|&input| input).collect();
        let inputs = used.len();
        if building.stock[..inputs].contains(&0) {
            return false;
        }
        if let Some(building) = self.world.building_mut(record) {
            for stock in &mut building.stock[..inputs] {
                *stock -= 1;
            }
            building.state |= BuildingState::WORKING;
        }
        for ware in used {
            self.world.drop_ware(building.owner, ware, 1);
        }
        self.start_work_animation(record);
        true
    }

    /// Donkey breeder `record` has bred a pack donkey (`0x2d563`). It goes to the first
    /// finished storehouse the roads reach from the breeder's flag, the flag itself included,
    /// for an order there that counts as already served; with none it is lost outside the
    /// breeder. The breeder puts nothing in an output slot and starts again on its next tick.
    fn breed_donkey(&mut self, record: u16) {
        let Some(breeder) = self.world.building(record).copied() else {
            return;
        };
        let Some((owner, flag_node)) = self.world.flag(breeder.flag).map(|flag| (flag.owner, flag.node))
        else {
            return;
        };
        self.world.count_people(owner, Job::PACK_DONKEY, 1);
        let store = search(
            &self.world,
            breeder.flag,
            STOREHOUSE_REACH,
            SearchOptions::empty(),
            |reached| {
                self.world
                    .building_at_flag(reached.flag)
                    .and_then(|store| self.world.building(store))
                    .is_some_and(|store| {
                        store.is_storehouse() && store.state.contains(BuildingState::FINISHED)
                    })
            },
        );
        let Some(store) = store else {
            self.lost_at(owner, breeder.node, Job::PACK_DONKEY);
            return;
        };
        let order = self
            .world
            .order_person(store.flag, Job::PACK_DONKEY, WORKER_MODE, 0);
        if let Some(open) = order.and_then(|order| self.world.orders.get_mut(order)) {
            open.delivering = true;
        }
        let mut donkey = Figure::fresh(FigureKind::Leaving {
            owner,
            job: Job::PACK_DONKEY,
            order,
        });
        donkey.start_walk(Direction::SouthEast, DOOR_SPEED);
        self.world.add_figure(flag_node, donkey);
        if let Some(breeder) = self.world.building_mut(record) {
            breeder.state |= BuildingState::DOOR;
        }
    }

    /// Starts the building animation of `record`'s type for its owner's nation, from the table
    /// at `0x1fa7a8`, and runs its first step. A type with none keeps whatever it had. Some
    /// types open their door for it.
    pub(crate) fn start_work_animation(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let nation = self.world.nations[building.owner.index()];
        let Some(start) = WORK_ANIMATIONS
            .get(usize::from(nation))
            .and_then(|table| table.get(building.kind.index()))
            .copied()
            .filter(|&start| start != NONE)
        else {
            return;
        };
        let animation = self.start_animation_at(start, building.node);
        if let Some(building) = self.world.building_mut(record) {
            building.animation = animation;
            if WORK_DOORS.contains(&building.kind) {
                building.state |= BuildingState::DOOR;
            }
        }
    }

    /// Finishes work at `record` (`0x2d429`): a gatherer's worker has brought its ware out
    /// already and a shipyard in ship mode makes nothing; any other workplace puts what it made
    /// in its first empty output slot, or drops it with none free, and has a ware to carry out.
    fn finish_work(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        if let Some(building) = self.world.building_mut(record) {
            building.state -= BuildingState::WORKING;
            building.timer = 0;
        }
        if building.kind.is_gatherer() || (building.kind == BuildingKind::SHIPYARD && building.ships) {
            return;
        }
        if building.kind == BuildingKind::DONKEY_BREEDER {
            self.breed_donkey(record);
            return;
        }
        let output = self.output_of(record);
        let building_owner = building.owner;
        let Some(building) = self.world.building_mut(record) else {
            return;
        };
        if let Some(ware) = output
            && let Some(slot) = building.outputs.iter_mut().find(|slot| slot.is_none())
        {
            *slot = Some(ware);
        }
        building.state |= BuildingState::OUTPUT;
        if let Some(ware) = output {
            self.world.count_ware(building_owner, ware, 1);
        }
    }

    /// What workplace `record` makes this time: the armoury turns between shields and swords,
    /// shields first; the shipyard makes boats; the rest make their type's ware.
    fn output_of(&mut self, record: u16) -> Option<WareKind> {
        let building = self.world.building_mut(record)?;
        match building.kind {
            BuildingKind::ARMOURY => {
                let swords = building.state.contains(BuildingState::SWORDS_NEXT);
                building.state.toggle(BuildingState::SWORDS_NEXT);
                Some(if swords { WareKind::SWORD } else { WareKind::SHIELD })
            }
            BuildingKind::SHIPYARD => Some(WareKind::BOAT),
            BuildingKind::METALWORKS => {
                let owner = building.owner;
                Some(self.pick_tool(owner))
            }
            kind => self.world.building_types().get(kind)?.output,
        }
    }

    /// The tool a metalworks of `owner` makes (`0x2d497`): one draw from 0 to the sum of the
    /// player's tool weights picks it ([`tool_for`]).
    fn pick_tool(&mut self, owner: crate::world::Player) -> WareKind {
        let weights = *self.world.distribution[owner.index()].tools();
        let sum: u16 = weights.iter().map(|&weight| u16::from(weight)).sum();
        tool_for(&weights, self.rng.up_to(sum))
    }

    /// Workplace `record` sends its first finished ware out with a new figure (`0x2d6de`,
    /// `0x36bc8`), once its worker is in. With none left it has nothing more to carry out.
    fn carry_out(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        if !building.state.contains(BuildingState::PRESENT) {
            return;
        }
        let Some(ware) = building.outputs.iter().flatten().next().copied() else {
            if let Some(building) = self.world.building_mut(record) {
                building.state -= BuildingState::OUTPUT;
            }
            return;
        };
        let Some(flag) = self.world.flag(building.flag).map(|flag| flag.node) else {
            return;
        };
        let mut figure = Figure::fresh(FigureKind::Porter(Porter {
            owner: building.owner,
            job: carry_look(building.kind, ware),
            building: record,
            ware: Some(ware),
            inside: false,
        }));
        figure.start_walk(Direction::SouthEast, DOOR_SPEED);
        self.world.add_figure(flag, figure);
        if let Some(building) = self.world.building_mut(record) {
            if let Some(slot) = building.outputs.iter_mut().find(|slot| slot.is_some()) {
                *slot = None;
            }
            building.state |= BuildingState::DOOR;
            building.state -= BuildingState::PRESENT;
        }
    }

    /// One step of porter `record` that is not walking. At the flag (`0x3648c`) it puts its
    /// ware out and turns back in; a soldier sent out of a military building goes home instead. Inside (`0x36007`) its workplace has its worker back and the
    /// figure is gone. A ware the flag refused goes back into an output slot, and only then is
    /// the output bit cleared while a slot is still empty (`0x2d88a`); otherwise the next
    /// carry-out tick clears it, one tick later.
    pub(crate) fn step_porter(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Porter(porter) = figure.kind else {
            return;
        };
        let Some(building) = self.world.building(porter.building).copied() else {
            self.make_lost(record);
            return;
        };
        if porter.inside {
            if let Some(site) = self.world.building_mut(porter.building) {
                site.state -= BuildingState::DOOR;
                site.state |= BuildingState::PRESENT;
                if let Some(ware) = porter.ware {
                    if let Some(slot) = site.outputs.iter_mut().find(|slot| slot.is_none()) {
                        *slot = Some(ware);
                    }
                    if site.outputs.contains(&None) {
                        site.state -= BuildingState::OUTPUT;
                    }
                }
            }
            self.world.remove_figure(record);
            return;
        }
        if let Some(site) = self.world.building_mut(porter.building) {
            site.state -= BuildingState::DOOR;
        }
        if porter.job.is_soldier() && porter.ware.is_none() {
            self.soldier_leaves(record, porter.owner, porter.job, building.flag);
            return;
        }
        let worker = self
            .world
            .building_types()
            .get(building.kind)
            .and_then(|row| row.worker)
            .unwrap_or_default();
        let placed = porter.ware.is_some_and(|ware| self.put_out(building.flag, ware));
        if let Some(FigureKind::Porter(porter)) = self.figure_kind_mut(record) {
            if placed {
                porter.ware = None;
                porter.job = worker;
            }
            porter.inside = true;
        }
        self.world.move_figure(record, building.node);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(Direction::NorthWest, DOOR_SPEED);
        }
    }

    /// The input order pass (`0x2bcdc`), every 240 steps: each finished workplace or military
    /// building in record order, unless it is stopped, a mine that has given up or a military
    /// building whose soldiers inside are all generals, orders one ware for the input
    /// with the least in stock and on its way below its capacity, the first among equals. It
    /// orders only when that input is empty or nothing it ordered is still on its way.
    pub(crate) fn order_inputs(&mut self) {
        let types = self.world.building_types();
        let wanting: Vec<(u16, usize, WareKind)> = self
            .world
            .buildings()
            .filter_map(|(record, building)| {
                let row = types.get(building.kind)?;
                let all_generals = row.category == Category::Military
                    && self
                        .world
                        .military_of(record)
                        .and_then(|(_, military)| military.weakest())
                        == Some(Job::GENERAL);
                if !matches!(row.category, Category::Workplace | Category::Military)
                    || all_generals
                    || !building.state.contains(BuildingState::FINISHED)
                    || building.state.contains(BuildingState::STOPPED)
                    || (building.kind.is_mine() && building.exhausted)
                {
                    return None;
                }
                let mut least: Option<(usize, u8, WareKind)> = None;
                for (slot, input) in row.inputs.iter().enumerate() {
                    let Some(ware) = *input else {
                        break;
                    };
                    let held = building.stock[slot] + building.on_way[slot];
                    if held < row.capacity[slot] && least.is_none_or(|(_, fewest, _)| held < fewest) {
                        least = Some((slot, held, ware));
                    }
                }
                let (slot, held, ware) = least?;
                (held == 0 || building.requested == 0).then_some((record, slot, ware))
            })
            .collect();
        for (record, slot, ware) in wanting {
            let Some(building) = self.world.building_mut(record) else {
                continue;
            };
            building.on_way[slot] += 1;
            building.requested += 1;
            let flag = building.flag;
            self.world.order_ware(flag, ware, 0);
        }
    }

    /// Puts a new ware of type `ware` out at flag `flag` (`0x2d8ef`): it is sent to the best
    /// destination within reach, or is stranded with none. Returns false when the flag is full:
    /// the ware gives its order up and is gone.
    pub(crate) fn put_out(&mut self, flag: u16, ware: WareKind) -> bool {
        let Some(record) = self.world.add_ware(Ware::new(ware)) else {
            return false;
        };
        if let Some(to) = self.world.destination_for(flag, ware)
            && self.claim_expedition_order(record, to).is_none()
        {
            self.world.send_ware(record, to);
        }
        if self.world.put_down(record, flag) {
            return true;
        }
        self.world.give_up_order(record);
        self.world.wares.remove(record);
        false
    }
}

/// The tool a roll of `roll` picks from `weights`, the tools' weights from ware 16 up: the first
/// whose span, from the weights before it to those plus its own, holds the roll, both ends
/// included. The first tool wins when none does.
pub(crate) fn tool_for(weights: &[u8], roll: u16) -> WareKind {
    let mut before = 0_u16;
    for (tool, &weight) in (WareKind::FIRST_TOOL.0..).zip(weights) {
        let weight = u16::from(weight);
        if before <= roll && roll <= before + weight {
            return WareKind(tool);
        }
        before += weight;
    }
    WareKind::FIRST_TOOL
}

#[cfg(test)]
mod tests;
