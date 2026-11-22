//! Buildings and building sites (`0x19b18`): the sign of a site being levelled, a site under
//! construction with its stakes, its piles of boards and stones, the building rising from its
//! bottom row and its builder, and finished buildings.

use s2_assets::{BobParts, Nation, SpriteSheet};
use s2_formats::bob::Direction as BobDirection;
use s2_sim::{
    Building, BuildingKind, BuildingState, BuildingType, Category, FigureKind, Job, WAITING, Ware, WareKind,
};

use super::extras::{Offset, Yard};
use super::settlers::{Pose, job_look, parts};
use super::{ObjectPass, WARE_ITEMS, darken};
use crate::Layers;
use crate::sprite::{Paint, blit_rows, colour_base, nation_colour_base};
use crate::terrain::{Point, Surface};

/// The first nation item of graphics group 0. Group `g` has five items from `250 + 5·g`: body,
/// shadow, construction body, construction shadow and door.
const GROUP_BASE: usize = 250;
const GROUP_ITEMS: usize = 5;
const SHADOW: usize = 1;
const CONSTRUCTION: usize = 2;
const CONSTRUCTION_SHADOW: usize = 3;
const DOOR: usize = 4;

/// The sign of a site being levelled, then its shadow: nation items, whatever the type.
const SIGN: (usize, usize) = (450, 451);

/// The stakes of a site under construction, then their shadow.
const STAKES: (usize, usize) = (455, 456);

/// Each ware in a pile sits this many pixels above the one before.
const PILE_STEP: i32 = 4;

/// Stones pile this many pixels right of boards.
const PILE_SPACING: i32 = 8;

/// Progress through construction runs to this.
const COMPLETE: u32 = 100;

/// Each ware's share of the progress grows in eighths, one per building tick.
const TIMER_STEPS: u32 = 8;

/// The progress at which the finished picture starts to rise over the construction picture.
const HALFWAY: u32 = 50;

/// The builder's walking frame is the step counter modulo this.
const BUILDER_FRAMES: u32 = 8;

/// Small flags a manned headquarters shows, and the most a military building shows.
const MOST_FLAGS: u8 = 4;

/// First of the eight small-flag frames in the object archive.
const SMALL_FLAG: usize = 3162;

/// How far down each small flag sits below the one before, and how many frames ahead it runs.
const FLAG_STEP: u8 = 3;

/// The small flags start this many pixels above the table's offset.
const FLAGS_RAISED: i32 = 2;

/// First of the eight smoke frames in the object archive.
const SMOKE: usize = 740;

/// First of the work glow's frames in the object archive: four sets of eight, one picked by the
/// building's record number.
const WORK: usize = 700;
const WORK_SETS: u16 = 4;

/// First of the banners in the object archive: four frames for each of the three border classes.
const BANNER: usize = 3150;
const BANNER_FRAMES: u32 = 4;

/// The sign that a building's production is stopped, in the object archive.
const STOPPED: usize = 46;

/// The mill's sails, a nation item pair: still, then one pair every five items for each phase
/// while it grinds.
const MILL_SAILS: usize = 460;

/// The pig farm's sow: twelve frames in the object archive, then the shadow every pig casts.
const SOW: usize = 2100;
const SOW_FRAMES: u32 = 12;
const SOW_SHADOW: usize = 2160;

/// The pig farm's other pigs: 48 frames, each pig twelve frames on from the one before, and
/// their shadow.
const PIGLET: usize = 2112;
const PIGLET_FRAMES: u32 = 48;
const PIGLET_SHADOW: usize = 2161;

/// The pigs' clock is half the step counter, read as a 16-bit word (`0x1a872`).
const PIG_CLOCK: u32 = 1 << 16;

/// Productivity per pig a pig farm shows beyond the sow, and the most it shows in all
/// (`0x1a81c`, `0x1a923`).
const PRODUCTIVITY_PER_PIG: u8 = 15;
const MOST_PIGS: usize = 5;

/// The donkey breeder's donkeys: frames from the object archive, picked through
/// [`DONKEY_STEPS`], and the productivity per donkey shown (`0x1aba2`).
const DONKEY_FOAL: usize = 2180;
const PRODUCTIVITY_PER_DONKEY: u8 = 26;

/// How many frames ahead each of the breeder's donkeys runs.
const DONKEY_LEAD: u32 = 15;

