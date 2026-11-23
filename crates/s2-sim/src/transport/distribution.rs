//! Where a new or stranded ware goes: each player's distribution settings, the demand score a
//! building gives a ware, and the search for the best destination.
//!
//! Not yet modelled: military buildings, which score and order coins by their distance to the
//! enemy.

use crate::economy::EconomyTables;
use crate::transport::{MAX_ORDERS, Order, OrderKind, SearchOptions, WARE_MODE, Wanted, WareState, search};
use crate::ware::WareKind;
use crate::world::{BUILDING_TYPES, BuildingKind, BuildingState, Category, WARE_TYPES, World};

/// How far a destination is looked for (`0x2d8ef`, `0x2a7e4`).
const DESTINATION_LIMIT: u16 = 245;

/// The search stops after this many buildings have scored.
const SCORED_ENOUGH: u8 = 10;

/// What a storehouse scores for any ware it takes.
const STOREHOUSE_SCORE: u8 = 5;

/// What a harbour preparing an expedition scores for boards and stones.
const EXPEDITION_SCORE: u8 = 60;

/// The wares a storehouse wants more of when it holds fewer than the mean of the other two:
/// beer, swords and shields.
const BALANCED: [WareKind; 3] = [WareKind::BEER, WareKind::SWORD, WareKind::SHIELD];

/// What each ware short of that mean adds to the score.
const SHORTFALL_WEIGHT: i32 = 5;

/// The highest score.
const MOST: u8 = 100;

/// What each border class adds to a military building's demand and coin order urgency.
const MILITARY_URGENCY: u8 = 40;

/// A full input scores nothing.
const FULL: u32 = 100;

/// One player's distribution settings (the 1,116-byte record at `0x3ceda`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Distribution {
    /// The weight each building type gives each ware (`+0x000`, `[type][ware]`).
    weights: [[u8; WARE_TYPES]; BUILDING_TYPES],
    /// The weight a site gives each ware (`+0x3e0`); only boards and stones are read.
    construction: [u8; WARE_TYPES],
    /// The metalworks' weight for each tool (`+0x3ff`).
    tools: [u8; WareKind::TOOLS],
}

impl Default for Distribution {
    fn default() -> Self {
        Self::new(&EconomyTables::ORIGINAL)
    }
}

impl Distribution {
    /// The settings every player starts a game with (`0x3cb82`, `0x3ce2c`): each building's
    /// weight for any ware is its type's.
    #[must_use]
    pub fn new(tables: &EconomyTables) -> Self {
        let mut weights = [[0; WARE_TYPES]; BUILDING_TYPES];
        for (kind, row) in (0..).zip(&mut weights) {
            *row = [tables.type_weight(BuildingKind(kind)); WARE_TYPES];
        }
        Self {
            weights,
            construction: [tables.construction(); WARE_TYPES],
            tools: *tables.tools(),
        }
    }

    /// The weight building type `kind` gives ware `ware`.
    #[must_use]
    pub fn weight(&self, kind: BuildingKind, ware: WareKind) -> u8 {
        self.weights
            .get(kind.index())
            .and_then(|row| row.get(ware.index()))
            .copied()
            .unwrap_or(0)
    }

    /// The weight a site gives ware `ware`.
    #[must_use]
    pub fn construction(&self, ware: WareKind) -> u8 {
        self.construction.get(ware.index()).copied().unwrap_or(0)
    }

    /// The metalworks' weights for the tools, from ware 16 up.
    #[must_use]
    pub const fn tools(&self) -> &[u8; WareKind::TOOLS] {
        &self.tools
    }

    /// Sets the weight building type `kind` gives ware `ware`.
    pub(crate) fn set_weight(&mut self, kind: BuildingKind, ware: WareKind, weight: u8) {
        if let Some(cell) = self
            .weights
            .get_mut(kind.index())
            .and_then(|row| row.get_mut(ware.index()))
        {
            *cell = weight;
        }
    }

    /// Sets the weight a site gives ware `ware`.
    pub(crate) fn set_construction(&mut self, ware: WareKind, weight: u8) {
        if let Some(cell) = self.construction.get_mut(ware.index()) {
            *cell = weight;
        }
    }

    /// The metalworks' weights for the tools, to change.
    pub(crate) const fn tools_mut(&mut self) -> &mut [u8; WareKind::TOOLS] {
        &mut self.tools
    }
}

