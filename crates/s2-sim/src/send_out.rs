//! A storehouse sending out what the player has set it to send.

use crate::figures::{Figure, FigureKind};
use crate::game::Game;
use crate::grid::Direction;
use crate::settlers::DOOR_SPEED;
use crate::transport::wares_by_rank;
use crate::world::{BuildingState, FLAG_CAPACITY, StockSetting};

impl Game {
    /// Storehouse building `record`'s send-out turn on the building tick (`0x31759`). With its
    /// door free and room at its flag, it takes turns: one tick it sends out one ware, the next
    /// one settler, each the first set to be sent that it holds.
    pub(crate) fn send_stock_out(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        if !building.state.contains(BuildingState::PRESENT)
            || self
                .world
                .flag(building.flag)
                .is_none_or(|flag| flag.waiting >= FLAG_CAPACITY)
        {
            return;
        }
        let Some(building) = self.world.building_mut(record) else {
            return;
        };
        building.state.toggle(BuildingState::SENDING_WARES);
        if building.state.contains(BuildingState::SENDING_WARES) {
            self.send_out_ware(record);
        } else {
            self.send_out_person(record);
        }
    }

    /// Sends out the first ware set to be sent that the storehouse holds, in its owner's transport
    /// list order. It goes nowhere until it is put down at the flag, where it is stranded and
    /// looks for somewhere to go. A send that fails ends the turn.
    fn send_out_ware(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let Some((_, store)) = self.world.storehouse_of(record) else {
            return;
        };
        let ranks = self.world.transport_ranks[building.owner.index()];
        let Some(ware) = wares_by_rank(&ranks).find(|&ware| {
            store.ware_setting(ware).contains(StockSetting::SEND) && store.wares[ware.index()] > 0
        }) else {
            return;
        };
        self.send_ware(None, record, building.flag, ware);
    }

    /// Sends out one settler of the first job set to be sent that the storehouse holds
    /// (`0x318af`, `0x36b00`). Unlike a settler sent for an order, it leaves the door free.
    fn send_out_person(&mut self, record: u16) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let Some((store, stock)) = self.world.storehouse_of(record) else {
            return;
        };
        let Some(index) = (0..stock.jobs.len())
            .find(|&index| stock.job_settings[index].contains(StockSetting::SEND) && stock.jobs[index] > 0)
        else {
            return;
        };
        let Some(job) = u8::try_from(index).ok().map(crate::job::Job) else {
            return;
        };
        let Some(flag_node) = self.world.flag(building.flag).map(|flag| flag.node) else {
            return;
        };
        if let Some(stock) = self.world.storehouse_mut(store) {
            stock.jobs[index] -= 1;
        }
        let mut figure = Figure::fresh(FigureKind::Leaving {
            owner: building.owner,
            job,
            order: None,
        });
        figure.start_walk(Direction::SouthEast, DOOR_SPEED);
        self.world.add_figure(flag_node, figure);
    }
}

#[cfg(test)]
mod tests;