/// The frame each half step of the breeder's donkeys shows (`0x1f8fd0`).
const DONKEY_STEPS: [u8; 40] = [
    2, 1, 2, 3, 4, 5, 6, 7, 7, 7, 7, 7, 6, 6, 6, 7, 7, 7, 6, 5, 5, 4, 3, 2, 1, 1, 2, 3, 4, 5, 6, 7, 7, 7, 6,
    7, 6, 5, 4, 3,
];

/// The harbour's own animation, a nation item every five from here, one per phase.
const HARBOUR_WORKS: usize = 500;

/// Where in a nation's harbour offsets the waiting builder stands, after the boards and stones.
const HARBOUR_BUILDER: usize = 2;

/// The direction the builder waiting at a harbour for its expedition faces.
const WAITING_BUILDER: u8 = 2;

/// The catapult at rest, an item of `ROM_BOBS.LST`, and where it sits from the building.
const CATAPULT_AT_REST: usize = 1776;
const CATAPULT_REST: (i32, i32) = (-7, -18);

/// The phase of an animation running from `offset` (`0x936b4`).
fn phase(anim: u32, offset: u16) -> usize {
    (usize::from(offset) % super::ANIMATION_PHASES + anim as usize) % super::ANIMATION_PHASES
}

/// The pigs a pig farm of `productivity` shows at step `step`: each one's body, its shadow and
/// where it stands (`0x1a7fe`). The sow always shows; one more pig for every 15 points of
/// productivity follows, up to five in all.
fn pigs(productivity: u8, step: u32, yard: &Yard) -> impl Iterator<Item = (usize, usize, Offset)> {
    let half = (step / 2) % PIG_CLOCK;
    let shown = (usize::from(productivity / PRODUCTIVITY_PER_PIG) + 1).min(MOST_PIGS);
    yard.pigs
        .into_iter()
        .enumerate()
        .take(shown)
        .map(move |(pig, offset)| {
            if pig == 0 {
                (SOW + (half % SOW_FRAMES) as usize, SOW_SHADOW, offset)
            } else {
                let lead = SOW_FRAMES * u32::try_from(pig).unwrap_or_default();
                (
                    PIGLET + ((half + lead) % PIGLET_FRAMES) as usize,
                    PIGLET_SHADOW,
                    offset,
                )
            }
        })
}

/// The donkeys a donkey breeder of `productivity` shows at step `step`: each one's frame and
/// where it stands (`0x1ab9a`). One shows for every 26 points of productivity.
fn donkeys(productivity: u8, step: u32, yard: &Yard) -> impl Iterator<Item = (usize, Offset)> {
    let shown = usize::from(productivity / PRODUCTIVITY_PER_DONKEY);
    yard.donkeys
        .into_iter()
        .enumerate()
        .take(shown)
        .map(move |(donkey, offset)| {
            let lead = DONKEY_LEAD * u32::try_from(donkey).unwrap_or_default();
            let half = (lead.wrapping_add(step) / 2) as usize % DONKEY_STEPS.len();
            (DONKEY_FOAL + usize::from(DONKEY_STEPS[half]), offset)
        })
}

/// How far construction of `site` has gone, 0 to 100 (`0x19d7a`): a hundred for each board or
/// stone used but the last, plus the last one's share by eighths of the timer, or all of it while
/// the site waits, out of a hundred for each the type needs.
fn progress(site: &Building, row: BuildingType) -> u32 {
    let used = u32::from(site.used[0]) + u32::from(site.used[1]);
    let needed = u32::from(row.boards) + u32::from(row.stones);
    if needed == 0 {
        return COMPLETE;
    }
    let mut done = COMPLETE * used.saturating_sub(1);
    if site.timer != WAITING {
        done += COMPLETE * u32::from(site.timer) / TIMER_STEPS;
    } else if used > 0 {
        done += COMPLETE;
    }
    (done / needed).min(COMPLETE)
}

