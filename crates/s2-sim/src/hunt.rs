//! The hunter: it walks out towards game, looks for an animal
//! near where it stands, waits for it, shoots when the animal's own step lets it, then butchers
//! it and brings the meat home.

use crate::figures::{AnimalState, FigureKind, Gatherer, Trip};
use crate::game::Game;
use crate::gather::{Find, Search};
use crate::grid::{Direction, Node};
use crate::homeward::Crossing;
use crate::job::Job;
use crate::sites::refresh_sites;
use crate::ware::WareKind;

/// Searches further afield a hunter may make on one trip, set at its flag (`0x368ca`).
pub(crate) const SEARCHES: u8 = 6;

/// `searches` while the hunter butchers (`0x34413`).
const BUTCHERING: u8 = u8::MAX;

/// The shooting animation for each way the hunter faces, east first (`0x41a68`).
const SHOOTING: [u16; 6] = [2453, 2884, 2895, 2937, 2948, 2959];

/// The butchering animation (`0x343fe`).
const BUTCHERING_ANIMATION: u16 = 2906;

/// Search kind 8: prey within two steps of the hunter.
const PREY_NEAR: Search = Search {
    find: Find::Prey,
    centre: false,
    radius: 2,
    hits: 1,
    same_area: true,
};

/// Search kind 9: prey within 21 steps, from the building at the start or from the hunter.
pub(crate) const PREY_FAR: Search = Search {
    find: Find::Prey,
    centre: false,
    radius: 21,
    hits: 1,
    same_area: true,
};

impl Game {
    /// The first animal on `node` a hunter can take (`0x2f0f6`, `0x420c8`): one of a species
    /// the spawner counts, with no hunter after it. Its state is not looked at.
    pub(crate) fn prey_at(&self, node: Node) -> Option<u16> {
        self.world
            .figures_at(node)
            .find_map(|(record, figure)| match figure.kind {
                FigureKind::Animal {
                    species,
                    hunter: None,
                    ..
                } if species.traits().counted => Some(record),
                _ => None,
            })
    }

    /// Hunter `record` is at the end of a path on `node` (`0x343e4`). With no prey it looks
    /// for one; with prey it butchers it, whatever is there.
    pub(crate) fn hunter_arrives(&mut self, record: u16, gatherer: Gatherer, node: Node, flag: Node) {
        match gatherer.prey {
            None => self.look_for_prey(record, gatherer, node, flag),
            Some(prey) => {
                let animation = self.start_animation_at(BUTCHERING_ANIMATION, node);
                self.world.move_figure(prey, node);
                self.set_hunter(record, |hunter| {
                    hunter.searches = BUTCHERING;
                    hunter.trip = Trip::Working { animation };
                });
            }
        }
    }

    /// The hunting search (`0x33ff0`): prey within two steps is picked and the hunter waits for
    /// it. Otherwise, with searches left, it heads for prey up to 21 steps away, and is lost
    /// with no path there; with none found or none left it goes home.
    fn look_for_prey(&mut self, record: u16, gatherer: Gatherer, node: Node, flag: Node) {
        let hits = self.search(node, PREY_NEAR);
        if let Some(target) = self.pick(&hits) {
            let Some(prey) = self.prey_at(target) else {
                return;
            };
            if let Some(FigureKind::Animal { hunter, .. }) = self.figure_kind_mut(prey) {
                *hunter = Some(record);
            }
            self.set_hunter(record, |hunter| {
                hunter.prey = Some(prey);
                hunter.trip = Trip::Waiting;
            });
            return;
        }
        if gatherer.searches == 0 {
            self.head_back(record, node, flag);
            return;
        }
        self.set_hunter(record, |hunter| hunter.searches -= 1);
        let hits = self.search(node, PREY_FAR);
        let Some(target) = self.pick(&hits) else {
            self.head_back(record, node, flag);
            return;
        };
        match self.find_path(node, target, Crossing::Walkable, false) {
            Some(path) => self.set_hunter(record, |hunter| hunter.trip = Trip::Walking { path }),
            None => self.make_lost(record),
        }
    }

    /// Animal `animal`, picked by hunter `hunter`, stops to be shot (`0x41c6e`): the hunter
    /// starts the shooting animation for the way from it to the animal, and the animal stands
    /// still.
    pub(crate) fn shoot(&mut self, hunter: u16, animal: u16) {
        if let (Some(from), Some(to)) = (
            self.world.figure(hunter).map(crate::figures::Figure::node),
            self.world.figure(animal).map(crate::figures::Figure::node),
        ) && matches!(
            self.world.figure(hunter).map(|figure| figure.kind),
            Some(FigureKind::Gatherer(_))
        ) {
            let facing = facing(from, to);
            let animation = self.start_animation_at(SHOOTING[facing.index()], from);
            self.set_hunter(hunter, |hunter| hunter.trip = Trip::Working { animation });
        }
        if let Some(figure) = self.world.figure_mut(animal) {
            if let FigureKind::Animal { state, .. } = &mut figure.kind {
                *state = AnimalState::Shot;
            }
            figure.counter = 0;
        }
    }

    /// Hunter `record`'s animation on `node` has ended (`0x383c5`, `0x38434`). After the shot
    /// it heads for the animal, which is now dead. After butchering it takes the meat, the
    /// carcass goes, and it heads home.
    pub(crate) fn hunter_done(&mut self, record: u16, gatherer: Gatherer, node: Node, flag: Option<Node>) {
        let Some(prey) = gatherer.prey else {
            refresh_sites(&mut self.nodes, &self.world, node, 1);
            return;
        };
        if gatherer.searches == BUTCHERING {
            self.set_hunter(record, |hunter| {
                hunter.job = Job::CARRYING_MEAT;
                hunter.prey = None;
            });
            self.world.count_ware(gatherer.owner, WareKind::MEAT, 1);
            self.world.remove_figure(prey);
            refresh_sites(&mut self.nodes, &self.world, node, 1);
            match flag {
                Some(flag) => self.head_back(record, node, flag),
                None => self.make_lost(record),
            }
            return;
        }
        let carcass = self.world.figure(prey).map(crate::figures::Figure::node);
        match carcass.and_then(|carcass| self.find_path(node, carcass, Crossing::Walkable, false)) {
            Some(path) => self.set_hunter(record, |hunter| hunter.trip = Trip::Walking { path }),
            None => self.make_lost(record),
        }
        if let Some(FigureKind::Animal { state, .. }) = self.figure_kind_mut(prey) {
            *state = AnimalState::Dead;
        }
        refresh_sites(&mut self.nodes, &self.world, node, 1);
    }

    fn set_hunter(&mut self, record: u16, change: impl FnOnce(&mut Gatherer)) {
        if let Some(FigureKind::Gatherer(hunter)) = self.figure_kind_mut(record) {
            change(hunter);
        }
    }
}

/// The way from `from` to `to` as the hunter faces it (`0x22df8`). Columns count half nodes,
/// with odd rows half a node to the right, and rows count double. Equal nodes face east.
pub(crate) fn facing(from: Node, to: Node) -> Direction {
    let column = |node: Node| 2 * i32::from(node.x) + i32::from(node.y % 2);
    let across = column(to) - column(from);
    let down = 2 * (i32::from(to.y) - i32::from(from.y));
    if across >= 0 {
        if across < down {
            Direction::SouthEast
        } else if -across > down {
            Direction::NorthEast
        } else {
            Direction::East
        }
    } else if -across < down {
        Direction::SouthWest
    } else if across > down {
        Direction::NorthWest
    } else {
        Direction::West
    }
}

#[cfg(test)]
mod tests;
