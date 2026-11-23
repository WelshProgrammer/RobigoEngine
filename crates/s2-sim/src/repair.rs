//! Brings a saved state back into line with tables it was not made with.

use crate::game::Game;
use crate::grid::Node;
use crate::job::Job;
use crate::ware::WareKind;
use crate::workplace::WORKER_MODE;
use crate::world::{BuildingKind, BuildingState, BuildingType, BuildingTypes, Category, Player};

/// One thing a repair took out of the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repaired {
    /// The type of the building it was taken from.
    pub kind: BuildingKind,
    /// Where the building stands, or stood.
    pub node: Node,
    /// Whose it is.
    pub owner: Player,
    /// Whether the building is a site.
    pub site: bool,
    /// What was taken.
    pub what: Repair,
}

/// What a repair took from one building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    /// The building's type is gone from the table or is of another category, and the building
    /// was demolished.
    Removed,
    /// A workplace's type has another worker. The old one left as a lost settler, every order at
    /// the flag went, and the new worker is on order.
    Worker,
    /// A stopped workplace's type can no longer be stopped, and it works again.
    Restarted,
    /// An input slot holds another ware now, and what it held is gone.
    WrongStock {
        /// The ware it held, when the old type had one in the slot.
        ware: Option<WareKind>,
        /// How many went.
        lost: u8,
    },
    /// An input slot held more than its type now stocks, and lost the rest.
    Overstock {
        /// The ware.
        ware: WareKind,
        /// How many it keeps.
        kept: u8,
    },
    /// The building had wares on order that it no longer takes or has no room for. The orders
    /// are gone, and a ware already on its way is stranded as one for a burnt building is.
    Orders {
        /// How many orders went.
        cancelled: u8,
    },
    /// A military building held more soldiers than its type now does, and the rest left.
    Soldiers {
        /// How many left.
        left: u8,
    },
    /// A site held more boards or stones than its type now costs, and lost the rest.
    Materials {
        /// How many wares it lost.
        lost: u8,
    },
}

/// Why a state cannot be repaired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeyondRepair {
    /// A building stands whose type takes another size of site. The ground it stands on and
    /// what stands round it were settled by its size.
    Size(BuildingKind),
    /// A finished building holds land whose type claims another reach of it. The land it holds
    /// was claimed by the old reach and would be released by the new.
    Strength(BuildingKind),
}

/// The list a repair fills for one building.
struct Found<'a> {
    kind: BuildingKind,
    node: Node,
    owner: Player,
    site: bool,
    list: &'a mut Vec<Repaired>,
}

impl Found<'_> {
    fn push(&mut self, what: Repair) {
        self.list.push(Repaired {
            kind: self.kind,
            node: self.node,
            owner: self.owner,
            site: self.site,
            what,
        });
    }
}

impl Game {
    /// Why [`Self::repair`] cannot bring the state in line with the game's building types,
    /// for the first building in record order it cannot: one whose type takes another size of
    /// site than in `saved`, the table the state was made with, or a finished one whose type
    /// has another strength. A military building nobody has occupied holds no land yet, and
    /// its strength may change.
    #[must_use]
    pub fn beyond_repair(&self, saved: &BuildingTypes) -> Option<BeyondRepair> {
        let types = self.world.building_types();
        self.world.buildings().find_map(|(_, building)| {
            let (then, now) = (saved.get(building.kind)?, types.get(building.kind)?);
            let holds_land = building.state.contains(BuildingState::FINISHED)
                && (now.category != Category::Military || building.state.contains(BuildingState::MANNED));
            if then.size != now.size {
                Some(BeyondRepair::Size(building.kind))
            } else if holds_land && then.strength != now.strength {
                Some(BeyondRepair::Strength(building.kind))
            } else {
                None
            }
        })
    }