impl ObjectPass<'_> {
    /// Building `record` at `at` (`0x19b18`). A site being levelled shows its sign, a site under
    /// construction its stakes, piles, the part built and its builder, and a finished building
    /// its body, door and, for a manned headquarters, its small flags. A running animation of
    /// any but a site under construction is drawn at the building (`0x1aca3`).
    pub(super) fn building(&self, surface: &mut Surface<'_>, top: i32, record: u16, at: Point) {
        let Some(building) = self.world.building(record) else {
            return;
        };
        let Some((nation, sheet)) = self.graphics.nation(building.owner) else {
            return;
        };
        let row = self.world.building_types().get(building.kind);
        if building.state.contains(BuildingState::LEVELLING) {
            self.pair(surface, top, sheet, SIGN, at);
        } else if building.state.contains(BuildingState::BUILDING) {
            if let Some(row) = row {
                self.site(surface, top, building, row, sheet, at);
            }
            return;
        } else if building.state.contains(BuildingState::FINISHED)
            && let Some(row) = row
        {
            self.finished(surface, top, record, building, nation, sheet, row, at);
        }
        if let Some(animation) = building.animation {
            self.animation(surface, top, building, animation, at);
        }
    }

    /// A shadow then its body, both nation items at `at` (`0x19a94`).
    fn pair(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        sheet: &SpriteSheet,
        (body, shadow): (usize, usize),
        at: Point,
    ) {
        let paint = darken(&self.graphics.shadow);
        self.sprite(surface, top, Some(sheet), shadow, at, Layers::BUILDINGS, paint);
        self.sprite(
            surface,
            top,
            Some(sheet),
            body,
            at,
            Layers::BUILDINGS,
            Paint::PLAIN,
        );
    }

    /// The bottom `q` hundredths of nation item `item`'s rows, where a full draw puts them; none
    /// when that rounds down to no rows.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the item, its share and how it is painted"
    )]
    fn rising(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        sheet: &SpriteSheet,
        item: usize,
        at: Point,
        paint: Paint<'_>,
        q: u32,
    ) {
        let pen = if paint.shadow.is_some() {
            Layers::BUILDINGS | Layers::SHADOWS
        } else {
            Layers::BUILDINGS
        };
        let Some(sprite) = sheet.get(item) else {
            return;
        };
        let height = u32::from(sprite.height());
        let shown = u16::try_from(height * q / COMPLETE).unwrap_or(u16::MAX);
        if shown == 0 || !self.layers.contains(pen) {
            return;
        }
        surface.set_pen(pen);
        blit_rows(
            surface,
            top,
            sprite,
            at,
            paint,
            sprite.height() - shown..sprite.height(),
        );
    }

    /// A site under construction (`0x19c1d`). Without its builder: the stakes, then the piles.
    /// With it: the piles, the stakes, the building risen to its progress and the builder. Below
    /// halfway the construction picture rises; from there it stands whole and the finished
    /// picture rises over it.
    fn site(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        site: &Building,
        row: BuildingType,
        sheet: &SpriteSheet,
        at: Point,
    ) {
        let present = site.state.contains(BuildingState::PRESENT);
        if !present {
            self.pair(surface, top, sheet, STAKES, at);
        }
        self.piles(surface, top, site, at);
        if !present {
            return;
        }
        self.pair(surface, top, sheet, STAKES, at);
        let base = GROUP_BASE + GROUP_ITEMS * usize::from(row.group);
        let shadow = darken(&self.graphics.shadow);
        let progress = progress(site, row);
        if progress < HALFWAY {
            let q = 2 * progress;
            self.rising(surface, top, sheet, base + CONSTRUCTION_SHADOW, at, shadow, q);
            self.rising(surface, top, sheet, base + CONSTRUCTION, at, Paint::PLAIN, q);
        } else {
            let q = 2 * (progress - HALFWAY);
            self.pair(
                surface,
                top,
                sheet,
                (base + CONSTRUCTION, base + CONSTRUCTION_SHADOW),
                at,
            );
            self.rising(surface, top, sheet, base + SHADOW, at, shadow, q);
            self.rising(surface, top, sheet, base, at, Paint::PLAIN, q);
        }
        self.builder(surface, top, site, at);
    }

    /// The boards, then the stones, a site holds (`0x19c67`): a column of one bitmap per ware,
    /// 4 pixels apart from the bottom up, at the door, the stones 8 pixels right of the boards.
    fn piles(&self, surface: &mut Surface<'_>, top: i32, site: &Building, at: Point) {
        let door = self.graphics.door(site.owner, site.kind).unwrap_or_default();
        let map = Some(self.graphics.map.as_ref());
        for (index, (&kind, &count)) in WareKind::CONSTRUCTION.iter().zip(&site.stock).enumerate() {
            let Some(bitmap) = Ware::new(kind).bitmap() else {
                continue;
            };
            let column = PILE_SPACING * i32::try_from(index).unwrap_or_default();
            for n in 0..i32::from(count) {
                let place = at.offset(i32::from(door.x) + column, i32::from(door.y) - PILE_STEP * n);
                let item = WARE_ITEMS + usize::from(bitmap);
                self.sprite(surface, top, map, item, place, Layers::BUILDINGS, Paint::PLAIN);
            }
        }
    }

    /// The builder of `site`, at the site plus its pixel offset, with no shadow (`0x1a170`,
    /// `0x1acb7`): while it hammers, its animation's current record; otherwise walking, its
    /// `JOBS.BOB` figure with the step counter's frame.
    fn builder(&self, surface: &mut Surface<'_>, top: i32, site: &Building, at: Point) {
        let Some(FigureKind::Builder(builder)) = site
            .builder
            .and_then(|record| self.world.figure(record))
            .map(|figure| figure.kind)
        else {
            return;
        };
        let Some(settlers) = self.graphics.settlers.as_deref() else {
            return;
        };
        let place = at.offset(i32::from(builder.x), i32::from(builder.y));
        if let Some(animation) = site.animation.and_then(|a| self.world.animation(a)) {
            self.frame_record(surface, top, settlers, place, site.owner, animation.index);
            return;
        }
        let Some(direction) = u8::try_from(builder.facing.index())
            .ok()
            .and_then(BobDirection::from_index)
        else {
            return;
        };
        let Some((nation, _)) = self.graphics.nation(site.owner) else {
            return;
        };
        let frame = u8::try_from(self.step % BUILDER_FRAMES).unwrap_or_default();
        let pose = Pose { direction, frame };
        let parts: Option<BobParts<'_>> = job_look(nation, Job::BUILDER, 0)
            .and_then(|(_, item, body)| parts(&settlers.jobs, item, body, pose));
        self.bob(surface, top, place, site.owner, parts);
    }

    /// A finished building (`0x1a39f`, `0x19a94`): shadow, body, the door while it is open, and
    /// a manned headquarters' four small flags.
    ///
    /// Small flag `n` sits 3 pixels below flag `n − 1` and shows frame `(anim + record + 3·n) & 7`.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the building, its record, look and type"
    )]
    fn finished(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        record: u16,
        building: &Building,
        nation: Nation,
        sheet: &SpriteSheet,
        row: BuildingType,
        at: Point,
    ) {
        let base = GROUP_BASE + GROUP_ITEMS * usize::from(row.group);
        self.pair(surface, top, sheet, (base, base + SHADOW), at);
        if building.state.contains(BuildingState::DOOR) {
            self.sprite(
                surface,
                top,
                Some(sheet),
                base + DOOR,
                at,
                Layers::BUILDINGS,
                Paint::PLAIN,
            );
        }
        self.extras(surface, top, record, building, nation, sheet, row, at);
    }

    /// What a finished building draws over itself (`0x1a448` to `0x1acb0`): smoke, the glow
    /// while it works, the small flags of a headquarters or a military building and the banner
    /// of the latter, the mill's sails and the sign that production is stopped.
    #[allow(
        clippy::too_many_arguments,
        reason = "the drawing context plus the building and what it is drawn from"
    )]
    fn extras(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        record: u16,
        building: &Building,
        nation: Nation,
        sheet: &SpriteSheet,
        row: BuildingType,
        at: Point,
    ) {
        let extra = self.graphics.extras(nation, building.kind);
        let map = Some(self.graphics.map.as_ref());
        let place = |offset: Offset| at.offset(i32::from(offset.x), i32::from(offset.y));
        let running = phase(self.anim, record);
        if let Some(smoke) = extra.smoke {
            let item = SMOKE + running;
            self.sprite(
                surface,
                top,
                map,
                item,
                place(smoke),
                Layers::BUILDINGS,
                Paint::PLAIN,
            );
        }
        if let Some(work) = extra.work
            && building
                .state
                .contains(BuildingState::MANNED | BuildingState::WORKING)
        {
            let item = WORK + running + super::ANIMATION_PHASES * usize::from(record % WORK_SETS);
            let paint = Paint {
                base: 0,
                shadow: Some(&self.graphics.work),
            };
            self.sprite(surface, top, map, item, place(work), Layers::BUILDINGS, paint);
        }
        let military = (row.category == Category::Military)
            .then(|| self.world.military_of(record))
            .flatten()
            .map(|(_, military)| military);
        let flags = match military {
            Some(military) => military.inside.min(MOST_FLAGS),
            None if building.kind == BuildingKind::HEADQUARTERS
                && building.state.contains(BuildingState::MANNED) =>
            {
                MOST_FLAGS
            }
            None => 0,
        };
        let paint = Paint {
            base: colour_base(building.owner),
            shadow: None,
        };
        for n in 0..flags {
            let item = SMALL_FLAG + phase(self.anim, record + u16::from(FLAG_STEP * n));
            let flag_at = place(extra.flags).offset(0, i32::from(FLAG_STEP * n) - FLAGS_RAISED);
            self.sprite(surface, top, map, item, flag_at, Layers::BUILDINGS, paint);
        }
        if let (Some(military), Some(banner)) = (military, extra.banner) {
            let frame = (self.step % BANNER_FRAMES) as usize;
            let item = BANNER + BANNER_FRAMES as usize * usize::from(military.border) + frame;
            self.sprite(surface, top, map, item, place(banner), Layers::BUILDINGS, paint);
        }
        if building.kind == BuildingKind::MILL {
            let body = if building.state.contains(BuildingState::WORKING) {
                MILL_SAILS + GROUP_ITEMS * running
            } else {
                MILL_SAILS
            };
            self.pair(surface, top, sheet, (body, body + SHADOW), at);
        }
        self.beside(surface, top, record, building, nation, at);
        if building.state.contains(BuildingState::STOPPED) {
            self.sprite(
                surface,
                top,
                map,
                STOPPED,
                place(extra.stopped),
                Layers::BUILDINGS,
                Paint::PLAIN,
            );
        }
    }

    /// What the pig farm, the donkey breeder, the harbour and the catapult draw of their own
    /// (`0x1a6e0`).
    fn beside(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        record: u16,
        building: &Building,
        nation: Nation,
        at: Point,
    ) {
        let yard = self.graphics.yard(nation);
        let map = Some(self.graphics.map.as_ref());
        let place = |offset: Offset| at.offset(i32::from(offset.x), i32::from(offset.y));
        match building.kind {
            BuildingKind::PIG_FARM => {
                let shadow = Paint {
                    base: 0,
                    shadow: Some(&self.graphics.shadow),
                };
                for (body, shade, offset) in pigs(building.productivity, self.step, &yard) {
                    self.sprite(surface, top, map, shade, place(offset), Layers::BUILDINGS, shadow);
                    self.sprite(
                        surface,
                        top,
                        map,
                        body,
                        place(offset),
                        Layers::BUILDINGS,
                        Paint::PLAIN,
                    );
                }
            }
            BuildingKind::DONKEY_BREEDER => {
                for (item, offset) in donkeys(building.productivity, self.step, &yard) {
                    self.sprite(
                        surface,
                        top,
                        map,
                        item,
                        place(offset),
                        Layers::BUILDINGS,
                        Paint::PLAIN,
                    );
                }
            }
            BuildingKind::HARBOUR => self.harbour(surface, top, record, building, nation, at),
            BuildingKind::CATAPULT if building.animation.is_none() => {
                let Some(settlers) = self.graphics.settlers.as_deref() else {
                    return;
                };
                let paint = Paint {
                    base: nation_colour_base(nation),
                    shadow: None,
                };
                let (dx, dy) = CATAPULT_REST;
                self.sprite(
                    surface,
                    top,
                    Some(&settlers.roman),
                    CATAPULT_AT_REST,
                    at.offset(dx, dy),
                    Layers::BUILDINGS,
                    paint,
                );
            }
            _ => {}
        }
    }

    /// A harbour's own animation, and while it prepares an expedition the boards and stones it
    /// has gathered and the builder once one has come (`0x1a96d`).
    fn harbour(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        record: u16,
        building: &Building,
        nation: Nation,
        at: Point,
    ) {
        if let Some((_, sheet)) = self.graphics.nation(building.owner) {
            let item = HARBOUR_WORKS + GROUP_ITEMS * phase(self.anim, record);
            self.sprite(
                surface,
                top,
                Some(sheet),
                item,
                at,
                Layers::BUILDINGS,
                Paint::PLAIN,
            );
        }
        let Some(harbour) = self
            .world
            .harbour_of(record)
            .and_then(|harbour| self.world.harbour(harbour))
            .filter(|harbour| harbour.expedition)
        else {
            return;
        };
        let yard = self.graphics.yard(nation);
        let map = Some(self.graphics.map.as_ref());
        let place = |offset: Offset| at.offset(i32::from(offset.x), i32::from(offset.y));
        let gathered = [harbour.boards, harbour.stones];
        for ((&kind, &count), &offset) in WareKind::CONSTRUCTION.iter().zip(&gathered).zip(&yard.harbour) {
            let Some(bitmap) = Ware::new(kind).bitmap() else {
                continue;
            };
            let item = WARE_ITEMS + usize::from(bitmap);
            for n in 0..i32::from(count) {
                let pile = place(offset).offset(0, -PILE_STEP * n);
                self.sprite(surface, top, map, item, pile, Layers::BUILDINGS, Paint::PLAIN);
            }
        }
        if harbour.builders == 0 {
            return;
        }
        let Some(settlers) = self.graphics.settlers.as_deref() else {
            return;
        };
        let Some(direction) = BobDirection::from_index(WAITING_BUILDER) else {
            return;
        };
        let pose = Pose { direction, frame: 0 };
        let parts = job_look(nation, Job::BUILDER, 0)
            .and_then(|(_, item, body)| parts(&settlers.jobs, item, body, pose));
        self.bob(
            surface,
            top,
            place(yard.harbour[HARBOUR_BUILDER]),
            building.owner,
            parts,
        );
    }

    /// A building's running animation at the building (`0x1aca3`).
    fn animation(&self, surface: &mut Surface<'_>, top: i32, building: &Building, animation: u16, at: Point) {
        let (Some(settlers), Some(animation)) =
            (self.graphics.settlers.as_deref(), self.world.animation(animation))
        else {
            return;
        };
        self.frame_record(surface, top, settlers, at, building.owner, animation.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use s2_sim::{Building, BuildingTypes, Node, Player};

    #[test]
    fn a_pig_farm_shows_the_sow_and_a_pig_for_every_fifteen_points() {
        let yard = Yard::ORIGINAL[0];
        assert_eq!(pigs(0, 0, &yard).count(), 1, "the sow alone");
        assert_eq!(pigs(29, 0, &yard).count(), 2);
        assert_eq!(pigs(100, 0, &yard).count(), MOST_PIGS);
        let shown: Vec<_> = pigs(100, 30, &yard).collect();
        assert_eq!(
            shown[0],
            (SOW + 3, SOW_SHADOW, yard.pigs[0]),
            "half of step 30 is 15, 15 mod 12"
        );
        assert_eq!(shown[1], (PIGLET + 27, PIGLET_SHADOW, yard.pigs[1]), "15 + 12");
        assert_eq!(shown[4].0, PIGLET + 15, "15 + 48 wraps to 15");
    }

    #[test]
    fn the_pigs_clock_is_a_sixteen_bit_word() {
        let yard = Yard::ORIGINAL[0];
        let wrapped = pigs(0, 2 * 65_536 + 2, &yard).next();
        assert_eq!(wrapped.map(|pig| pig.0), Some(SOW + 1));
    }

    #[test]
    fn a_donkey_breeder_shows_a_donkey_for_every_twenty_six_points() {
        let yard = Yard::ORIGINAL[0];
        assert_eq!(donkeys(25, 0, &yard).count(), 0);
        assert_eq!(donkeys(26, 0, &yard).count(), 1);
        let shown: Vec<_> = donkeys(100, 0, &yard).collect();
        assert_eq!(
            shown,
            [
                (DONKEY_FOAL + 2, yard.donkeys[0]),
                (DONKEY_FOAL + 7, yard.donkeys[1]),
                (DONKEY_FOAL + 7, yard.donkeys[2]),
            ],
            "steps 0, 7 and 15 of the sequence"
        );
    }

    fn site(used: [u8; 2], timer: u8) -> Building {
        let mut site = Building::new(
            BuildingKind::SAWMILL,
            Player::new(0).unwrap(),
            Node { x: 0, y: 0 },
            0,
            BuildingState::BUILDING,
        );
        site.used = used;
        site.timer = timer;
        site
    }

    #[test]
    fn progress_grows_by_eighths_of_a_ware_and_never_jumps() {
        let sawmill = BuildingTypes::ORIGINAL.get(BuildingKind::SAWMILL).unwrap();
        assert_eq!((sawmill.boards, sawmill.stones), (2, 2));
        assert_eq!(progress(&site([0, 0], 0), sawmill), 0);
        assert_eq!(progress(&site([1, 0], 1), sawmill), 3);
        assert_eq!(progress(&site([1, 0], 8), sawmill), 25);
        assert_eq!(
            progress(&site([1, 0], WAITING), sawmill),
            25,
            "waiting counts the ware in full"
        );
        assert_eq!(
            progress(&site([1, 0], 9), sawmill),
            progress(&site([2, 0], 1), sawmill),
            "the ninth tick shows what the next ware's first does"
        );
        assert_eq!(progress(&site([2, 2], 9), sawmill), 100, "capped");
    }
}
