//! Ships: the ship records, launching a finished ship, and sailing.

use crate::figures::{Figure, FigureKind};
use crate::game::Game;
use crate::gather::{Find, Search};
use crate::grid::{Direction, Node};
use crate::homeward::{MAX_PATHS, Path};
use crate::job::Job;
use crate::world::{Player, WARE_TYPES};

mod cargo;
mod expedition;

pub use cargo::CargoWeights;

/// The most ship records (`0x253da`).
pub const MAX_SHIPS: usize = 200;

/// The sectors an expedition looks for harbour sites in: north, north-east, south-east, south,
/// south-west and north-west of the ship.
pub const SECTORS: usize = 6;

/// A ship's speed, the same on every edge whatever the terrain.
const SHIP_SPEED: u8 = 16;

/// How far round each node it sails onto a ship reveals the map.
const SAILING_SIGHT: u16 = 3;

/// A ship with no harbour looks for one on steps that are a multiple of this.
const HOMELESS_PERIOD: u32 = 120;

/// A resting ship with a harbour plans when its record number plus the step is a multiple of
/// this.
const PLANNING_PERIOD: u32 = 30;

/// How far an anchorage may be from the landing node, and how many the search keeps (search
/// kind 17).
const ANCHORAGE_REACH: u16 = 10;
const ANCHORAGES: usize = 6;

/// The order `0x414d5` looks round a finished ship's node for the sea it floats on.
const SEA_LOOK: [Direction; 6] = [
    Direction::NorthWest,
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
];

/// What a ship does (record byte 0, bits 3 to 5), with the record fields each mode reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ShipMode {
    /// 8: carries wares and people between harbours, serving harbour record `harbour`
    /// (`+0x04`).
    Transport {
        /// The harbour record it serves or sails to.
        harbour: Option<u16>,
    },
    /// 24: carries an expedition, sailing to `target` (`+0x04`, `+0x06`) once the player
    /// picks a way.
    Expedition {
        /// The node next to the harbour site it sails to.
        target: Option<Node>,
    },
    /// 32: moving off to an anchorage, still serving `harbour`.
    Anchoring {
        /// The harbour record it serves.
        harbour: Option<u16>,
    },
}

/// A ship record (170 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Ship {
    /// Owner (`+0x01`).
    pub owner: Player,
    /// Its figure (`+0x02`).
    pub figure: u16,
    /// Sailing a path (phase 2); otherwise at rest (phase 3).
    pub sailing: bool,
    /// What it does.
    pub mode: ShipMode,
    /// Its path record (`+0x08`).
    pub path: Option<u16>,
    /// Wares aboard by type (`+0x0a`).
    pub wares: [u16; WARE_TYPES],
    /// People aboard by job (`+0x48`).
    pub people: [u16; Job::PEOPLE],
    /// The area of the sea it floats on (`+0x99`), set at launch and never changed.
    pub sea: u8,
    /// On an expedition, the sectors with a harbour site (`+0x9a`).
    pub sectors: [bool; SECTORS],
    /// On an expedition, whether the node it stopped on belongs to nobody (`+0xa6`).
    pub unclaimed: bool,
    /// Its number among the owner's ships, from 1 (`+0xa8`).
    pub number: u16,
}

impl Ship {
    /// A new ship of `owner` on sea `sea` (`0x40623`): at rest, carrying nothing, with no
    /// harbour.
    #[must_use]
    pub const fn new(owner: Player, sea: u8) -> Self {
        Self {
            owner,
            figure: 0,
            sailing: false,
            mode: ShipMode::Transport { harbour: None },
            path: None,
            wares: [0; WARE_TYPES],
            people: [0; Job::PEOPLE],
            sea,
            sectors: [false; SECTORS],
            unclaimed: false,
            number: 0,
        }
    }
}

impl Game {
    /// A ship of `owner` floats from the site on `node` (`0x414d5`): on the sea of the first
    /// neighbour, north-west round to west, in a body of water, or of the west neighbour with
    /// none. Returns its figure, or `None` when the ship pool is full and the ship is lost.
    pub(crate) fn launch_ship(&mut self, node: Node, owner: Player) -> Option<u16> {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let mut sea = 0;
        for direction in SEA_LOOK {
            sea = self.nodes.get(node.neighbour(direction, width, height)).area;
            if self.nodes.is_sea(sea) {
                break;
            }
        }
        let record = self.world.ships.insert(Ship::new(owner, sea), MAX_SHIPS)?;
        let figure = self
            .world
            .add_figure(node, Figure::new(FigureKind::Ship { owner, record }));
        let count = &mut self.world.ship_counts[owner.index()];
        *count = count.wrapping_add(1);
        let number = *count;
        if let Some(ship) = self.world.ships.get_mut(record) {
            ship.figure = figure;
            ship.number = number;
        }
        Some(figure)
    }