    /// Takes out of the state whatever the building types no longer allow, building by building
    /// in record order, and returns what went. `saved` is the table the state was made with; a
    /// state made with the game's own table comes back unchanged.
    ///
    /// A building whose type is gone or has changed category is demolished. A workplace whose
    /// type has another worker loses the old one and orders the new, and one stopped whose type
    /// can no longer be stopped works again. A finished workplace or
    /// military building loses the stock of a slot whose ware changed and any stock above its
    /// capacity, and its orders for wares it does not take or has no room for. A military
    /// building keeps as many soldiers as its type holds. A site keeps as many boards and stones
    /// as its type costs, and orders more when the cost has risen.
    pub fn repair(&mut self, saved: &BuildingTypes) -> Vec<Repaired> {
        let mut repaired = Vec::new();
        let records: Vec<u16> = self.world.buildings().map(|(record, _)| record).collect();
        for record in records {
            let Some(building) = self.world.building(record).copied() else {
                continue;
            };
            let finished = building.state.contains(BuildingState::FINISHED);
            let mut found = Found {
                kind: building.kind,
                node: building.node,
                owner: building.owner,
                site: !finished,
                list: &mut repaired,
            };
            let now = self.world.building_types().get(building.kind);
            let then = saved.get(building.kind).or(now);
            let (Some(now), Some(then)) = (now, then) else {
                self.demolish(record);
                found.push(Repair::Removed);
                continue;
            };
            if !finished {
                if building.state.contains(BuildingState::BUILDING) {
                    self.repair_site(record, then, now, &mut found);
                }
                continue;
            }
            if then.category != now.category {
                self.demolish(record);
                found.push(Repair::Removed);
                continue;
            }
            match now.category {
                Category::Workplace => {
                    if then.worker != now.worker {
                        self.replace_worker(record, then.worker, now.worker);
                        found.push(Repair::Worker);
                    }
                    if !now.stoppable && building.state.contains(BuildingState::STOPPED) {
                        if let Some(building) = self.world.building_mut(record) {
                            building.state -= BuildingState::STOPPED;
                        }
                        found.push(Repair::Restarted);
                    }
                    self.repair_inputs(record, then, now, &mut found);
                }
                Category::Military => {
                    self.repair_inputs(record, then, now, &mut found);
                    self.repair_soldiers(record, now, &mut found);
                }
                Category::Storehouse => {}
            }
        }
        repaired
    }

    /// Workplace `record`'s worker of job `old` leaves as the worker of a burning building does,
    /// with every order at the flag settled, and the building waits for one of job `new` as a
    /// newly finished one does.
    fn replace_worker(&mut self, record: u16, old: Option<Job>, new: Option<Job>) {
        self.vacate_of(record, old);
        self.stop_building_animation(record);
        let Some(building) = self.world.building_mut(record) else {
            return;
        };
        building.state -= BuildingState::MANNED | BuildingState::PRESENT;
        building.state |= BuildingState::DOOR;
        building.on_way = [0; 4];
        building.requested = 0;
        building.exhausted = false;
        building.productivity = 0;
        building.tally = 0;
        let flag = building.flag;
        if let Some(job) = new {
            self.world.order_person(flag, job, WORKER_MODE, 0);
        }
    }

    /// Brings finished building `record`'s input stock and its ware orders within its type's
    /// row `now`, from the row `then` it was saved with.
    fn repair_inputs(&mut self, record: u16, then: BuildingType, now: BuildingType, found: &mut Found<'_>) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let room: [u8; 4] = std::array::from_fn(|slot| now.inputs[slot].map_or(0, |_| now.capacity[slot]));
        let mut stock = building.stock;
        for slot in 0..room.len() {
            let same = then.inputs[slot] == now.inputs[slot];
            let kept = if same { room[slot] } else { 0 };
            let over = stock[slot].saturating_sub(kept);
            if over == 0 {
                continue;
            }
            stock[slot] = kept;
            if let Some(ware) = then.inputs[slot] {
                self.world.drop_ware(building.owner, ware, u16::from(over));
            }
            match now.inputs[slot] {
                Some(ware) if same => found.push(Repair::Overstock { ware, kept }),
                _ => found.push(Repair::WrongStock {
                    ware: then.inputs[slot],
                    lost: over,
                }),
            }
        }
        let mut on_way = [0; 4];
        let mut cancelled = 0;
        for order in self.ware_orders(building.flag) {
            let slot = self
                .world
                .order(order)
                .and_then(crate::transport::Order::ware)
                .and_then(|ware| now.inputs.iter().position(|&input| input == Some(ware)));
            if let Some(slot) = slot
                && stock[slot] + on_way[slot] < room[slot]
            {
                on_way[slot] += 1;
                continue;
            }
            self.cancel_ware_order(order);
            cancelled += 1;
        }
        if let Some(building) = self.world.building_mut(record) {
            building.stock = stock;
            building.on_way = on_way;
            building.requested = building.requested.saturating_sub(cancelled);
        }
        if cancelled > 0 {
            found.push(Repair::Orders { cancelled });
        }
    }

