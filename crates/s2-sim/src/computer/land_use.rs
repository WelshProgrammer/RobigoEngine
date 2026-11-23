//! Task 12: what the land is used for (`0x6fdc5`). Cell by cell, it asks for buildings where the
//! survey shows trees, stone, ore, fish or open land.

use super::ComputerPlayer;
use super::board::{BEER, COAL, DONKEYS, FOOD, GOLD, STONE, TOOLS, WEAPONS, WOOD, ledger};
use super::grid::{CELL, Cell};
use super::jobs::Job;
use super::projects::WITH_WOODCUTTER;
use super::tasks::Think;
use crate::grid::Node;
use crate::ware::WareKind;
use crate::world::BuildingKind;

/// Cells one call looks at, at most, before giving up (`0x6fe03`).
const CELLS_A_CALL: u16 = 128;

/// Task 12's fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LandUse {
    /// Calls block A has waited (`+0x07`).
    pub delay: u8,
    /// The cell last looked at (`+0x08`, `+0x0a`).
    pub cursor: (u16, u16),
}

/// What a run does next: go on to the next block, or end, having made a job or not.
enum Next {
    Block,
    Done(bool),
}

impl ComputerPlayer {
    /// One call: the next cell, then the one after, until one makes a job or 128 have been
    /// looked at (`0x6fdc5`).
    pub(super) fn land_use(&mut self, think: &mut Think<'_>) {
        for _ in 0..CELLS_A_CALL {
            if self.land_use_cell(think) {
                return;
            }
        }
    }

    /// One cell (`0x6fe22`): the cursor moves on a cell, along the row and then down, and a
    /// cell whose centre the player owns goes through blocks A to O in turn until one makes a
    /// job or [`Self::skip`] passes over the cell.
    fn land_use_cell(&mut self, think: &mut Think<'_>) -> bool {
        let (columns, rows) = think.grid.size();
        let (mut column, mut row) = self.tasks.land_use.cursor;
        column = column.wrapping_add(1);
        if column == columns {
            column = 0;
            row = row.wrapping_add(1);
            if row == rows {
                row = 0;
            }
        }
        self.tasks.land_use.cursor = (column, row);
        let centre = Node {
            x: column.wrapping_mul(CELL).wrapping_add(CELL / 2),
            y: row.wrapping_mul(CELL).wrapping_add(CELL / 2),
        };
        if think.game.nodes.get(centre).owner != Some(self.player) {
            return false;
        }
        let Some(&cell) = think.grid.cell(column, row) else {
            return false;
        };
        for block in [
            Self::block_a,
            Self::block_b,
            Self::block_c,
            Self::block_d,
            Self::block_e,
            Self::block_f,
            Self::block_g,
            Self::block_h,
            Self::block_i_to_l,
            Self::block_m,
            Self::block_n,
            Self::block_o,
        ] {
            if let Next::Done(made) = block(self, think, &cell, centre) {
                return made;
            }
        }
        false
    }

    /// `0x68e68`: one draw, and true when it falls below the profile's skip chance. Its walk
    /// over the 20 rings round the node changes nothing.
    fn skip(think: &mut Think<'_>) -> bool {
        think.game.rng.up_to(100) < u16::from(think.profile.skip_chance)
    }

    /// Whether the player has a build order for `kind`, or a placement of another kind, within
    /// `reach` of `node` (`0x68370`). The placement test compares unequal, as `S2.EXE` does.
    pub(super) fn pending(&self, think: &Think<'_>, kind: BuildingKind, node: Node, reach: u32) -> bool {
        self.jobs().any(|(_, job)| match job.payload {
            super::jobs::Payload::BuildOrder {
                kind: wanted,
                node: at,
                ..
            } => wanted == kind && think.distance(at, node) <= reach,
            super::jobs::Payload::Place(placement) => {
                placement.kind != kind && think.distance(placement.node, node) <= reach
            }
            _ => false,
        })
    }

    /// Requests `kind` of the board for a project at `node` (`0x72489`).
    fn grant(&mut self, think: &Think<'_>, kind: u8, node: Node) -> bool {
        let ledger = ledger(
            &think.game.world,
            think.game.nodes.owned(self.player),
            self.player,
            &think.profile,
        );
        let add_wait = think.profile.add_wait;
        self.tasks.board.grant(kind, node, &ledger, add_wait)
    }

    /// Makes `job` a root job with no owner.
    fn make(&mut self, job: Job) -> Next {
        self.add(job, None, None);
        Next::Done(true)
    }