impl World {
    /// The urgency a new order for ware `ware` at flag `flag` starts with (`0x2afa0`,
    /// `0x2b12c`): 10 with no building there, a site's construction weight, or the building's
    /// weight for the ware. A finished military building's order starts at 40 per border
    /// class, or 100 when that and the weight pass 100: `S2.EXE` drops the weight otherwise.
    pub(crate) fn order_urgency(&self, flag: u16, ware: WareKind) -> Option<u8> {
        let owner = self.flag(flag)?.owner;
        let settings = &self.distribution[owner.index()];
        let Some(building) = self
            .building_at_flag(flag)
            .and_then(|building| self.building(building))
        else {
            return Some(crate::transport::NO_BUILDING_URGENCY);
        };
        if building.state.contains(BuildingState::BUILDING) {
            return Some(settings.construction(ware));
        }
        let weight = settings.weight(building.kind, ware);
        let record = self.building_at_flag(flag)?;
        if let Some((_, military)) = self.military_of(record) {
            let nearness = MILITARY_URGENCY * military.border;
            return Some(if u16::from(weight) + u16::from(nearness) > u16::from(MOST) {
                MOST
            } else {
                nearness
            });
        }
        Some(weight)
    }

    /// How much building `record` wants ware `ware` (`0x2e118`). A site with its builder there
    /// wants boards and stones until it has ordered all it needs, at its owner's construction
    /// weight. A finished building that is not stopped wants an input that is not full, more the
    /// emptier it is: `(3 × weight + 100 − fill) / 4`, with `fill` the percentage of its capacity
    /// held or on its way. A military building adds 40 per border class, up to 100, and scores 0
    /// while every soldier inside is a general.
    #[must_use]
    pub fn demand(&self, record: u16, ware: WareKind) -> u8 {
        let Some(building) = self.building(record) else {
            return 0;
        };
        let Some(row) = self.building_types().get(building.kind) else {
            return 0;
        };
        let owner = self.flag(building.flag).map_or(building.owner, |flag| flag.owner);
        let settings = &self.distribution[owner.index()];
        if building.state.contains(BuildingState::BUILDING) {
            let wanted = crate::supply::construction_index(ware)
                .is_some_and(|index| building.on_way[index] < row.cost(index));
            return if building.state.contains(BuildingState::PRESENT) && wanted {
                settings.construction(ware)
            } else {
                0
            };
        }
        if !building.state.contains(BuildingState::FINISHED)
            || building.state.contains(BuildingState::STOPPED)
        {
            return 0;
        }
        let Some(slot) = row.inputs.iter().position(|&input| input == Some(ware)) else {
            return 0;
        };
        let capacity = u32::from(row.capacity[slot]);
        if capacity == 0 {
            return 0;
        }
        let fill = u32::from(building.stock[slot] + building.on_way[slot]) * FULL / capacity;
        if fill == FULL {
            return 0;
        }
        let weight = u32::from(settings.weight(building.kind, ware));
        let score = u8::try_from((3 * weight + FULL - fill) / 4).unwrap_or(u8::MAX);
        let Some((_, military)) = self.military_of(record) else {
            return score;
        };
        if military.weakest() == Some(crate::job::Job::GENERAL) {
            return 0;
        }
        let nearness = MILITARY_URGENCY * military.border;
        if u16::from(score) + u16::from(nearness) > u16::from(MOST) {
            MOST
        } else {
            score + nearness
        }
    }

    /// What storehouse building `record` scores for ware `ware` (`0x3c034`): 0 when it stops or
    /// sends out the ware, 60 for boards and stones at a harbour preparing an expedition,
    /// otherwise 5, raised for beer, swords and shields when it holds fewer than the mean of the
    /// other two.
    pub(crate) fn storehouse_score(&self, record: u16, ware: WareKind) -> u8 {
        let Some((_, store)) = self.storehouse_of(record) else {
            return STOREHOUSE_SCORE;
        };
        if !store.takes_ware(ware) {
            return 0;
        }
        if crate::supply::construction_index(ware).is_some()
            && self
                .harbour_of(record)
                .and_then(|harbour| self.harbours.get(harbour))
                .is_some_and(|harbour| harbour.expedition)
        {
            return EXPEDITION_SCORE;
        }
        let Some(position) = BALANCED.iter().position(|&kind| kind == ware) else {
            return STOREHOUSE_SCORE;
        };
        let count = |kind: WareKind| i32::from(store.wares[kind.index()]);
        let others: i32 = BALANCED
            .iter()
            .enumerate()
            .filter(|&(index, _)| index != position)
            .map(|(_, &kind)| count(kind))
            .sum();
        let short = others / 2 - count(ware);
        if short <= 0 {
            return STOREHOUSE_SCORE;
        }
        let raised = i32::from(STOREHOUSE_SCORE) + SHORTFALL_WEIGHT * short;
        u8::try_from(raised.min(i32::from(MOST))).unwrap_or(MOST)
    }

