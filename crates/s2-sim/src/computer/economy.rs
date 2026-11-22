//! Task 14: the economy planner (`0x716eb`). Every few calls it ages the board's cooldowns, writes
//! the player's settings the first time, orders workplaces from one storehouse's stock, and asks
//! the board for the economy chains the player lacks.

use super::ComputerPlayer;
use super::board::{DONKEYS, FOOD, GOLD, STONE, TOOLS, WEAPONS, WOOD};
use super::tasks::Think;
use crate::grid::Node;
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, Category, Player, World};

/// Land the stock rules other than wood need (`0x718fb`).
const STOCK_LAND: u32 = 600;

/// The first tool among the wares, which the tool weights start from (`0x717f5`).
const FIRST_TOOL: u8 = 16;

/// Where a request is wanted when it may go anywhere.
const ANYWHERE: (i16, i16) = (-1, -1);

/// Task 14's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EconomyPlanner {
    /// Calls since the last run (`+0x06`).
    pub calls: u16,
    /// Whether the settings have been written (`+0x296`).
    pub written: bool,
    /// The storehouse the stock rules looked at last, `None` when there was none (`+0x298`).
    pub cursor: Option<u16>,
}

impl Default for EconomyPlanner {
    fn default() -> Self {
        Self {
            calls: 0,
            written: false,
            cursor: Some(0),
        }
    }
}

impl ComputerPlayer {
    /// One call: every profile's number of calls a run (`0x716eb`).
    pub(super) fn economy(&mut self, think: &mut Think<'_>) {
        let planner = &mut self.tasks.economy;
        planner.calls += 1;
        if planner.calls < think.profile.economy.calls {
            return;
        }
        planner.calls = 0;
        self.tasks.board.tick();
        if !self.tasks.economy.written {
            self.tasks.economy.written = true;
            Self::write_settings(think);
        }
        let land = think.game.nodes.owned(think.player);
        let headquarters = headquarters(&think.game.world, &think.game.nodes, think.player);
        self.stock_rules(think, land);
        self.requests(think, land, headquarters);
    }

    /// The first run's settings (`0x71787` to `0x71855`): the distribution sliders, the tool
    /// weights and the military settings of the profile's first set.
    fn write_settings(think: &mut Think<'_>) {
        let profile = think.profile;
        for (group, sliders) in profile.economy.sliders.iter().enumerate() {
            for (consumer, &weight) in sliders.iter().enumerate() {
                let _ = think.game.world.set_slider(think.player, group, consumer, weight);
            }
        }
        for (tool, &weight) in (FIRST_TOOL..).zip(&profile.economy.tools) {
            let _ = think
                .game
                .world
                .set_tool_weight(think.player, WareKind(tool), weight);
        }
        think.game.world.military_settings[think.player.index()] = profile.military_sets[0];
    }

    /// The stock rules (`0x7188a` to `0x7205a`): the next finished storehouse after the cursor, and
    /// orders round it for what its own stock has too much or too little of.
    fn stock_rules(&mut self, think: &mut Think<'_>, land: u32) {
        let next = next_storehouse(
            &think.game.world,
            &think.game.nodes,
            self.tasks.economy.cursor,
            think.player,
        );
        self.tasks.economy.cursor = next;
        let Some(record) = next else {
            return;
        };
        let Some(centre) = think.game.world.building(record).map(|building| building.node) else {
            return;
        };
        let stock = |ware: WareKind| {
            think
                .game
                .world
                .storehouse_of(record)
                .map_or(0, |(_, store)| store.wares[ware.index()])
        };
        let (gold, coal, iron_ore) = (
            stock(WareKind::GOLD),
            stock(WareKind::COAL),
            stock(WareKind::IRON_ORE),
        );
        let (wood, flour, grain) = (
            stock(WareKind::WOOD),
            stock(WareKind::FLOUR),
            stock(WareKind::GRAIN),
        );
        let rules = think.profile.economy.stock;
        let roomy = land > STOCK_LAND;
        if roomy && gold > rules[0] {
            if !think.any_near(think.player, BuildingKind::MINT, centre, 12)
                && !self.pending(think, BuildingKind::MINT, centre, 12)
            {
                self.order(think, BuildingKind::MINT, centre, 8);
            }
            if coal < rules[1] {
                self.order_if(think, (BuildingKind::COAL_MINE, 16, 5, 16), centre, 12);
            }
        }
        if roomy && iron_ore > rules[2] {
            self.order_if(think, (BuildingKind::IRON_SMELTER, 12, 3, 12), centre, 8);
            let maker = if think.game.rng.up_to(100) < rules[3] {
                BuildingKind::ARMOURY
            } else {
                BuildingKind::METALWORKS
            };
            self.order_if(think, (maker, 12, 2, 12), centre, 8);
        }
        if wood > rules[4] {
            self.order_if(think, (BuildingKind::SAWMILL, 8, 2, 8), centre, 8);
        }
        if roomy
            && flour > rules[5]
            && self.order_if(think, (BuildingKind::BAKERY, 12, 2, 15), centre, 15)
            && !think.any_near(think.player, BuildingKind::WELL, centre, 16)
            && !self.pending(think, BuildingKind::WELL, centre, 16)
        {
            self.order(think, BuildingKind::WELL, centre, 12);
        }
        if roomy && grain > rules[6] {
            self.order_if(think, (BuildingKind::MILL, 12, 2, 15), centre, 15);
        }
    }