    /// Asks for `kind` and, when the board grants it, makes its project.
    fn ask(&mut self, think: &mut Think<'_>, kind: u8, node: Node, options: u16) -> Option<Next> {
        self.grant(think, kind, node)
            .then(|| self.make(Job::project(kind, node, options, think.step)))
    }

    /// Block A: open land with few trees and room for a large building, every 20 to 30 such cells,
    /// asks for beer, donkeys or food.
    fn block_a(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ground[0] < 25 || cell.trees >= 80 || cell.sites[2] < 1 {
            return Next::Block;
        }
        let state = &mut self.tasks.land_use;
        state.delay = state.delay.wrapping_add(1);
        let wait = 20 + think.game.rng.up_to(10);
        if u16::from(self.tasks.land_use.delay) < wait {
            return Next::Block;
        }
        self.tasks.land_use.delay = 0;
        let board = &self.tasks.board;
        if board.count(BEER) + board.count(DONKEYS) + board.count(FOOD) == 0 {
            return Next::Block;
        }
        if Self::skip(think) {
            return Next::Done(false);
        }
        let wooded = if cell.trees > 10 { WITH_WOODCUTTER } else { 0 };
        if let Some(next) = self.ask(think, BEER, node, 0) {
            return next;
        }
        if let Some(next) = self.ask(think, DONKEYS, node, wooded) {
            return next;
        }
        if self.grant(think, FOOD, node) {
            let fish = u16::from(think.game.rng.up_to(100) > 50);
            return self.make(Job::project(FOOD, node, wooded | fish, think.step));
        }
        Next::Block
    }

    /// Block B: open land now and then asks for wood, with no sawmill near.
    fn block_b(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ground[0] < 25 || think.game.rng.up_to(100) >= 5 {
            return Next::Block;
        }
        if think.any_near(self.player, BuildingKind::SAWMILL, node, 5)
            || self.pending(think, BuildingKind::SAWMILL, node, 5)
        {
            return Next::Block;
        }
        self.request(think, &[WOOD], node, 0)
    }

    /// Block C: gold asks for a gold mine's chain.
    fn block_c(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ore[2] < 8
            || self.pending(think, BuildingKind::GOLD_MINE, node, 4)
            || think.count_near(self.player, Some(BuildingKind::GOLD_MINE), node, 5) >= 2
        {
            return Next::Block;
        }
        self.request(think, &[GOLD], node, 2)
    }

    /// Block D: iron asks for tools when armouries outnumber metalworks, else for weapons.
    fn block_d(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ore[0] < 8 || think.game.rng.up_to(100) <= 30 {
            return Next::Block;
        }
        if self.pending(think, BuildingKind::IRON_MINE, node, 4)
            || think.count_near(self.player, Some(BuildingKind::IRON_MINE), node, 4) >= 3
        {
            return Next::Block;
        }
        let board = &self.tasks.board;
        if board.count(WEAPONS) + board.count(TOOLS) == 0 {
            return Next::Block;
        }
        if Self::skip(think) {
            return Next::Done(false);
        }
        if think
            .game
            .world
            .finished_buildings(self.player, BuildingKind::ARMOURY)
            >= 2
            && self.tasks.board.count(TOOLS) != 0
            && think.built(BuildingKind::METALWORKS) < think.built(BuildingKind::ARMOURY)
            && let Some(next) = self.ask(think, TOOLS, node, 0)
        {
            return next;
        }
        self.ask(think, WEAPONS, node, 0).unwrap_or(Next::Block)
    }

    /// Block E: coal asks for a coal mine's chain.
    fn block_e(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ore[1] < 12 || think.game.rng.up_to(100) <= 75 {
            return Next::Block;
        }
        if self.pending(think, BuildingKind::COAL_MINE, node, 3)
            || think.count_near(self.player, Some(BuildingKind::COAL_MINE), node, 5) >= 4
        {
            return Next::Block;
        }
        self.request(think, &[COAL], node, 0)
    }

    /// Block F: granite piles with no quarry near make a stone project, three times in 101 without
    /// asking the board.
    fn block_f(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.granite_piles < 3
            || think.any_near(self.player, BuildingKind::QUARRY, node, 6)
            || self.pending(think, BuildingKind::QUARRY, node, 6)
        {
            return Next::Block;
        }
        if think.game.rng.up_to(100) < 3 {
            return self.make(Job::project(STONE, node, 0, think.step));
        }
        self.request(think, &[STONE], node, 0)
    }

