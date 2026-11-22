//! Settlers (`0x1aee4` from `0x1b4e3`): carriers, settlers walking the roads, into or out of a
//! building, lost settlers and their walk home, pack donkeys and carriers' idle animations, and the
//! frames of any animation, a bitmap or a figure from the job table.
//!
//! Not drawn yet: boats, the wares carriers and donkeys hold, and carriers walking into a
//! building.

use s2_assets::{BobParts, BobSprites, Nation, Settlers};
use s2_formats::bob::Sprite as AnimSprite;
use s2_formats::bob::{BodyType, Direction as BobDirection};
use s2_sim::{
    CarrierState, Exploring, Figure, FigureKind, Job, MapObject, Node, PlanerWork, Player, SoldierState,
    Transport, Trip,
};

use super::{ObjectPass, darken};
use crate::Layers;
use crate::sprite::{Paint, colour_base};
use crate::terrain::{Point, Surface};

/// The frame of a settler that is not walking, or is held at a storehouse door (`0x1bccd`).
const STANDING_FRAME: u8 = 6;

/// Walking frames per direction.
const FRAMES: usize = 8;

/// First settler shadow in the object archive, one per direction and frame.
const SHADOW: usize = 900;

/// First pack donkey in the object archive, one per direction and frame.
const DONKEY: usize = 2000;

/// A pack donkey's shadow by direction (`0x1f7cac`).
const DONKEY_SHADOWS: [usize; 6] = [2048, 2049, 2050, 2048, 2049, 2050];

/// Figures whose record number is 3 modulo 4 take the fat body when the record decides.
const FAT_RECORD: u16 = 3;

/// The body figure `record` has when its record number decides it (`0x1b5e5`).
const fn record_body(record: u16) -> BodyType {
    if record % 4 == FAT_RECORD {
        BodyType::Fat
    } else {
        BodyType::Slim
    }
}

/// Which file a job table row draws from (byte 0 of the row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BobFile {
    /// `JOBS.BOB`.
    Jobs,
    /// `CARRIER.BOB`.
    Carrier,
}

/// The file, item and body type of figure `record` with job `job` for `nation` (bytes 0 to 2 of
/// the job table at `0x1fad94`, 72 rows). Rows past the 30 jobs are looks, such as a worker
/// carrying what it made. The nations differ only in the five soldier ranks and the scout, rows
/// 22 to 27. A helper takes its body from its record number, as a carrier does: the drawer skips
/// byte 2 for job 0 (`0x1b7b8`). Rows 70 and 71 draw from `CARRIER.BOB`.
pub(super) fn job_look(nation: Nation, job: Job, record: u16) -> Option<(BobFile, usize, BodyType)> {
    const CARRIER_ROWS: [(Job, u8); 2] = [(Job::WATER_ROAD, 15), (Job::CARRYING_WATER, 11)];
    const ITEMS: [u8; 70] = [
        0, 5, 12, 8, 6, 7, 20, 13, 16, 17, 15, 10, 3, 14, 24, 11, 9, 18, 4, 23, 22, 26, 30, 31, 32, 33, 34,
        35, 25, 25, 13, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74,
        75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 91, 89, 90, 92,
    ];
    const FAT: [u8; 13] = [8, 9, 12, 18, 31, 32, 33, 34, 35, 36, 37, 52, 53];
    const NATION_STRIDE: usize = 6;
    if let Some(&(_, item)) = CARRIER_ROWS.iter().find(|(row, _)| *row == job) {
        return Some((BobFile::Carrier, usize::from(item), BodyType::Slim));
    }
    let mut item = usize::from(*ITEMS.get(job.index())?);
    if job.is_soldier() || job == Job::SCOUT {
        item += NATION_STRIDE * nation.index();
    }
    let body = if job == Job::HELPER {
        record_body(record)
    } else if FAT.contains(&job.0) {
        BodyType::Fat
    } else {
        BodyType::Slim
    };
    Some((BobFile::Jobs, item, body))
}

