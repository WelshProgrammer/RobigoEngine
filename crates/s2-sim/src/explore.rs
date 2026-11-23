//! Geologists and scouts the player sends to a flag: the order, the work cross-country round the
//! flag and the walk home.

use crate::figures::{Explorer, Exploring, FigureKind, Walker};
use crate::game::Game;
use crate::gather::{Find, Search};
use crate::grid::Node;
use crate::ground::Property;
use crate::homeward::{Crossing, HOME_SPEED};
use crate::job::Job;
use crate::nodes::{MINERAL, Mineral, Resources};
use crate::settlers::HOME_MODE;
use crate::sites::refresh_sites;
use crate::territory::claim_order;
use crate::world::{MapObject, Player, Sign};

/// The tag of an order for a geologist or scout the player sent (`+0x0a`).
pub(crate) const SENT_TAG: u8 = 3;

/// The mode of a geologist's or scout's order.
/// How far round a find a sign of the same kind stops the message (`0x380ea`).
const SIGN_LOOK: u8 = 4;

const SENT_MODE: u8 = 0;

/// Most nodes a geologist goes to before it heads back (`0x37ee8`).
const GEOLOGIST_VISITS: u8 = 15;

/// Search kind 3: nodes a geologist can examine round its flag.
const GEOLOGIST_SEARCH: Search = Search {
    find: Find::Surveyable,
    centre: false,
    radius: 12,
    hits: 20,
    same_area: true,
};

/// Search kind 4: land the scout's owner has not seen round its flag.
const fn scout_search(owner: Player) -> Search {
    Search {
        find: Find::Unseen(owner),
        centre: false,
        radius: 15,
        hits: 3,
        same_area: true,
    }
}

/// How far round the node ahead a scout sees as it sets off along each edge (`0x34ae2`).
const SCOUT_SIGHT: u16 = 4;

/// The geologist's animations in `ROM_ANIM.DAT`.
const TAPPING: u16 = 3507;
const CELEBRATING: u16 = 3617;

/// The sign for a mineral: iron ore 0, gold 1, coal 2, granite 3.
const fn mineral_sign(mineral: Mineral) -> u8 {
    match mineral {
        Mineral::IronOre => 0,
        Mineral::Gold => 1,
        Mineral::Coal => 2,
        Mineral::Granite => 3,
    }
}

impl Game {
    /// Places an order for a geologist or scout at flag `flag`, as the action window's button
    /// does (`0x5bc6f`, `0x2aefc`). Returns the order.
    pub(crate) fn send_explorer(&mut self, flag: u16, job: Job) -> Option<u16> {
        self.world.order_person(flag, job, SENT_MODE, SENT_TAG)
    }

    /// Flag `flag` is going (`0x25763`): each order the player placed there is freed
    /// (`0x2becc`). A settler already on its way heads home from its road's start flag, or is
    /// lost with no storehouse to go to, and one still in a storehouse door forgets the order.
    pub(crate) fn cancel_sent(&mut self, flag: u16) {
        let orders: Vec<_> = self
            .world
            .orders()
            .filter(|(_, order)| order.flag == flag && order.tag == SENT_TAG)
            .map(|(record, order)| (record, *order))
            .collect();
        for (record, order) in orders {
            if let Some(on_way) = order.on_way.filter(|_| order.delivering) {
                match self.world.figure(on_way).map(|figure| figure.kind) {
                    Some(FigureKind::Walker(walker)) => {
                        let start = self.world.road(walker.on.road).map(|road| road.start);
                        if start.and_then(|start| self.aim_home(on_way, start)).is_none() {
                            self.make_lost(on_way);
                        }
                    }
                    Some(FigureKind::Leaving { .. }) => {
                        if let Some(FigureKind::Leaving { order, .. }) = self.figure_kind_mut(on_way) {
                            *order = None;
                        }
                    }
                    _ => {}
                }
            }
            self.world.orders.remove(record);
        }
    }