    /// The best building for a ware of type `ware` at flag `flag` to go to (`0x3c33c`): the
    /// flag search from `flag`, the start included, scores the building at each flag reached
    /// and takes `cost / 10` off a positive score, leaving at least 1. The highest score wins,
    /// the first found among equals, and the search stops at the tenth building that scored.
    #[must_use]
    pub fn destination_for(&self, flag: u16, ware: WareKind) -> Option<u16> {
        let mut best: Option<(u16, u8)> = None;
        let mut scored = 0;
        search(
            self,
            flag,
            DESTINATION_LIMIT,
            SearchOptions::BUSY | SearchOptions::LOAD | SearchOptions::WATER,
            |reached| {
                let Some(record) = self.building_at_flag(reached.flag) else {
                    return false;
                };
                let Some(building) = self.building(record) else {
                    return false;
                };
                let storehouse = building.state.contains(BuildingState::FINISHED)
                    && self
                        .building_types()
                        .get(building.kind)
                        .is_some_and(|row| row.category == Category::Storehouse);
                let score = if storehouse {
                    self.storehouse_score(record, ware)
                } else {
                    self.demand(record, ware)
                };
                if score == 0 {
                    return false;
                }
                let penalty = u8::try_from(reached.cost / 10).unwrap_or(u8::MAX);
                let score = if score > penalty { score - penalty } else { 1 };
                if best.is_none_or(|(_, top)| score > top) {
                    best = Some((record, score));
                }
                scored += 1;
                scored == SCORED_ENOUGH
            },
        );
        best.map(|(record, _)| record)
    }

    /// Sends ware `record` to building `to` (`0x2db71`, `0x2b12c`): a site hands it the first
    /// open order it has for the ware, any other building gets a new order with the ware on its
    /// way, and counts it on its way to the input. The ware waits with somewhere to go.
    pub(crate) fn send_ware(&mut self, record: u16, to: u16) {
        let Some(kind) = self.wares.get(record).map(|ware| ware.kind) else {
            return;
        };
        let Some(building) = self.building(to).copied() else {
            return;
        };
        let order = if building.state.contains(BuildingState::BUILDING) {
            self.claim_site_order(record, building.flag, kind)
        } else {
            let urgency = self.order_urgency(building.flag, kind).unwrap_or(0);
            let owner = self.flag(building.flag).map_or(building.owner, |flag| flag.owner);
            let order = self.orders.insert(
                Order {
                    flag: building.flag,
                    urgency,
                    wanted: Wanted::Ware(kind),
                    delivering: true,
                    mode: WARE_MODE,
                    on_way: Some(record),
                    harbour: None,
                    tag: 0,
                    owner,
                },
                MAX_ORDERS,
            );
            if let Some(slot) = self.input_slot(to, kind)
                && let Some(workplace) = self.building_mut(to)
            {
                workplace.on_way[slot] += 1;
                workplace.requested += 1;
            }
            order
        };
        if let Some(ware) = self.wares.get_mut(record) {
            if order.is_some() {
                ware.order = order;
            }
            ware.state = WareState::Waiting;
        }
    }

    /// The first open order for ware type `kind` at site flag `flag` takes ware `record` on
    /// (`0x2db71`), and the site counts one more on its way. Returns the order; with none open
    /// nothing changes.
    fn claim_site_order(&mut self, record: u16, flag: u16, kind: WareKind) -> Option<u16> {
        let order = self
            .orders()
            .find(|(_, order)| {
                order.kind() == OrderKind::Ware && order.flag == flag && order.ware() == Some(kind)
            })
            .map(|(order, _)| order)?;
        let urgency = self.order_urgency(flag, kind).unwrap_or(0);
        if let Some(open) = self.orders.get_mut(order) {
            open.on_way = Some(record);
            open.delivering = true;
            open.mode = WARE_MODE;
            open.urgency = urgency;
        }
        self.release_link(order);
        if let Some(index) = crate::supply::construction_index(kind)
            && let Some(site) = self
                .building_at_flag(flag)
                .and_then(|site| self.building_mut(site))
        {
            site.on_way[index] += 1;
            site.requested += 1;
        }
        Some(order)
    }
}