/// The `JOBS.BOB` item and body type the look table's row gives job `job` for `nation`, or
/// `None` for a job with no row or one drawn from `CARRIER.BOB`.
#[must_use]
pub fn job_item(nation: Nation, job: Job) -> Option<(usize, BodyType)> {
    match job_look(nation, job, 0)? {
        (BobFile::Jobs, item, body) => Some((item, body)),
        (BobFile::Carrier, ..) => None,
    }
}

/// A settler as a BOB file draws it, outside any world.
#[derive(Debug, Clone, Copy)]
pub struct Walking<'a> {
    /// The file it is drawn from.
    pub bob: &'a BobSprites,
    /// The item: a ware for `CARRIER.BOB`, a job's item for `JOBS.BOB`.
    pub item: usize,
    /// Its body.
    pub body: BodyType,
    /// The way it faces.
    pub direction: BobDirection,
    /// Its walking frame.
    pub frame: u8,
    /// The first index of the colour ramp its player-colour pixels take.
    pub base: u8,
}

/// Draws `settler` with its anchor at `at`: the body, then the overlay, with no shadow, as the
/// credits draw their crowd (`0x60f54` through `0x77889` and `0x7802e`). An item with no overlay
/// draws its body alone.
pub fn draw_walking(surface: &mut Surface<'_>, settler: Walking<'_>, at: Point) {
    let Walking {
        bob,
        item,
        body,
        direction,
        frame,
        base,
    } = settler;
    let paint = Paint { base, shadow: None };
    if let Some(parts) = bob.parts(item, body, direction, frame) {
        crate::sprite::blit(surface, 0, parts.body, at, paint);
        let lower = at.offset(0, i32::from(parts.dy));
        crate::sprite::blit(surface, 0, parts.overlay, lower, paint);
    } else if let Some(sprite) =
        s2_formats::bob::Bob::body_index(body, direction, frame).and_then(|index| bob.bodies().get(index))
    {
        crate::sprite::blit(surface, 0, sprite, at, paint);
    }
}

