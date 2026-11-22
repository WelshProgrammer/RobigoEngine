//! Task 15 guards what matters (`0x7270b`): every so many calls it looks at a few cells of the
//! grid, and round the player's valuable buildings orders a catapult, a watchtower or a fortress.

use super::ComputerPlayer;
use super::tasks::Think;
use crate::grid::Node;
use crate::world::{BuildingKind, BuildingState, MapObject};

/// A sum worth guarding (`0x72996`, `0x72a8c`).
const WORTH_GUARDING: u16 = 100;

/// The rings round a cell's centre kept free of another catapult (`0x729ed`) and of a pending one
/// (`0x72a0a`).
const CATAPULT_RINGS: u16 = 12;
const CATAPULT_PENDING: u32 = 15;

/// The towers of a kind a cell's centre may have within 10 rings (`0x72afc`, `0x72b0d`) and the
/// reach of a pending one (`0x72b1c`).
const TOWER_RINGS: u16 = 10;
const TOWERS: u16 = 3;
const TOWER_PENDING: u32 = 12;

/// The radius of the orders (`0x72a5a`, `0x72b66`).
const ORDER_RADIUS: u16 = 12;

/// Nodes on a cell's side, and a cell's centre from its corner.
const CELL_NODES: u16 = 8;
const CELL_CENTRE: u16 = 4;

/// Task 15's fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Guards {
    /// Calls since the last scan (`+0x06`).
    pub calls: u16,
    /// The cell looked at last, column and row (`+0x08`, `+0x0a`).
    pub cursor: (u16, u16),
}

/// What a finished building of `kind` adds to a cell's value and to its military weight, by the
/// profile's weights (`0x728cc`).
fn weights(kind: BuildingKind, profile: &super::profiles::Towers) -> (u16, u16) {
    match kind {
        BuildingKind::METALWORKS => (profile.metalworks, 0),
        BuildingKind::ARMOURY => (profile.armoury, 0),
        BuildingKind::MINT => (profile.mint.0, profile.mint.1),
        BuildingKind::GOLD_MINE => (profile.gold_mine, 0),
        BuildingKind::STOREHOUSE | BuildingKind::HARBOUR => (profile.storehouse.0, profile.storehouse.1),
        BuildingKind::WATCHTOWER => (0, profile.watchtower),
        BuildingKind::FORTRESS => (0, profile.fortress),
        _ => (0, 0),
    }
}

impl ComputerPlayer {
    /// Task 15's call (`0x7270b`): every profile's number of calls, the profile's number of cells.
    pub(super) fn guard(&mut self, think: &mut Think<'_>) {
        let guards = &mut self.tasks.guards;
        guards.calls += 1;
        if guards.calls < think.profile.towers.every {
            return;
        }
        guards.calls = 0;
        for _ in 0..think.profile.towers.cells {
            self.guard_cell(think);
        }
    }

    /// One cell (`0x7274d`): the cursor moves on a cell, and in one whose centre the player owns
    /// the player's finished buildings on the centre's rings are summed into a value and a
    /// military weight. Enough weight may order a catapult and enough value a watchtower or a
    /// fortress, each at the centre, by draws.
    fn guard_cell(&mut self, think: &mut Think<'_>) {
        let (columns, rows) = think.grid.size();
        let (mut column, mut row) = self.tasks.guards.cursor;
        column = column.wrapping_add(1);
        if column == columns {
            column = 0;
            row = row.wrapping_add(1);
            if row == rows {
                row = 0;
            }
        }
        self.tasks.guards.cursor = (column, row);
        let centre = Node {
            x: column.wrapping_mul(CELL_NODES).wrapping_add(CELL_CENTRE),
            y: row.wrapping_mul(CELL_NODES).wrapping_add(CELL_CENTRE),
        };
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        if centre.x >= width || centre.y >= height || think.game.nodes.get(centre).owner != Some(think.player)
        {
            return;
        }
        let profile = think.profile.towers;
        let (mut value, mut weight) = (0u16, 0u16);
        for at in centre.rings(profile.rings, width, height).skip(1) {
            let MapObject::Building(record) = think.game.world.object(at) else {
                continue;
            };
            let Some(building) = think.game.world.building(record) else {
                continue;
            };
            if think.game.nodes.get(at).owner != Some(think.player)
                || think.game.world.building_owner(record) != Some(think.player)
                || !building.state.contains(BuildingState::FINISHED)
            {
                continue;
            }
            let (worth, military) = weights(building.kind, &profile);
            value = value.wrapping_add(worth);
            weight = weight.wrapping_add(military);
        }
        if weight >= WORTH_GUARDING
            && think.game.rng.up_to(100) < profile.catapult_chance
            && !think.any_near(think.player, BuildingKind::CATAPULT, centre, CATAPULT_RINGS)
            && !self.pending(think, BuildingKind::CATAPULT, centre, CATAPULT_PENDING)
        {
            self.add_order(think, (BuildingKind::CATAPULT, centre, ORDER_RADIUS), None, None);
        }
        if value < WORTH_GUARDING || think.game.rng.up_to(100) < profile.tower_chance {
            return;
        }
        let kind = if think.game.rng.up_to(100) < profile.fortress_chance {
            BuildingKind::FORTRESS
        } else {
            BuildingKind::WATCHTOWER
        };
        if think.count_near(think.player, Some(kind), centre, TOWER_RINGS) >= TOWERS
            || self.pending(think, kind, centre, TOWER_PENDING)
        {
            return;
        }
        self.add_order(think, (kind, centre, ORDER_RADIUS), None, None);
    }
}

#[cfg(test)]
mod tests;