    /// One step of ship figure `figure` (`0x4113c`). Sailing an edge, it moves on. At rest, a
    /// transport ship looks for work and a ship back from an anchorage serves its harbour
    /// again. Otherwise it sails on towards its harbour's landing node, its expedition's target
    /// or its anchorage, arriving there or taking the next edge. A route used up on the way is
    /// searched again from where it stands; with none the ship stops where it is.
    pub(crate) fn step_ship(&mut self, figure: u16) {
        let Some(state) = self.world.figure_mut(figure) else {
            return;
        };
        if state.walking() {
            state.advance_walk();
            return;
        }
        let (node, FigureKind::Ship { owner, record }) = (state.node(), state.kind) else {
            return;
        };
        let Some(ship) = self.world.ships.get(record).copied() else {
            return;
        };
        if !ship.sailing {
            match ship.mode {
                ShipMode::Transport { .. } => self.idle_ship(record, node),
                ShipMode::Expedition { .. } => {}
                ShipMode::Anchoring { harbour } => {
                    self.set_ship_mode(record, ShipMode::Transport { harbour });
                }
            }
            return;
        }
        let destination = match ship.mode {
            ShipMode::Transport { harbour } => {
                harbour.and_then(|harbour| self.world.landing(harbour, ship.sea))
            }
            ShipMode::Expedition { target } => target,
            ShipMode::Anchoring { .. } => ship
                .path
                .and_then(|path| self.world.paths.get(path))
                .map(|path| path.to),
        };
        let (Some(destination), Some(path)) = (destination, ship.path) else {
            self.stop_ship(record);
            return;
        };
        if node == destination {
            self.stop_ship(record);
            match ship.mode {
                ShipMode::Transport { .. } => self.ship_arrives(record, node),
                ShipMode::Expedition { .. } => self.expedition_arrives(record, node),
                ShipMode::Anchoring { harbour } => {
                    self.set_ship_mode(record, ShipMode::Transport { harbour });
                }
            }
            return;
        }
        let mut step = self.world.paths.get_mut(path).and_then(Path::next);
        if step.is_none() && self.sea_route(path, node, destination, ship.sea) {
            step = self.world.paths.get_mut(path).and_then(Path::next);
        }
        let Some(direction) = step else {
            self.stop_ship(record);
            return;
        };
        let next = node.neighbour(direction, self.nodes.width(), self.nodes.height());
        if !self.nodes.fully_seen(next, owner) {
            self.nodes.reveal(next, owner, SAILING_SIGHT);
        }
        self.world.move_figure(figure, next);
        if let Some(figure) = self.world.figure_mut(figure) {
            figure.start_walk(direction, SHIP_SPEED);
        }
    }

    fn set_ship_mode(&mut self, record: u16, mode: ShipMode) {
        if let Some(ship) = self.world.ships.get_mut(record) {
            ship.mode = mode;
        }
    }

    /// Ship `record` comes to rest and its path record is freed (`0x3d5ac`).
    fn stop_ship(&mut self, record: u16) {
        let Some(ship) = self.world.ships.get_mut(record) else {
            return;
        };
        ship.sailing = false;
        if let Some(path) = ship.path.take() {
            self.world.paths.remove(path);
        }
    }

    /// Ship `record` sets off on sea `sea` from `from` to `to` along path record `path`, or frees
    /// the record when there is no route. Returns whether it set off.
    fn set_sail(&mut self, record: u16, path: u16, from: Node, to: Node, sea: u8) -> bool {
        if !self.sea_route(path, from, to, sea) {
            self.world.paths.remove(path);
            return false;
        }
        if let Some(ship) = self.world.ships.get_mut(record) {
            ship.path = Some(path);
            ship.sailing = true;
        }
        true
    }