impl ObjectPass<'_> {
    /// Settler `record`, standing on or walking to `node`, which is drawn at `at`.
    pub(super) fn settler(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        node: Node,
        at: Point,
        record: u16,
        figure: &Figure,
    ) {
        let Some(settlers) = self.graphics.settlers.as_deref() else {
            return;
        };
        if !figure.walking() && matches!(self.world.object(node), MapObject::Building(_)) {
            return;
        }
        let Some(direction) = u8::try_from(figure.direction.index())
            .ok()
            .and_then(BobDirection::from_index)
        else {
            return;
        };
        let frame = if figure.walking() && !figure.held {
            u8::try_from(self.anim as usize % FRAMES).unwrap_or_default()
        } else {
            STANDING_FRAME
        };
        let pose = Pose { direction, frame };
        match figure.kind {
            FigureKind::Animal { .. } | FigureKind::Builder(_) | FigureKind::Ship { .. } => {}
            FigureKind::Carrier(carrier) => {
                let at = self.walking_offset(node, at, figure, (0, 0));
                if carrier.state == CarrierState::Animating
                    && carrier.transport == Transport::Foot
                    && let Some(animation) = carrier.animation.and_then(|a| self.world.animation(a))
                {
                    self.idle(surface, top, settlers, at, carrier.owner, animation.index);
                    return;
                }
                let load = Load {
                    record,
                    owner: carrier.owner,
                    transport: carrier.transport,
                    shows: carrier.shows,
                };
                self.carrier(surface, top, settlers, at, load, pose);
            }
            FigureKind::Delivering(delivery) => {
                let at = self.through_door(node, at, figure, delivery.inside);
                let load = Load {
                    record,
                    owner: delivery.owner,
                    transport: delivery.transport,
                    shows: delivery.shows,
                };
                self.carrier(surface, top, settlers, at, load, pose);
            }
            FigureKind::Bringing {
                owner, shows, inside, ..
            } => {
                let at = self.through_door(node, at, figure, inside);
                let load = Load {
                    record,
                    owner,
                    transport: Transport::Foot,
                    shows,
                };
                self.carrier(surface, top, settlers, at, load, pose);
            }
            FigureKind::Planer(_) | FigureKind::Gatherer(_) | FigureKind::Explorer(_) => {
                self.out_working(surface, top, settlers, (node, at), record, figure, pose);
            }
            FigureKind::Porter(porter) => {
                let at = self.through_door(node, at, figure, porter.inside);
                let who = Person {
                    record,
                    owner: porter.owner,
                    job: porter.job,
                };
                self.person(surface, top, settlers, at, who, pose);
            }
            FigureKind::Walker(walker) => {
                let at = self.walking_offset(node, at, figure, (0, 0));
                let who = Person {
                    record,
                    owner: walker.owner,
                    job: walker.job,
                };
                self.person(surface, top, settlers, at, who, pose);
            }
            FigureKind::Lost { owner, job, .. } | FigureKind::Homeward { owner, job, .. } => {
                let at = self.walking_offset(node, at, figure, (0, 0));
                self.person(surface, top, settlers, at, Person { record, owner, job }, pose);
            }
            FigureKind::Entering { owner, job } | FigureKind::Settling { owner, job, .. } => {
                let (dx, dy) = self.door_of(node);
                let at = self.walking_offset(node, at.offset(dx, dy), figure, (-dx, -dy));
                self.person(surface, top, settlers, at, Person { record, owner, job }, pose);
            }
            FigureKind::Soldier(_) => self.soldier(surface, top, settlers, (node, at), record, figure, pose),
            FigureKind::Leaving { owner, job, .. } => {
                let map = self.terrain.map();
                let inside = node.neighbour(s2_sim::Direction::NorthWest, map.width(), map.height());
                let at = self.walking_offset(node, at, figure, self.door_of(inside));
                self.person(surface, top, settlers, at, Person { record, owner, job }, pose);
            }
        }
    }

    /// A planer, gatherer or explorer: someone who works out on the land.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the figure, its record and its pose"
    )]
    fn out_working(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        place: (Node, Point),
        record: u16,
        figure: &Figure,
        pose: Pose,
    ) {
        match figure.kind {
            FigureKind::Planer(_) => self.planer(surface, top, settlers, place, record, figure, pose),
            FigureKind::Gatherer(_) => self.gatherer(surface, top, settlers, place, record, figure, pose),
            _ => self.explorer(surface, top, settlers, place, record, figure, pose),
        }
    }

    /// A soldier: fighting or dying in place, through a door, or walking.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the soldier, its record and its pose"
    )]
    fn soldier(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        (node, at): (Node, Point),
        record: u16,
        figure: &Figure,
        pose: Pose,
    ) {
        let FigureKind::Soldier(soldier) = figure.kind else {
            return;
        };
        if let SoldierState::Animating {
            animation: Some(animation),
        } = soldier.state
            && let Some(animation) = self.world.animation(animation)
        {
            self.idle(surface, top, settlers, at, soldier.owner, animation.index);
            return;
        }
        let at = match soldier.state {
            SoldierState::Leaving { .. } => self.through_door(node, at, figure, false),
            SoldierState::Entering { .. } => self.through_door(node, at, figure, true),
            _ => self.walking_offset(node, at, figure, (0, 0)),
        };
        let who = Person {
            record,
            owner: soldier.owner,
            job: soldier.job,
        };
        self.person(surface, top, settlers, at, who, pose);
    }

    /// A gatherer: working in place, through its door, or walking.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the gatherer, its record and its pose"
    )]
    fn gatherer(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        (node, at): (Node, Point),
        record: u16,
        figure: &Figure,
        pose: Pose,
    ) {
        let FigureKind::Gatherer(gatherer) = figure.kind else {
            return;
        };
        if let Trip::Working {
            animation: Some(animation),
        } = gatherer.trip
            && let Some(animation) = self.world.animation(animation)
        {
            self.idle(surface, top, settlers, at, gatherer.owner, animation.index);
            return;
        }
        let at = match gatherer.trip {
            Trip::Leaving { .. } => self.through_door(node, at, figure, false),
            Trip::Entering => self.through_door(node, at, figure, true),
            _ => self.walking_offset(node, at, figure, (0, 0)),
        };
        let who = Person {
            record,
            owner: gatherer.owner,
            job: gatherer.job,
        };
        self.person(surface, top, settlers, at, who, pose);
    }

    /// A geologist or scout: a tapping or pleased geologist's animation in place, otherwise a
    /// settler of its job.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the explorer, its record and its pose"
    )]
    fn explorer(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        (node, at): (Node, Point),
        record: u16,
        figure: &Figure,
        pose: Pose,
    ) {
        let FigureKind::Explorer(explorer) = figure.kind else {
            return;
        };
        if let Exploring::Tapping { animation } | Exploring::Celebrating { animation } = explorer.work
            && let Some(animation) = animation.and_then(|a| self.world.animation(a))
        {
            self.idle(surface, top, settlers, at, explorer.owner, animation.index);
            return;
        }
        let at = self.walking_offset(node, at, figure, (0, 0));
        let who = Person {
            record,
            owner: explorer.owner,
            job: explorer.job,
        };
        self.person(surface, top, settlers, at, who, pose);
    }

    /// A planer (`0x1b4e3`): while it digs, the standing shadow and its animation's current
    /// record, as for kind 40; otherwise a settler of its job.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the planer, its record and its pose"
    )]
    fn planer(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        (node, at): (Node, Point),
        record: u16,
        figure: &Figure,
        pose: Pose,
    ) {
        let FigureKind::Planer(planer) = figure.kind else {
            return;
        };
        if let PlanerWork::Digging { animation } = planer.work
            && let Some(animation) = animation.and_then(|a| self.world.animation(a))
        {
            self.idle(surface, top, settlers, at, planer.owner, animation.index);
            return;
        }
        let at = self.walking_offset(node, at, figure, (0, 0));
        let who = Person {
            record,
            owner: planer.owner,
            job: Job::PLANER,
        };
        self.person(surface, top, settlers, at, who, pose);
    }

    /// Where a settler walking between a door and its flag is drawn: in through the door of the
    /// building on `node` when `inside`, otherwise out from the door of the building north-west
    /// of `node`.
    fn through_door(&self, node: Node, at: Point, figure: &Figure, inside: bool) -> Point {
        if inside {
            let (dx, dy) = self.door_of(node);
            return self.walking_offset(node, at.offset(dx, dy), figure, (-dx, -dy));
        }
        let map = self.terrain.map();
        let building = node.neighbour(s2_sim::Direction::NorthWest, map.width(), map.height());
        self.walking_offset(node, at, figure, self.door_of(building))
    }

    /// The door offset of the building on `node`, or none.
    fn door_of(&self, node: Node) -> (i32, i32) {
        match self.world.object(node) {
            MapObject::Building(building) => self
                .world
                .building(building)
                .and_then(|building| self.graphics.door(building.owner, building.kind))
                .map_or((0, 0), |door| (i32::from(door.x), i32::from(door.y))),
            _ => (0, 0),
        }
    }

    /// A carrier that is not playing an idle animation (`0x1b641`): a pack donkey, or a settler
    /// drawn from `JOBS.BOB` item 0 with nothing to carry and from `CARRIER.BOB` with a ware.
    fn carrier(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        at: Point,
        carrier: Load,
        pose: Pose,
    ) {
        match carrier.transport {
            Transport::Boat => return,
            Transport::Donkey => {
                self.donkey(surface, top, at, pose);
                return;
            }
            Transport::Foot => {}
        }
        let body = record_body(carrier.record);
        let (bob, item) = match carrier.shows {
            None => (&settlers.jobs, 0),
            Some(bitmap) => (&settlers.carrier, usize::from(bitmap)),
        };
        self.on_foot(
            surface,
            top,
            at,
            carrier.owner,
            parts(bob, item, body, pose),
            pose,
        );
    }

    /// A settler drawn by its job: a pack donkey for job 29, otherwise the job's `JOBS.BOB` item.
    fn person(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        at: Point,
        who: Person,
        pose: Pose,
    ) {
        let Person { record, owner, job } = who;
        if job == Job::PACK_DONKEY {
            self.donkey(surface, top, at, pose);
            return;
        }
        let nation = self
            .graphics
            .nation(owner)
            .map(|(nation, _)| nation)
            .unwrap_or_default();
        let parts = job_look(nation, job, record).and_then(|(file, item, body)| {
            let bob = match file {
                BobFile::Jobs => &settlers.jobs,
                BobFile::Carrier => &settlers.carrier,
            };
            parts(bob, item, body, pose)
        });
        self.on_foot(surface, top, at, owner, parts, pose);
    }

    /// A settler on foot (`0x1bea3`): the shadow for its direction and frame, then the body and
    /// the overlay in its owner's colours.
    fn on_foot(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        at: Point,
        owner: Player,
        parts: Option<BobParts<'_>>,
        pose: Pose,
    ) {
        let map = Some(self.graphics.map.as_ref());
        let shadow = SHADOW + FRAMES * pose.direction.index() + usize::from(pose.frame);
        let paint = darken(&self.graphics.shadow);
        self.sprite(surface, top, map, shadow, at, Layers::FIGURES, paint);
        self.bob(surface, top, at, owner, parts);
    }

    /// A settler's body, then its overlay, in `owner`'s colours (`0x77889`, `0x7802e`).
    pub(super) fn bob(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        at: Point,
        owner: Player,
        parts: Option<BobParts<'_>>,
    ) {
        let Some(parts) = parts else {
            return;
        };
        if !self.layers.contains(Layers::FIGURES) {
            return;
        }
        let paint = Paint {
            base: colour_base(owner),
            shadow: None,
        };
        surface.set_pen(Layers::FIGURES);
        crate::sprite::blit(surface, top, parts.body, at, paint);
        crate::sprite::blit(
            surface,
            top,
            parts.overlay,
            at.offset(0, i32::from(parts.dy)),
            paint,
        );
    }

    /// A pack donkey (`0x1bcf1`): its shadow, then its body, neither in player colours.
    fn donkey(&self, surface: &mut Surface<'_>, top: i32, at: Point, pose: Pose) {
        let map = Some(self.graphics.map.as_ref());
        let d = pose.direction.index();
        let paint = darken(&self.graphics.shadow);
        self.sprite(surface, top, map, DONKEY_SHADOWS[d], at, Layers::FIGURES, paint);
        let body = DONKEY + FRAMES * d + usize::from(pose.frame);
        self.sprite(surface, top, map, body, at, Layers::FIGURES, Paint::PLAIN);
    }

    /// A carrier's idle animation (`0x1b66b`, `0x23761`): the standing shadow of direction 0,
    /// then the record the animation last stopped on, moved by its `dx` and `dy`.
    fn idle(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        at: Point,
        owner: Player,
        index: u16,
    ) {
        let map = Some(self.graphics.map.as_ref());
        let paint = darken(&self.graphics.shadow);
        self.sprite(surface, top, map, SHADOW, at, Layers::FIGURES, paint);
        self.frame_record(surface, top, settlers, at, owner, index);
    }

    /// `ROM_ANIM` record `index` (`0x23761`) in `owner`'s colours, moved by its `dx` and `dy`.
    pub(super) fn frame_record(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        settlers: &Settlers,
        at: Point,
        owner: Player,
        index: u16,
    ) {
        let Some(record) = settlers.script.records().get(usize::from(index)) else {
            return;
        };
        let shifted = at.offset(i32::from(record.dx), i32::from(record.dy));
        match record.sprite() {
            AnimSprite::Bitmap(id) => {
                let paint = Paint {
                    base: colour_base(owner),
                    shadow: None,
                };
                let sheet = Some(&settlers.roman);
                self.sprite(
                    surface,
                    top,
                    sheet,
                    usize::from(id),
                    shifted,
                    Layers::FIGURES,
                    paint,
                );
            }
            AnimSprite::Figure { job, direction } => {
                let Some(direction) = BobDirection::from_index(direction) else {
                    return;
                };
                let nation = self
                    .graphics
                    .nation(owner)
                    .map(|(nation, _)| nation)
                    .unwrap_or_default();
                let frame = u8::try_from(usize::from(index) % FRAMES).unwrap_or_default();
                let pose = Pose { direction, frame };
                let parts = animation_look(nation, Job(job)).and_then(|(file, item, body)| {
                    let bob = match file {
                        BobFile::Jobs => &settlers.jobs,
                        BobFile::Carrier => &settlers.carrier,
                    };
                    parts(bob, item, body, pose)
                });
                self.bob(surface, top, shifted, owner, parts);
            }
        }
    }
}

