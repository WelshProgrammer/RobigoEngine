//! A carrier taking a ware in through the door of the building at its destination flag.
//!
//! Not yet modelled: a ware whose order changes while the carrier walks in, which `S2.EXE` was
//! not traced for. Such a ware is put down at the building's flag.

use crate::figures::{Carrier, CarrierState, Delivery, Figure, FigureKind, OnRoad};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::settlers::DOOR_SPEED;

impl Game {
    /// Carrier `record`, standing on the flag its `ware` is destined for, turns to walk it in
    /// through the door north-west of the flag (`0x34e64`, state 3).
    pub(super) fn start_delivery(&mut self, record: u16, carrier: &Carrier, ware: u16) {
        let Some(node) = self.world.figure(record).map(Figure::node) else {
            return;
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Delivering(Delivery {
                owner: carrier.owner,
                transport: carrier.transport,
                road: Some(carrier.on.road),
                ware: Some(ware),
                shows: carrier.shows,
                inside: true,
            });
        }
        let door = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        self.walk_through_door(record, door, Direction::NorthWest);
    }

    /// One step of a delivering carrier that is not walking. Inside the building it hands the
    /// ware over and walks back out (`0x35f52`). Back at the flag it is an idle carrier again at
    /// that end of its road, or lost when the road has gone (`0x363c2`).
    pub(crate) fn step_delivery(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Delivering(mut delivery) = figure.kind else {
            return;
        };
        let node = figure.node();
        if delivery.inside {
            let flag_node = node.neighbour(Direction::SouthEast, self.nodes.width(), self.nodes.height());
            if let Some(ware) = delivery.ware.take() {
                let flag = self
                    .world
                    .destination(ware)
                    .or_else(|| self.world.flag_at(flag_node));
                if let Some(flag) = flag {
                    self.world.put_down(ware, flag);
                }
            }
            delivery.shows = None;
            delivery.inside = false;
            if let Some(kind) = self.figure_kind_mut(record) {
                *kind = FigureKind::Delivering(delivery);
            }
            self.walk_through_door(record, flag_node, Direction::SouthEast);
            return;
        }
        self.world.move_figure(record, node);
        let flag = self.world.flag_at(node);
        let on = delivery.road.and_then(|road| {
            let record = self.world.road(road)?;
            let position = if Some(record.start) == flag {
                0
            } else {
                u8::try_from(record.edges()).unwrap_or(u8::MAX)
            };
            Some(OnRoad {
                road,
                position,
                to_start: false,
            })
        });
        let Some(on) = on else {
            self.lose_delivery(record, delivery);
            return;
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Carrier(Carrier {
                owner: delivery.owner,
                on,
                state: CarrierState::Idle,
                transport: delivery.transport,
                ware: None,
                shows: None,
                animation: None,
            });
        }
    }

    /// Moves figure `record` to `to` and starts its walk there towards `direction` at the door
    /// speed.
    fn walk_through_door(&mut self, record: u16, to: Node, direction: Direction) {
        self.world.move_figure(record, to);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, DOOR_SPEED);
        }
    }
}

#[cfg(test)]
mod tests;
