//! Task 11 orders storehouses (`0x6fae1`): every fifth call it looks at the next cell of the grid
//! and, on open land of the player's with no storehouse near, orders one at a flag there, now and
//! then with a sawmill and woodcutters beside it.

use super::ComputerPlayer;
use super::tasks::Think;
use crate::grid::Node;
use crate::world::BuildingKind;

/// Task 11 looks at a cell every this many calls (`0x6fb12`).
const EVERY: u16 = 5;

/// The storehouse sites under construction at which task 11 waits (`0x6fb35`).
const SITES: u16 = 2;

/// The rings round a cell's centre searched for a flag (`0x6fbb7`).
const FLAG_RINGS: u16 = 5;

/// The least share of buildable land, in percent, for a storehouse (`0x6fbf8`) and for the wood
/// orders beside it (`0x6fcce`).
const STOREHOUSE_LAND: u16 = 40;
const WOOD_LAND: u16 = 75;

/// The storehouse's rings closer than the profile's reach that hold no other one (`0x6fc3c`).
const STANDING_SHORT: u16 = 5;

/// The radius of the storehouse order (`0x6fc99`) and of the wood orders (`0x686a0`).
const STOREHOUSE_RADIUS: u16 = 6;
const WOOD_RADIUS: u16 = 8;

/// The wood orders that go with a storehouse, in order (`0x6863f`).
const WOOD: [BuildingKind; 4] = [
    BuildingKind::SAWMILL,
    BuildingKind::WOODCUTTER,
    BuildingKind::FORESTER,
    BuildingKind::WOODCUTTER,
];

/// Nodes on a cell's side, and a cell's centre from its corner.
const CELL_NODES: u16 = 8;
const CELL_CENTRE: u16 = 4;

/// Task 11's fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StorehouseSearch {
    /// Calls since the last look (`+0x06`).
    pub calls: u16,
    /// The cell looked at last, column and row (`+0x0c`, `+0x0e`). The constructor leaves it as
    /// the heap had it, which a new game has as (0, 0), and saves do not keep it.
    pub cursor: (u16, u16),
}

impl Think<'_> {
    /// The first of the player's flags on rings 1 to `reach` round `node`, ring by ring, each in
    /// the ring cursor's order (`0x68f49`).
    fn flag_near(&self, node: Node, reach: u16) -> Option<(u16, Node)> {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        (1..=reach)
            .flat_map(|ring| node.stepped_ring(ring, width, height))
            .find_map(|at| {
                let flag = self.game.world.flag_at(at)?;
                (self.game.world.flag(flag)?.owner == self.player).then_some((flag, at))
            })
    }
}

impl ComputerPlayer {
    /// Task 11's call (`0x6fae1`).
    pub(super) fn storehouses(&mut self, think: &mut Think<'_>) {
        let search = &mut self.tasks.storehouses;
        search.calls += 1;
        if search.calls < EVERY {
            return;
        }
        search.calls = 0;
        if think
            .game
            .world
            .building_sites(think.player, BuildingKind::STOREHOUSE)
            >= SITES
        {
            return;
        }
        let (columns, rows) = think.grid.size();
        let (mut column, mut row) = search.cursor;
        column = column.wrapping_add(1);
        if column == columns {
            column = 0;
            row = row.wrapping_add(1);
            if row == rows {
                row = 0;
            }
        }
        search.cursor = (column, row);
        let centre = Node {
            x: column.wrapping_mul(CELL_NODES).wrapping_add(CELL_CENTRE),
            y: row.wrapping_mul(CELL_NODES).wrapping_add(CELL_CENTRE),
        };
        if centre.x >= think.game.nodes.width()
            || centre.y >= think.game.nodes.height()
            || think.game.nodes.get(centre).owner != Some(think.player)
        {
            return;
        }
        let Some((_, at)) = think.flag_near(centre, FLAG_RINGS) else {
            return;
        };
        let Some(land) = think.grid.cell(column, row).map(|cell| cell.ground[0]) else {
            return;
        };
        let reach = think.profile.storehouse_spacing;
        if land < STOREHOUSE_LAND
            || self.pending(think, BuildingKind::STOREHOUSE, at, u32::from(reach))
            || think.any_near(
                think.player,
                BuildingKind::STOREHOUSE,
                at,
                reach.wrapping_sub(STANDING_SHORT),
            )
        {
            return;
        }
        self.add_order(
            think,
            (BuildingKind::STOREHOUSE, at, STOREHOUSE_RADIUS),
            None,
            None,
        );
        if land >= WOOD_LAND && think.game.rng.up_to(100) < think.profile.wood_chance {
            for kind in WOOD {
                self.add_order(think, (kind, at, WOOD_RADIUS), None, None);
            }
        }
    }
}

#[cfg(test)]
mod tests;