    /// Orders `kind` at `centre` when fewer than `most` stand within `rings` and none is pending
    /// within `reach`. Returns whether it ordered.
    fn order_if(
        &mut self,
        think: &mut Think<'_>,
        (kind, rings, most, reach): (BuildingKind, u16, u16, u32),
        centre: Node,
        radius: u16,
    ) -> bool {
        if think.count_near(think.player, Some(kind), centre, rings) >= most
            || self.pending(think, kind, centre, reach)
        {
            return false;
        }
        self.order(think, kind, centre, radius);
        true
    }

    /// A build order for `kind` at `centre`, a root job with no owner.
    fn order(&mut self, think: &Think<'_>, kind: BuildingKind, centre: Node, radius: u16) {
        self.add_order(think, (kind, centre, radius), None, None);
    }

    /// The request rules (`0x7205a` to `0x723be`): requests for the chains the player has none
    /// of, or too few of for its land, at the headquarters or anywhere.
    fn requests(&mut self, think: &Think<'_>, land: u32, headquarters: (i16, i16)) {
        let built = |kind: BuildingKind| think.built(kind);
        let add_wait = think.profile.add_wait;
        let board = &mut self.tasks.board;
        let idle = |board: &super::board::RequestBoard, kind: u8| {
            board.add_wait(kind) == 0 && board.count(kind) == 0
        };
        let sawmills = built(BuildingKind::SAWMILL);
        if idle(board, WOOD) && sawmills == 0 {
            board.add(WOOD, headquarters, add_wait, false);
        }
        for (fewer, more_land) in [(2, 300), (3, 800), (5, 1_400)] {
            if board.count(WOOD) == 0 && sawmills < fewer && land > more_land {
                board.add(WOOD, ANYWHERE, add_wait, false);
            }
        }
        if board.count(FOOD) == 0 && land > 280 && land < 800 {
            board.add(FOOD, headquarters, add_wait, false);
        }
        if idle(board, STONE) && built(BuildingKind::QUARRY) == 0 {
            board.add(STONE, headquarters, add_wait, false);
        }
        if sawmills == 0 {
            return;
        }
        let shares = think.profile.economy.land;
        let per = |share: u16| land / u32::from(share.max(1));
        let stone = built(BuildingKind::QUARRY) + built(BuildingKind::GRANITE_MINE);
        let rows = [
            (WOOD, u32::from(sawmills) <= per(shares[0]), ANYWHERE),
            (STONE, u32::from(stone) <= per(shares[1]), headquarters),
            (
                WEAPONS,
                u32::from(built(BuildingKind::ARMOURY)) <= per(shares[2]),
                ANYWHERE,
            ),
            (
                GOLD,
                u32::from(built(BuildingKind::MINT)) <= per(shares[3]),
                ANYWHERE,
            ),
            (
                TOOLS,
                u32::from(built(BuildingKind::METALWORKS)) <= per(shares[4]),
                ANYWHERE,
            ),
            (
                DONKEYS,
                u32::from(built(BuildingKind::DONKEY_BREEDER)) < per(shares[5]),
                ANYWHERE,
            ),
        ];
        for (kind, wanted, at) in rows {
            if idle(board, kind) && wanted {
                board.add(kind, at, add_wait, false);
            }
        }
    }
}

/// The node of `player`'s first headquarters by record, as signed words, or anywhere when it
/// has none finished (`0x29c7b`).
pub(super) fn headquarters(world: &World, nodes: &crate::nodes::NodeMap, player: Player) -> (i16, i16) {
    if world.finished_buildings(player, BuildingKind::HEADQUARTERS) == 0 {
        return ANYWHERE;
    }
    world
        .buildings()
        .find(|(_, building)| {
            building.kind == BuildingKind::HEADQUARTERS && nodes.get(building.node).owner == Some(player)
        })
        .map_or(ANYWHERE, |(_, building)| {
            (building.node.x.cast_signed(), building.node.y.cast_signed())
        })
}

/// The first finished storehouse, harbour or headquarters of `player` after record `cursor`, round
/// to the start, `cursor` itself last (`0x68809`). `None` for no cursor starts at record 0.
pub(super) fn next_storehouse(
    world: &World,
    nodes: &crate::nodes::NodeMap,
    cursor: Option<u16>,
    player: Player,
) -> Option<u16> {
    let start = cursor.map_or(0, |at| at.wrapping_add(1));
    let fits = |record: u16| {
        world.building(record).is_some_and(|building| {
            building.state.contains(BuildingState::FINISHED)
                && nodes.get(building.node).owner == Some(player)
                && world
                    .building_types()
                    .get(building.kind)
                    .is_some_and(|row| row.category == Category::Storehouse)
        })
    };
    let records: Vec<u16> = world.buildings().map(|(record, _)| record).collect();
    records
        .iter()
        .copied()
        .find(|&record| record >= start && fits(record))
        .or_else(|| {
            records
                .iter()
                .copied()
                .find(|&record| record < start && fits(record))
        })
}

#[cfg(test)]
mod tests;
