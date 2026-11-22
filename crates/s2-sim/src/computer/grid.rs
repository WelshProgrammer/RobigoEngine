//! The grid the computer players read the map through: one survey of each 8 × 8-node cell,
//! shared by every player (`0x67c02`).

use crate::grid::Node;
use crate::ground::Property;
use crate::nodes::{NodeMap, SiteFlags, SiteSize};
use crate::world::{MapObject, World};

/// A cell is this many nodes on a side.
pub const CELL: u16 = 8;

/// Nodes in a cell, the divisor of every share (`+0x0c`).
const NODES: u32 = 64;

/// The most a summed amount is stored as; a sum up to it is halved (`0x67fd8`).
const MOST: u32 = 250;

/// A share in hundredths.
const PERCENT: u32 = 100;

/// Slot 11's mineral bits for each ore the cell counts on mountain nodes.
const COAL: u8 = 0;
const IRON: u8 = 8;
const GOLD: u8 = 16;
const GRANITE: u8 = 24;

/// Grid passes rebuild this many cells a step (`0x68150`).
const PER_STEP: usize = 4;

bitflags::bitflags! {
    /// What a cell's nodes are like (`+0x00` of the survey).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    pub struct CellFlags: u16 {
        /// Some node has a property in `0x3f300`: mountain, class 6, buildable or class 9 ground.
        const SOLID = 1;
        /// No node touches water.
        const DRY = 1 << 1;
        /// Some node touches water.
        const WET = 1 << 2;
        /// No node has a property in `0x3f300`.
        const OPEN = 1 << 3;
    }
}

/// One cell's survey (21 bytes from `0x67d85`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    /// What its nodes are like.
    pub flags: CellFlags,
    /// Granite pile sizes added up, at most 250.
    pub granite_piles: u8,
    /// Share of nodes with a tree a woodcutter fells.
    pub trees: u8,
    /// Share of land nodes with ground water.
    pub ground_water: u8,
    /// Iron, coal, gold and granite on mountain nodes: half the summed amounts, or 250 when the sum
    /// is above it.
    pub ore: [u8; 4],
    /// Fish on open water, at most 250.
    pub fish: u8,
    /// Shares of nodes that take a small or larger building, a medium or larger, a large one, a mine.
    pub sites: [u8; 4],
    /// Share of nodes a tree can be planted on.
    pub plantable: u8,
    /// Shares of nodes all of whose triangles are buildable, water, mountain.
    pub ground: [u16; 3],
}