/// The file, item and body type an animation frame of job table row `job` draws for `nation`
/// (`0x23761`). The animation drawer takes byte 2 of the row for the body even for row 0, where the
/// figure drawer uses the record number instead.
fn animation_look(nation: Nation, job: Job) -> Option<(BobFile, usize, BodyType)> {
    let (file, item, body) = job_look(nation, job, 0)?;
    let body = if job == Job::HELPER { BodyType::Slim } else { body };
    Some((file, item, body))
}

/// What a carrier's look depends on: its record, owner, how it gets about and the ware it shows.
#[derive(Clone, Copy)]
struct Load {
    record: u16,
    owner: Player,
    transport: Transport,
    shows: Option<u8>,
}

/// What the look of a settler drawn by its job depends on: its record, owner and job.
#[derive(Clone, Copy)]
struct Person {
    record: u16,
    owner: Player,
    job: Job,
}

/// The way a settler faces and the frame it shows.
#[derive(Debug, Clone, Copy)]
pub(super) struct Pose {
    pub(super) direction: BobDirection,
    pub(super) frame: u8,
}

pub(super) fn parts(bob: &BobSprites, item: usize, body: BodyType, pose: Pose) -> Option<BobParts<'_>> {
    bob.parts(item, body, pose.direction, pose.frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_draw_as_the_table_says() {
        assert_eq!(
            job_look(Nation::Romans, Job::HELPER, 2),
            Some((BobFile::Jobs, 0, BodyType::Slim)),
            "a helper in record 2"
        );
        assert_eq!(
            job_look(Nation::Romans, Job::HELPER, 11),
            Some((BobFile::Jobs, 0, BodyType::Fat)),
            "a helper in a record that is 3 mod 4"
        );
        assert_eq!(
            job_look(Nation::Vikings, Job::MILLER, 0),
            Some((BobFile::Jobs, 16, BodyType::Fat)),
            "a miller"
        );
        assert_eq!(
            job_look(Nation::Romans, Job::PRIVATE, 0),
            Some((BobFile::Jobs, 30, BodyType::Slim))
        );
        assert_eq!(
            job_look(Nation::Africans, Job::SCOUT, 0),
            Some((BobFile::Jobs, 53, BodyType::Slim))
        );
        assert_eq!(
            job_look(Nation::Japanese, Job::PACK_DONKEY, 0),
            Some((BobFile::Jobs, 25, BodyType::Slim))
        );
        assert_eq!(
            job_look(Nation::Romans, Job::CARRYING_WOOD, 0),
            Some((BobFile::Jobs, 61, BodyType::Slim)),
            "a woodcutter with wood"
        );
        assert_eq!(
            job_look(Nation::Romans, Job::WATER_ROAD, 0),
            Some((BobFile::Carrier, 15, BodyType::Slim))
        );
        assert_eq!(
            job_look(Nation::Romans, Job::CARRYING_WATER, 0),
            Some((BobFile::Carrier, 11, BodyType::Slim)),
            "a well's helper with water"
        );
        let fat = (0..30)
            .map(Job)
            .filter(|&job| job_look(Nation::Romans, job, 0).is_some_and(|(_, _, b)| b == BodyType::Fat));
        assert_eq!(
            fat.collect::<Vec<_>>(),
            [Job::MILLER, Job::BAKER, Job::BREWER, Job::ARMOURER]
        );
    }
}
