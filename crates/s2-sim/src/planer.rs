//! The planer: its arrival at a site's flag, the walk round the site digging and levelling five
//! nodes, then the site and its flag, and the walk home.

use crate::figures::{FigureKind, OnRoad, Planer, PlanerWork, Walker};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::homeward::{Crossing, HOME_SPEED};
use crate::job::Job;
use crate::settlers::HOME_MODE;
use crate::sites::refresh_sites;

/// The digging animation a planer plays on each node (`0x33b30`): a record in `ROM_ANIM.DAT`.
const DIGGING: u16 = 3110;

/// The nodes round the site a planer walks to and digs, before it sets the site and the flag.
const DUG: u8 = 5;

/// The direction from the node levelled `n`-th to the next is `n + 3`, modulo 6.
const TURN: usize = 3;

/// The nodes whose altitudes make a site's target: the site and its six neighbours.
const FOOTPRINT: u16 = 7;

impl Game {
    /// Planer walker `record` has reached its site's flag (`0x37e0a`, `0x2e626`): it takes the
    /// mean altitude of the site and its six neighbours, rounded down, as its target, frees the
    /// order and sets off across country to the flag's west neighbour. With no site it is lost.
    pub(crate) fn planer_arrives(&mut self, record: u16, order: u16) {
        let Some(FigureKind::Walker(walker)) = self.world.figure(record).map(|figure| figure.kind) else {
            return;
        };
        let flag = self.world.order(order).map(|order| order.flag);
        let site = flag
            .and_then(|flag| self.world.building_at_flag(flag))
            .and_then(|building| self.world.building(building))
            .map(|building| building.node);
        self.world.orders.remove(order);
        let (Some(flag), Some(site)) = (flag, site) else {
            self.make_lost(record);
            return;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let sum: u16 = std::iter::once(site)
            .chain(Direction::ALL.map(|direction| site.neighbour(direction, width, height)))
            .map(|node| u16::from(self.nodes.get(node).altitude))
            .sum();
        let target = u8::try_from(sum / FOOTPRINT).unwrap_or(u8::MAX);
        let Some(flag_node) = self.world.flag(flag).map(|flag| flag.node) else {
            self.make_lost(record);
            return;
        };
        let first = flag_node.neighbour(Direction::West, width, height);
        let Some(path) = self.find_path(flag_node, first, Crossing::Anywhere, false) else {
            self.make_lost(record);
            return;
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Planer(Planer {
                owner: walker.owner,
                flag,
                target,
                levelled: 0,
                work: PlanerWork::Walking { path },
            });
        }
    }

    /// One step of planer `record` that is not walking. On its way (`0x3473c`) it takes the
    /// next step of its path at speed 14 whatever the slope, and at the path's end it arrives.
    /// Digging (`0x32c20`), its animation takes a step, and the node is levelled in the step the
    /// animation ends.
    pub(crate) fn step_planer(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Planer(planer) = figure.kind else {
            return;
        };
        match planer.work {
            PlanerWork::Walking { path } => {
                let Some(walked) = self.world.paths.get(path).copied() else {
                    self.make_lost(record);
                    return;
                };
                let node = figure.node();
                if node == walked.to {
                    self.planer_reaches(record, planer, path, node);
                    return;
                }
                let mut ahead = walked;
                let Some(direction) = ahead.next() else {
                    self.make_lost(record);
                    return;
                };
                let next = node.neighbour(direction, self.nodes.width(), self.nodes.height());
                if self.held_up(record, next, path).blocked() {
                    return;
                }
                if let Some(stored) = self.world.paths.get_mut(path) {
                    *stored = ahead;
                }
                self.world.move_figure(record, next);
                if let Some(figure) = self.world.figure_mut(record) {
                    figure.start_walk(direction, HOME_SPEED);
                }
            }
            PlanerWork::Digging { animation } => {
                if animation.is_some_and(|animation| self.run_animation(animation, figure.node())) {
                    return;
                }
                self.level(record, planer, figure.node());
            }
        }
    }

    /// Planer `record` stands at the end of its path `path` on `node` (`0x33b30`): the path is
    /// freed. Back at the site's flag it walks home along the roads (`0x38b48`), or is lost with
    /// no storehouse or route. Anywhere else it starts digging, the first step of the animation
    /// at once (`0x339b8`).
    fn planer_reaches(&mut self, record: u16, planer: Planer, path: u16, node: Node) {
        self.world.paths.remove(path);
        let flag_node = self.world.flag(planer.flag).map(|flag| flag.node);
        if flag_node == Some(node) {
            if let Some(kind) = self.figure_kind_mut(record) {
                *kind = FigureKind::Walker(Walker {
                    owner: planer.owner,
                    job: Job::PLANER,
                    order: planer.flag,
                    mode: HOME_MODE,
                    on: OnRoad {
                        road: 0,
                        position: 0,
                        to_start: false,
                    },
                });
            }
            if !self.send_home(record, planer.flag) {
                self.make_lost(record);
            }
            return;
        }
        if flag_node.is_none() {
            self.make_lost(record);
            return;
        }
        let animation = self.start_animation(DIGGING, node);
        if let Some(FigureKind::Planer(planer)) = self.figure_kind_mut(record) {
            planer.work = PlanerWork::Digging { animation };
        }
    }

    /// Planer `record` has dug `node` (`0x37f78`, [`Job::PLANER`]): the node takes the target
    /// altitude. Before the fifth node it sets off for the next; after it the site and its flag
    /// take the target too and construction starts. Slot 8 is recomputed round `node`, and then
    /// a planer done with all five walks back to the flag over walkable nodes.
    fn level(&mut self, record: u16, planer: Planer, node: Node) {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        self.nodes.set_altitude(node, planer.target);
        let levelled = planer.levelled + 1;
        if let Some(FigureKind::Planer(planer)) = self.figure_kind_mut(record) {
            planer.levelled = levelled;
        }
        let mut lost = false;
        if levelled < DUG {
            let next = node.neighbour(Direction::from_index(usize::from(levelled) + TURN), width, height);
            lost = !self.set_off(record, node, next, Crossing::Anywhere);
        } else if let Some(building) = self.world.building_at_flag(planer.flag)
            && let Some(site) = self.world.building(building).map(|building| building.node)
        {
            self.nodes.set_altitude(site, planer.target);
            self.nodes
                .set_altitude(site.neighbour(Direction::SouthEast, width, height), planer.target);
            self.world.start_construction(building);
        }
        refresh_sites(&mut self.nodes, &self.world, node, 1);
        if levelled >= DUG {
            lost = match self.world.flag(planer.flag).map(|flag| flag.node) {
                Some(flag) => !self.set_off(record, node, flag, Crossing::Walkable),
                None => true,
            };
        }
        if lost {
            self.make_lost(record);
        }
    }

    /// Gives planer `record` a path from `from` to `to` (`0x33894`). Returns false when the
    /// search fails.
    fn set_off(&mut self, record: u16, from: Node, to: Node, crossing: Crossing) -> bool {
        let Some(path) = self.find_path(from, to, crossing, false) else {
            return false;
        };
        if let Some(FigureKind::Planer(planer)) = self.figure_kind_mut(record) {
            planer.work = PlanerWork::Walking { path };
        }
        true
    }
}

#[cfg(test)]
mod tests;