    /// Flag `flag` is gone (`0x258c2`): every geologist or scout walking cross-country from it
    /// is lost. One busy at a node carries on round where the flag stood.
    pub(crate) fn lose_explorers(&mut self, flag: u16) {
        for record in 0..self.world.figure_records() {
            if let Some(FigureKind::Explorer(Explorer {
                flag: from,
                work: Exploring::Walking { .. },
                ..
            })) = self.world.figure(record).map(|figure| figure.kind)
                && from == flag
            {
                self.make_lost(record);
            }
        }
    }

    /// The search a geologist or scout of `owner` makes round its flag.
    const fn explorer_search(job: Job, owner: Player) -> Search {
        if job.0 == Job::SCOUT.0 {
            scout_search(owner)
        } else {
            GEOLOGIST_SEARCH
        }
    }

    /// Walker `record`, sent by the player, has reached the flag of its order `order`
    /// (`0x37a50`): the order is freed and the figure starts work round the flag. With nothing
    /// to go to it goes home; a geologist with no way to its first node is lost, a scout goes
    /// home.
    pub(crate) fn explorer_arrives(&mut self, record: u16, walker: Walker) {
        self.world.orders.remove(walker.order);
        let Some(node) = self.world.figure(record).map(crate::figures::Figure::node) else {
            return;
        };
        let Some(flag) = self.world.flag_at(node) else {
            self.make_lost(record);
            return;
        };
        let hits = self.search(node, Self::explorer_search(walker.job, walker.owner));
        let path = self
            .pick(&hits)
            .map(|target| self.find_path(node, target, Crossing::Walkable, true));
        let path = match path {
            Some(Some(path)) => path,
            Some(None) if walker.job == Job::GEOLOGIST => {
                self.make_lost(record);
                return;
            }
            _ => {
                if !self.send_home(record, flag) {
                    self.make_lost(record);
                }
                return;
            }
        };
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Explorer(Explorer {
                owner: walker.owner,
                job: walker.job,
                flag,
                centre: node,
                visits: 0,
                work: Exploring::Walking { path },
            });
        }
        self.walk_path(record, false);
    }

    /// One step of explorer `record` that is not walking.
    pub(crate) fn step_explorer(&mut self, record: u16) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Explorer(explorer) = figure.kind else {
            return;
        };
        match explorer.work {
            Exploring::Walking { .. } => self.walk_path(record, true),
            Exploring::Tapping { animation } | Exploring::Celebrating { animation } => {
                if animation.is_some_and(|animation| self.run_animation_at(animation, figure.node())) {
                    return;
                }
                self.examine(record);
            }
        }
    }

    fn set_work(&mut self, record: u16, work: Exploring) {
        if let Some(FigureKind::Explorer(explorer)) = self.figure_kind_mut(record) {
            explorer.work = work;
        }
    }

    /// The next edge of explorer `record`'s path (`0x3473c`), or its arrival at the end. With
    /// `ahead`, a scout sees round the node the edge leads to; the first edge from the flag
    /// starts outside the walker handler and sees nothing.
    fn walk_path(&mut self, record: u16, ahead: bool) {
        let Some(figure) = self.world.figure(record).copied() else {
            return;
        };
        let FigureKind::Explorer(Explorer {
            work: Exploring::Walking { path },
            owner,
            job,
            ..
        }) = figure.kind
        else {
            return;
        };
        let Some(walked) = self.world.paths.get(path).copied() else {
            self.make_lost(record);
            return;
        };
        let node = figure.node();
        if node == walked.to {
            self.world.paths.remove(path);
            self.reach(record, node);
            return;
        }
        let mut walk = walked;
        let Some(direction) = walk.next() else {
            self.make_lost(record);
            return;
        };
        let next = node.neighbour(direction, self.nodes.width(), self.nodes.height());
        if self.held_up(record, next, path).blocked() {
            return;
        }
        if let Some(stored) = self.world.paths.get_mut(path) {
            *stored = walk;
        }
        if ahead {
            self.scout_ahead(job, owner, next);
        }
        self.world.move_figure(record, next);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, HOME_SPEED);
        }
    }

    /// A settler of job `job` about to step onto `next` cross-country: a scout shows its owner
    /// the land round `next`, unless the owner has already seen all of `next` (`0x34ae2`).
    pub(crate) fn scout_ahead(&mut self, job: Job, owner: Player, next: Node) {
        if job == Job::SCOUT && !self.nodes.fully_seen(next, owner) {
            self.nodes.reveal(next, owner, SCOUT_SIGHT);
        }
    }

    /// Where explorer `explorer`'s flag stands: the flag record's node, or where it stood once
    /// the record is free.
    fn explorer_centre(&self, explorer: Explorer) -> Node {
        self.world
            .flag(explorer.flag)
            .map_or(explorer.centre, |flag| flag.node)
    }

    /// Explorer `record` is at the end of its path on `node` (`0x33b30`). Back at the flag it
    /// goes home by road. A geologist at a node it can examine, and alone there, starts tapping;
    /// otherwise it moves on.
    fn reach(&mut self, record: u16, node: Node) {
        let Some(FigureKind::Explorer(explorer)) = self.world.figure(record).map(|figure| figure.kind) else {
            return;
        };
        if node == self.explorer_centre(explorer) {
            self.explorer_home(record, explorer);
            return;
        }
        if explorer.job == Job::SCOUT {
            self.scout_on(record, node, explorer);
            return;
        }
        let alone = self
            .world
            .figures_at(node)
            .filter(|(_, figure)| figure.is_settler())
            .count()
            == 1;
        if alone && self.surveyable(node) {
            let animation = self.start_animation_at(TAPPING, node);
            self.set_work(record, Exploring::Tapping { animation });
        } else {
            self.move_on(record, node, false);
        }
    }

    /// Scout `record` has reached a target on `node` (`0x33f32`): it counts the target, which
    /// nothing reads, and sets off for more unseen land round its flag from the next step, or
    /// heads back to the flag when there is none. With no way to the next target, or no flag to
    /// go back to, it is lost.
    fn scout_on(&mut self, record: u16, node: Node, explorer: Explorer) {
        if let Some(FigureKind::Explorer(explorer)) = self.figure_kind_mut(record) {
            explorer.visits = explorer.visits.wrapping_add(1);
        }
        let Some(flag) = self.world.flag(explorer.flag).map(|flag| flag.node) else {
            self.make_lost(record);
            return;
        };
        let hits = self.search(flag, scout_search(explorer.owner));
        let target = self.pick(&hits).unwrap_or(flag);
        match self.find_path(node, target, Crossing::Walkable, false) {
            Some(path) => self.set_work(record, Exploring::Walking { path }),
            None => self.make_lost(record),
        }
    }

    /// Explorer `record` is back at its flag (`0x33e00`): it turns into a walker going home to
    /// the nearest storehouse that takes it, or is lost when there is none or the flag is gone.
    fn explorer_home(&mut self, record: u16, explorer: Explorer) {
        if self.world.flag(explorer.flag).is_none() {
            self.make_lost(record);
            return;
        }
        if let Some(kind) = self.figure_kind_mut(record) {
            *kind = FigureKind::Walker(Walker {
                owner: explorer.owner,
                job: explorer.job,
                order: explorer.flag,
                mode: HOME_MODE,
                on: crate::figures::OnRoad {
                    road: 0,
                    position: 0,
                    to_start: false,
                },
            });
        }
        if !self.send_home(record, explorer.flag) {
            self.make_lost(record);
        }
    }

    /// Geologist `record` has finished an animation on its node (`0x380ea`). A node it can still
    /// examine gets a sign from what lies under it. A find starts the pleased animation, after
    /// which this runs again and finds the sign; anything else moves on.
    fn examine(&mut self, record: u16) {
        let Some(node) = self.world.figure(record).map(crate::figures::Figure::node) else {
            return;
        };
        if !self.surveyable(node) {
            self.move_on(record, node, true);
            return;
        }
        let (found, size) = self.find_under(node);
        self.world
            .set_object(node, MapObject::Sign(Sign::placed(found, size)));
        if found == Sign::NOTHING {
            self.move_on(record, node, true);
            return;
        }
        self.note_find(record, node, found);
        let animation = self.start_animation_at(CELEBRATING, node);
        self.set_work(record, Exploring::Celebrating { animation });
    }

    /// Geologist `record`'s find of `found` at `node` sets its owner's found word and posts a
    /// message, unless a sign of the same kind already stands within [`SIGN_LOOK`] steps
    /// (`0x380ea`).
    fn note_find(&mut self, record: u16, node: Node, found: u8) {
        let Some(FigureKind::Explorer(explorer)) = self.world.figure(record).map(|figure| figure.kind) else {
            return;
        };
        let owner = explorer.owner;
        self.world.note_found_sign(owner, found);
        let size = (self.nodes.width(), self.nodes.height());
        let seen = claim_order(size, node, SIGN_LOOK)
            .into_iter()
            .skip(1)
            .any(|at| matches!(self.world.object(at), MapObject::Sign(sign) if sign.found == found));
        if !seen {
            self.post_message(
                crate::messages::MessageId::GEOLOGIST,
                owner,
                Some(node),
                None,
                u32::from(found),
            );
        }
    }

    /// What a geologist finds under `node` and the sign's size before capping (`0x380ea`): on
    /// mountain, the mineral its bits name; on meadow-class or class 6 ground, water; with no
    /// amount, nothing. On any other ground `S2.EXE` passes the raw mineral bits as the sign
    /// kind, which makes a fire for iron ore and granite; Robigo shows the sign for what the node
    /// holds instead.
    fn find_under(&self, node: Node) -> (u8, u8) {
        let resources = self.nodes.get(node).resources;
        let (mineral, amount) = (resources.mineral_bits(), resources.amount());
        if amount == 0 {
            return (Sign::NOTHING, 0);
        }
        let size = amount - 1;
        let property = self.nodes.property(node);
        if property.contains(Property::ALL_MOUNTAIN) {
            return (mineral_sign(Mineral::of(MINERAL.get(mineral))), size);
        }
        let water_ground = property.intersects(Property::ALL_BUILDABLE | Property::ALL_CLASS_6);
        if water_ground && mineral == 0 {
            return (Sign::WATER, 0);
        }
        match resources {
            Resources::Mineral(mineral, _) => (mineral_sign(mineral), size),
            Resources::Water(_) => (Sign::WATER, 0),
            _ => (Sign::NOTHING, 0),
        }
    }

    /// Geologist `record` on `node` moves on (`0x37ee8`): after its fifteenth node, or with
    /// nothing left to examine round the flag, it heads back to the flag; otherwise it sets off
    /// to the next node from the next step. `refreshed` recomputes the node's sites before it
    /// heads back, as it does after an animation.
    fn move_on(&mut self, record: u16, node: Node, refreshed: bool) {
        let Some(FigureKind::Explorer(explorer)) = self.figure_kind_mut(record).map(|kind| {
            if let FigureKind::Explorer(explorer) = kind {
                explorer.visits += 1;
            }
            *kind
        }) else {
            return;
        };
        let target = if explorer.visits < GEOLOGIST_VISITS {
            let hits = self.search(self.explorer_centre(explorer), GEOLOGIST_SEARCH);
            self.pick(&hits)
        } else {
            None
        };
        let Some(target) = target else {
            self.head_for_flag(record, node, explorer, refreshed);
            return;
        };
        match self.find_path(node, target, Crossing::Walkable, false) {
            Some(path) => self.set_work(record, Exploring::Walking { path }),
            None => self.make_lost(record),
        }
    }

    /// Explorer `record` on `node` sets off back to its flag (`0x37f78`, `0x3875c`), or is lost
    /// with no way there.
    fn head_for_flag(&mut self, record: u16, node: Node, explorer: Explorer, refreshed: bool) {
        if refreshed {
            refresh_sites(&mut self.nodes, &self.world, node, 1);
        }
        let flag = self.explorer_centre(explorer);
        match self.find_path(node, flag, Crossing::Walkable, false) {
            Some(path) => self.set_work(record, Exploring::Walking { path }),
            None => self.make_lost(record),
        }
    }

    /// Whether a geologist can examine `node` (`0x19923`): figures can walk on it and it holds
    /// no sign, flag or tree.
    pub(crate) fn surveyable(&self, node: Node) -> bool {
        self.nodes.get(node).sites.passable()
            && !matches!(
                self.world.object(node),
                MapObject::Sign(_) | MapObject::Flag(_) | MapObject::Tree(_)
            )
    }
}

#[cfg(test)]
mod tests;