    /// A transport ship `record` at rest on `node` looks for work (`0x40c5f`). With no harbour,
    /// every 120 steps it heads for the first of its owner's harbours on its sea, or unloads
    /// there when already on its landing node. With a harbour, on its turn every 30 steps, and
    /// only when some harbour has work for it, it sails back to its harbour, or arrives there at
    /// once when already on the landing node. `S2.EXE` leaves the path record taken for that
    /// voyage allocated for good; Robigo frees it, as a full path pool stops every figure walking
    /// cross-country.
    fn idle_ship(&mut self, record: u16, node: Node) {
        let Some(ship) = self.world.ships.get(record).copied() else {
            return;
        };
        let ShipMode::Transport { harbour } = ship.mode else {
            return;
        };
        let step = self.clock.step();
        let Some(harbour) = harbour else {
            if !step.is_multiple_of(HOMELESS_PERIOD) {
                return;
            }
            let Some(found) = self.world.first_harbour(ship.owner, ship.sea) else {
                return;
            };
            let Some(landing) = self.world.landing(found, ship.sea) else {
                return;
            };
            let Some(path) = self.world.paths.insert(Path::empty(landing), MAX_PATHS) else {
                return;
            };
            if node == landing {
                self.set_ship_mode(record, ShipMode::Transport { harbour: Some(found) });
                self.unload(record, found);
                self.world.paths.remove(path);
            } else if self.set_sail(record, path, node, landing, ship.sea) {
                self.set_ship_mode(record, ShipMode::Transport { harbour: Some(found) });
            }
            return;
        };
        if !step
            .wrapping_add(u32::from(record))
            .is_multiple_of(PLANNING_PERIOD)
        {
            return;
        }
        if self.choose_harbour(harbour, ship.sea).is_none() {
            return;
        }
        let Some(landing) = self.world.landing(harbour, ship.sea) else {
            return;
        };
        let Some(path) = self.world.paths.insert(Path::empty(landing), MAX_PATHS) else {
            return;
        };
        if node == landing {
            self.world.paths.remove(path);
            self.ship_arrives(record, node);
            return;
        }
        self.set_sail(record, path, node, landing, ship.sea);
    }

    /// Ship `record` is at its harbour's landing node `node` (`0x40fdb`): it unloads and takes an
    /// expedition ready there aboard, or loads for the harbour [`Self::choose_harbour`] picks and
    /// sails there. With no harbour to sail to,
    /// or its own, it moves off to an anchorage. With no route it rests, loaded, until its next
    /// turn sends it on.
    fn ship_arrives(&mut self, record: u16, node: Node) {
        let Some(ship) = self.world.ships.get(record).copied() else {
            return;
        };
        let ShipMode::Transport { harbour: Some(home) } = ship.mode else {
            return;
        };
        self.unload(record, home);
        if self.board_expedition(record, home) {
            return;
        }
        let next = self.choose_harbour(home, ship.sea);
        let Some(next) = next.filter(|&next| next != home) else {
            self.anchor(record, node);
            return;
        };
        self.load(record, home, next);
        self.set_ship_mode(record, ShipMode::Transport { harbour: Some(next) });
        let Some(landing) = self.world.landing(next, ship.sea) else {
            return;
        };
        let Some(path) = self.world.paths.insert(Path::empty(landing), MAX_PATHS) else {
            return;
        };
        self.set_sail(record, path, node, landing, ship.sea);
    }

    /// Ship `record` on `node` moves off to a free node of its sea within 10, picked by a draw
    /// (`0x40ec3`). With none found, or no route, the ship rests where it is. `S2.EXE` leaves the
    /// path record allocated for good when none is found; Robigo frees it.
    fn anchor(&mut self, record: u16, node: Node) {
        let Some(ship) = self.world.ships.get(record).copied() else {
            return;
        };
        let ShipMode::Transport { harbour } = ship.mode else {
            return;
        };
        let Some(path) = self.world.paths.insert(Path::empty(node), MAX_PATHS) else {
            return;
        };
        let hits = self.search(
            node,
            Search {
                find: Find::Anchorage(ship.sea),
                centre: false,
                radius: ANCHORAGE_REACH,
                hits: ANCHORAGES,
                same_area: false,
            },
        );
        let Some(anchorage) = self.pick(&hits) else {
            self.world.paths.remove(path);
            return;
        };
        if self.set_sail(record, path, node, anchorage, ship.sea) {
            self.set_ship_mode(record, ShipMode::Anchoring { harbour });
        }
    }

    /// Harbour `harbour` is going (`0x40757`): every transport ship serving it loses it, stops
    /// after the edge it is on and keeps its cargo. A ship moving off to an anchorage keeps the
    /// record number.
    pub(crate) fn strand_ships(&mut self, harbour: u16) {
        let serving: Vec<u16> = self
            .world
            .ships()
            .filter(|(_, ship)| {
                ship.mode
                    == ShipMode::Transport {
                        harbour: Some(harbour),
                    }
            })
            .map(|(record, _)| record)
            .collect();
        for record in serving {
            self.set_ship_mode(record, ShipMode::Transport { harbour: None });
            self.stop_ship(record);
        }
    }

    /// Whether `node` is a free anchorage on sea `sea` (`0x30c5f`): on that sea, with no figure on
    /// it.
    pub(crate) fn anchorage(&self, node: Node, sea: u8) -> bool {
        self.nodes.get(node).area == sea && self.world.figures_at(node).next().is_none()
    }
}

#[cfg(test)]
mod tests;