impl Cell {
    /// The 21 bytes as `S2.EXE` lays them out.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 21] {
        let mut bytes = [0; 21];
        bytes[0..2].copy_from_slice(&self.flags.bits().to_le_bytes());
        bytes[2] = self.granite_piles;
        bytes[3] = self.trees;
        bytes[4] = self.ground_water;
        bytes[5..9].copy_from_slice(&self.ore);
        bytes[9] = self.fish;
        bytes[10..14].copy_from_slice(&self.sites);
        bytes[14] = self.plantable;
        for (place, share) in self.ground.iter().enumerate() {
            bytes[15 + 2 * place..17 + 2 * place].copy_from_slice(&share.to_le_bytes());
        }
        bytes
    }

    /// Surveys cell (`column`, `row`) of `nodes` and `world` (`0x67d85`).
    fn survey(nodes: &NodeMap, world: &World, column: u16, row: u16) -> Self {
        let solid = Property::ALL_MOUNTAIN
            | Property::ANY_MOUNTAIN
            | Property::ALL_CLASS_6
            | Property::ANY_CLASS_6
            | Property::ALL_BUILDABLE
            | Property::ANY_BUILDABLE
            | Property::ALL_CLASS_9
            | Property::ANY_CLASS_9;
        let mut flags = CellFlags::DRY | CellFlags::OPEN;
        let mut counts = [0u32; 6];
        let (mut sites, mut ore, mut ground) = ([0u32; 4], [0u32; 4], [0u32; 3]);
        let (mut piles, mut fish) = (0u32, 0u32);
        for j in 0..CELL {
            for i in 0..CELL {
                let node = Node {
                    x: column * CELL + i,
                    y: row * CELL + j,
                };
                let data = nodes.get(node);
                let property = nodes.property(node);
                if property.intersects(solid) {
                    flags = (flags | CellFlags::SOLID) - CellFlags::OPEN;
                }
                if property.intersects(Property::ALL_WATER | Property::ANY_WATER) {
                    flags = (flags | CellFlags::WET) - CellFlags::DRY;
                }
                for (count, kept) in ground.iter_mut().zip([
                    Property::ALL_BUILDABLE,
                    Property::ALL_WATER,
                    Property::ALL_MOUNTAIN,
                ]) {
                    *count += u32::from(property.contains(kept));
                }
                let reach = match data.sites.size {
                    SiteSize::Small => 1,
                    SiteSize::Medium => 2,
                    SiteSize::Large => 3,
                    _ => 0,
                };
                for count in sites.iter_mut().take(reach) {
                    *count += 1;
                }
                if data.sites.size == SiteSize::Mine {
                    sites[3] += 1;
                }
                counts[0] += u32::from(!data.sites.flags.contains(SiteFlags::NO_PLANTING));
                if let MapObject::Granite(granite) = world.object(node) {
                    piles += u32::from(granite.size);
                }
                counts[1] += u32::from(world.fellable(node));
                let (kind, amount) = (data.resources.mineral_bits(), u32::from(data.resources.amount()));
                if property.contains(Property::ALL_MOUNTAIN) {
                    let slot = match kind {
                        IRON => Some(0),
                        COAL => Some(1),
                        GOLD => Some(2),
                        GRANITE => Some(3),
                        _ => None,
                    };
                    if let Some(slot) = slot {
                        ore[slot] += amount;
                    }
                } else if property.intersects(Property::ALL_BUILDABLE | Property::ALL_CLASS_6) {
                    counts[2] += u32::from(kind == COAL && amount != 0);
                } else if property.contains(Property::ALL_WATER) && kind == COAL {
                    fish += amount;
                }
            }
        }
        let share = |count: u32| count * PERCENT / NODES;
        let byte = |count: u32| u8::try_from(share(count) & u32::from(u8::MAX)).unwrap_or_default();
        let held = |sum: u32| u8::try_from(if sum > MOST { MOST } else { sum / 2 }).unwrap_or_default();
        let capped = |sum: u32| u8::try_from(sum.min(MOST)).unwrap_or_default();
        Self {
            flags,
            granite_piles: capped(piles),
            trees: byte(counts[1]),
            ground_water: byte(counts[2]),
            ore: ore.map(held),
            fish: capped(fish),
            sites: sites.map(byte),
            plantable: byte(counts[0]),
            ground: ground.map(|count| u16::try_from(share(count)).unwrap_or_default()),
        }
    }
}

/// The grid and the cursor of the pass that keeps it fresh.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Grid {
    columns: u16,
    rows: u16,
    cursor: (u16, u16),
    cells: Vec<Cell>,
}

impl Grid {
    /// The grid over `nodes`, every cell surveyed (`0x67cab`): the map's width and height over 8,
    /// truncated, and the cursor at the first cell.
    #[must_use]
    pub fn build(nodes: &NodeMap, world: &World) -> Self {
        let (columns, rows) = (nodes.width() / CELL, nodes.height() / CELL);
        let cells = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| Cell::survey(nodes, world, column, row))
            .collect();
        Self {
            columns,
            rows,
            cursor: (0, 0),
            cells,
        }
    }

    /// Cells across and down.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (self.columns, self.rows)
    }

    /// The pass's cursor, column and row.
    #[must_use]
    pub fn cursor(&self) -> (u16, u16) {
        self.cursor
    }

    /// Cell (`column`, `row`), when the grid has it.
    #[must_use]
    pub fn cell(&self, column: u16, row: u16) -> Option<&Cell> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.columns) + usize::from(column))
    }

    /// The cells, row by row.
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// One step's pass (`0x68150`): four times, the cursor moves on a cell, along the row and
    /// then to the next, back to the first after the last, and that cell is surveyed again.
    pub(crate) fn pass(&mut self, nodes: &NodeMap, world: &World) {
        if self.cells.is_empty() {
            return;
        }
        for _ in 0..PER_STEP {
            let (mut column, mut row) = self.cursor;
            column += 1;
            if column >= self.columns {
                column = 0;
                row += 1;
                if row >= self.rows {
                    row = 0;
                }
            }
            self.cursor = (column, row);
            let at = usize::from(row) * usize::from(self.columns) + usize::from(column);
            self.cells[at] = Cell::survey(nodes, world, column, row);
        }
    }
}
