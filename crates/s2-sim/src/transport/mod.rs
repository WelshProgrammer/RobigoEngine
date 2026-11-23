//! Transport: routing over the road network, the orders that ask for people and wares, and the
//! wares that wait at flags.

mod distribution;
mod orders;
mod search;
mod wares;

pub use distribution::Distribution;
pub use orders::{
    CHOSEN, CONSTRUCTION_WEIGHT, MAX_ORDERS, MOST_URGENT, NO_BUILDING_URGENCY, Order, OrderKind, ROAD_MODE,
    WARE_MODE, Wanted,
};
pub use search::{Reached, SearchOptions, next_flag, search};
pub use wares::{MAX_WARES, TRANSPORT_GROUP_NAMES, TRANSPORT_GROUPS, Ware, WareState};
pub(crate) use wares::{transport_group, wares_by_rank};

impl crate::game::Game {
    /// Every 16 steps (`0x2b3e8`): ages the orders and serves the twenty most urgent, in the
    /// order [`crate::world::World::choose_orders`] lists them. An order served earlier in the
    /// pass changes what later ones find.
    pub(crate) fn serve_orders(&mut self) {
        for record in self.world.choose_orders() {
            match self.world.order(record).map(Order::kind) {
                Some(OrderKind::Person) => self.serve_person(record),
                Some(OrderKind::Ware) => self.serve_ware(record),
                _ => {}
            }
        }
    }
}