    /// Block G: granite in the mountain asks for a granite mine's chain.
    fn block_g(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ore[3] < 12
            || self.pending(think, BuildingKind::GRANITE_MINE, node, 5)
            || think.count_near(self.player, Some(BuildingKind::GRANITE_MINE), node, 5) >= 2
        {
            return Next::Block;
        }
        self.request(think, &[STONE], node, 1)
    }

    /// Block H: fish with no fishery near asks for food.
    fn block_h(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.fish < 8
            || think.any_near(self.player, BuildingKind::FISHERY, node, 12)
            || self.pending(think, BuildingKind::FISHERY, node, 12)
        {
            return Next::Block;
        }
        self.request(think, &[FOOD], node, 2)
    }

    /// Blocks I to L: a quarry, a woodcutter, a fishery or a hunter where the survey has enough
    /// granite, trees, fish or trees and none of the kind stands near. After a fishery, the low
    /// bit of a raw draw drops the first food request.
    fn block_i_to_l(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        let [woodcutter, quarry, fishery, hunter] = think.profile.land_use;
        let rows = [
            (cell.granite_piles >= quarry, BuildingKind::QUARRY, 8, 8, 5),
            (cell.trees >= woodcutter, BuildingKind::WOODCUTTER, 6, 8, 6),
            (cell.fish >= fishery, BuildingKind::FISHERY, 10, 10, 10),
            (cell.trees >= hunter, BuildingKind::HUNTER, 12, 12, 8),
        ];
        for (enough, kind, standing, ordered, radius) in rows {
            if !enough
                || think.any_near(self.player, kind, node, standing)
                || self.pending(think, kind, node, ordered)
            {
                continue;
            }
            self.add_order(think, (kind, node, radius), None, None);
            if kind == BuildingKind::FISHERY && think.game.rng.step() & 1 == 1 {
                self.tasks.board.remove_first(FOOD);
            }
            return Next::Done(true);
        }
        Next::Block
    }

    /// Block M: coal orders a coal mine when the board grants coal.
    fn block_m(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        self.mine(think, (cell.ore[1], BuildingKind::COAL_MINE, COAL), node)
    }

    /// Block N: gold orders a gold mine when the board grants gold.
    fn block_n(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        self.mine(think, (cell.ore[2], BuildingKind::GOLD_MINE, GOLD), node)
    }

    /// Blocks M and N: at least 12 ore, a draw above 75, no order within 4 and fewer than 4 mines
    /// within 6, and a grant, order a mine.
    fn mine(
        &mut self,
        think: &mut Think<'_>,
        (ore, kind, request): (u8, BuildingKind, u8),
        node: Node,
    ) -> Next {
        if ore < 12 || think.game.rng.up_to(100) <= 75 {
            return Next::Block;
        }
        if self.pending(think, kind, node, 4)
            || think.count_near(self.player, Some(kind), node, 6) >= 4
            || !self.grant(think, request, node)
        {
            return Next::Block;
        }
        self.add_order(think, (kind, node, 6), None, None);
        Next::Done(true)
    }

    /// Block O: open land with few trees and room for a large building, once in about 67 cells and
    /// with less than 50 grain in stock, orders a farm.
    fn block_o(&mut self, think: &mut Think<'_>, cell: &Cell, node: Node) -> Next {
        if cell.ground[0] < 75 || cell.trees >= 60 || cell.sites[2] < 1 || think.game.rng.up_to(1000) >= 15 {
            return Next::Block;
        }
        if think.game.world.owned_wares(self.player, WareKind::GRAIN) >= 50
            || self.pending(think, BuildingKind::FARM, node, 15)
            || think.count_near(self.player, Some(BuildingKind::FARM), node, 15) >= 3
        {
            return Next::Block;
        }
        self.add_order(think, (BuildingKind::FARM, node, 6), None, None);
        Next::Done(true)
    }

    /// Asks for the first of `kinds` the board holds any of: one [`Self::skip`] draw when it
    /// holds some, then a grant makes the project with `options`.
    fn request(&mut self, think: &mut Think<'_>, kinds: &[u8], node: Node, options: u16) -> Next {
        let held: u16 = kinds.iter().map(|&kind| self.tasks.board.count(kind)).sum();
        if held == 0 {
            return Next::Block;
        }
        if Self::skip(think) {
            return Next::Done(false);
        }
        for &kind in kinds {
            if let Some(next) = self.ask(think, kind, node, options) {
                return next;
            }
        }
        Next::Block
    }
}