    /// Brings military building `record`'s soldiers within its type's row `now`.
    fn repair_soldiers(&mut self, record: u16, now: BuildingType, found: &mut Found<'_>) {
        let Some((military, held)) = self
            .world
            .military_of(record)
            .map(|(military, held)| (military, *held))
        else {
            return;
        };
        if held.capacity == now.soldiers {
            return;
        }
        let left = self.cap_soldiers(military, now.soldiers);
        if left > 0 {
            found.push(Repair::Soldiers { left });
        }
    }

    /// Brings site `record`'s boards and stones in line with its type's row `now`, from the row
    /// `then` it was saved with. With a lower cost, what the site has used and holds is cut to
    /// it and the orders past it go. With a higher cost, a site that has ordered its wares
    /// orders the difference.
    fn repair_site(&mut self, record: u16, then: BuildingType, now: BuildingType, found: &mut Found<'_>) {
        let Some(site) = self.world.building(record).copied() else {
            return;
        };
        let (mut lost, mut cancelled) = (0, 0);
        for (index, &ware) in WareKind::CONSTRUCTION.iter().enumerate() {
            let cost = now.cost(index);
            let risen = cost.saturating_sub(then.cost(index));
            if risen > 0 && site.requested >= crate::supply::WARES_ORDERED {
                for _ in 0..risen {
                    self.world.order_ware(site.flag, ware, 0);
                    if let Some(site) = self.world.building_mut(record) {
                        site.requested = site.requested.saturating_add(1);
                    }
                }
            }
            if cost >= then.cost(index) {
                continue;
            }
            let used = site.used[index].min(cost);
            let stock = site.stock[index].min(cost - used);
            let gone = site.used[index] + site.stock[index] - used - stock;
            let mut planned = used + stock;
            let mut recalled = 0;
            for order in self.ware_orders(site.flag) {
                let Some(held) = self.world.order(order).copied() else {
                    continue;
                };
                if held.ware() != Some(ware) {
                    continue;
                }
                if planned < cost {
                    planned += 1;
                    continue;
                }
                recalled += u8::from(held.delivering);
                self.cancel_ware_order(order);
                cancelled += 1;
            }
            lost += gone;
            if let Some(site) = self.world.building_mut(record) {
                site.used[index] = used;
                site.stock[index] = stock;
                site.on_way[index] = site.on_way[index].saturating_sub(gone + recalled);
                site.requested = site.requested.saturating_sub(recalled);
            }
        }
        if lost > 0 {
            found.push(Repair::Materials { lost });
        }
        if cancelled > 0 {
            found.push(Repair::Orders { cancelled });
        }
    }

    /// The orders for wares at flag `flag`, in record order.
    fn ware_orders(&self, flag: u16) -> Vec<u16> {
        self.world
            .orders()
            .filter(|(_, order)| order.flag == flag && order.ware().is_some())
            .map(|(order, _)| order)
            .collect()
    }

    /// Frees ware order `order`. A ware already on its way to it is stranded.
    fn cancel_ware_order(&mut self, order: u16) {
        if let Some(held) = self.world.order(order).copied()
            && held.delivering
            && let Some(ware) = held.on_way
        {
            self.strand_ordered(ware);
        }
        self.world.release_link(order);
        self.world.orders.remove(order);
    }
}

#[cfg(test)]
mod tests;
