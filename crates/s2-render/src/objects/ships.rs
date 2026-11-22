//! Ships and part-built ships, drawn from the ship sheet `BOOT_Z.LST`.
//!
//! [`ShipFlags::ORIGINAL`] holds the pennant offsets of `S2.EXE` at `0x1f7c7c` (x) and
//! `0x1f7c88` (y).

use s2_assets::SpriteSheet;
use s2_sim::{Figure, Node, Ship, ShipMode, ShipSite};

use super::{ObjectPass, darken};
use crate::Layers;
use crate::sprite::{Paint, blit_rows, colour_base};
use crate::terrain::{Point, Surface};

/// First picture of a ship at rest. Each direction has a body and, one after it, its shadow.
const AT_REST: usize = 0;

/// First picture of a ship under sail, laid out as [`AT_REST`].
const SAILING: usize = 12;

/// Items per direction and per site stage: the body, then its shadow.
const PAIR: usize = 2;

/// The shadow follows its body.
const SHADOW: usize = 1;

/// First picture of a part-built ship, stage 0; each stage has a body and a shadow.
const SITE: usize = 24;

/// The cargo heaped on deck, one picture per direction.
const CARGO: usize = 30;

/// First frame of the pennant at the masthead, in player colours.
const PENNANT: usize = 40;

/// Pennant frames, cycled by the step counter.
const PENNANT_FRAMES: u32 = 6;

/// An expedition ship's second pennant runs this many frames ahead of the first.
const EXPEDITION_LEAD: u32 = 2;

/// The second pennant hangs this many pixels below the first.
const EXPEDITION_DROP: i32 = 4;

/// The colour ramp of the expedition pennant: palette entries 156 to 159, the four after
/// player 6's ramp.
const EXPEDITION_COLOURS: u8 = 156;

/// A part-built ship shows its stage's picture in quarters, one per part laid.
const PARTS: u32 = 4;

/// Where a ship's pennant hangs, as pixel offsets from the ship, by direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShipFlags {
    offsets: [Point; 6],
}

impl ShipFlags {
    /// The offsets of `S2.EXE`: the masthead of each picture.
    pub const ORIGINAL: Self = Self::new([
        Point::new(-1, -70),
        Point::new(5, -63),
        Point::new(5, -63),
        Point::new(3, -70),
        Point::new(0, -64),
        Point::new(0, -64),
    ]);

    /// A custom table: `offsets[d]` is where a ship facing direction `d` hangs its pennant.
    #[must_use]
    pub const fn new(offsets: [Point; 6]) -> Self {
        Self { offsets }
    }

    /// The offset for direction index `direction`, or none past the table.
    #[must_use]
    pub fn get(&self, direction: usize) -> Point {
        self.offsets.get(direction).copied().unwrap_or_default()
    }
}

impl Default for ShipFlags {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// The rows of a stage picture `height` rows tall that `part` parts show, counted from its
/// bottom edge (`0x1f145`). Part numbers above 3 never occur in play; they are capped at the
/// whole picture.
fn site_rows(height: u16, part: u8) -> u16 {
    let shown = u32::from(height) * (u32::from(part) + 1) / PARTS;
    u16::try_from(shown).unwrap_or(u16::MAX).min(height)
}

impl ObjectPass<'_> {
    fn ships(&self) -> Option<&SpriteSheet> {
        self.graphics.ships.as_deref()
    }

    /// A part-built ship (`0x1f145`): its stage's shadow whole, the previous stage's picture
    /// whole, then the bottom (part + 1) quarters of its stage's picture.
    pub(super) fn ship_site(&self, surface: &mut Surface<'_>, top: i32, site: ShipSite, at: Point) {
        let sheet = self.ships();
        let item = SITE + PAIR * usize::from(site.stage);
        let paint = darken(&self.graphics.shadow);
        self.sprite(surface, top, sheet, item + SHADOW, at, Layers::OBJECTS, paint);
        let Some(sprite) = sheet.and_then(|sheet| sheet.get(item)) else {
            return;
        };
        if site.stage != 0 {
            self.sprite(
                surface,
                top,
                sheet,
                item - PAIR,
                at,
                Layers::OBJECTS,
                Paint::PLAIN,
            );
        }
        if !self.layers.contains(Layers::OBJECTS) {
            return;
        }
        let height = sprite.height();
        let rows = height - site_rows(height, site.part)..height;
        surface.set_pen(Layers::OBJECTS);
        blit_rows(surface, top, sprite, at, Paint::PLAIN, rows);
    }

    /// Ship figure `record` with ship record `ship` (`0x1b26e`), as [`ship_look`] says.
    pub(super) fn ship(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        (node, at): (Node, Point),
        (record, ship): (u16, u16),
        figure: &Figure,
    ) {
        let Some(ship) = self.world.ship(ship) else {
            return;
        };
        let sheet = self.ships();
        let at = self.walking_offset(node, at, figure, (0, 0));
        let d = figure.direction.index();
        let look = ship_look(ship, d, self.step.wrapping_add(u32::from(record)));
        let paint = darken(&self.graphics.shadow);
        self.sprite(
            surface,
            top,
            sheet,
            look.body + SHADOW,
            at,
            Layers::FIGURES,
            paint,
        );
        self.sprite(surface, top, sheet, look.body, at, Layers::FIGURES, Paint::PLAIN);
        if let Some(cargo) = look.cargo {
            self.sprite(surface, top, sheet, cargo, at, Layers::FIGURES, Paint::PLAIN);
        }
        let mast = self.graphics.tables.ship_flags.get(d);
        let at = at.offset(mast.x, mast.y);
        for (item, offset, base) in look.pennants.into_iter().flatten() {
            let paint = Paint { base, shadow: None };
            self.sprite(
                surface,
                top,
                sheet,
                item,
                at.offset(0, offset),
                Layers::FIGURES,
                paint,
            );
        }
    }
}

/// What a ship draws: its body (the shadow is the item after it), the cargo picture, and up to
/// two pennants as (item, pixels below the masthead, colour base).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShipLook {
    body: usize,
    cargo: Option<usize>,
    pennants: [Option<(usize, i32, u8)>; 2],
}

/// The look of `ship` facing direction index `d`, where `clock` is the step counter plus the
/// figure's record number (`0x1b2ad`). A ship at rest has pictures of its own, the cargo shows
/// while any ware or person is aboard, and the pennant cycles through its frames with the clock.
/// An expedition ship flies a second pennant in [`EXPEDITION_COLOURS`] below the first.
fn ship_look(ship: &Ship, d: usize, clock: u32) -> ShipLook {
    let first = if ship.sailing { SAILING } else { AT_REST };
    let laden = ship.wares.iter().chain(&ship.people).any(|&count| count != 0);
    let frame =
        |lead: u32| PENNANT + usize::try_from(clock.wrapping_add(lead) % PENNANT_FRAMES).unwrap_or_default();
    let expedition = matches!(ship.mode, ShipMode::Expedition { .. })
        .then(|| (frame(EXPEDITION_LEAD), EXPEDITION_DROP, EXPEDITION_COLOURS));
    ShipLook {
        body: first + PAIR * d,
        cargo: laden.then_some(CARGO + d),
        pennants: [Some((frame(0), 0, colour_base(ship.owner))), expedition],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_show_quarters_from_the_bottom_rounding_down() {
        assert_eq!(site_rows(30, 0), 7);
        assert_eq!(site_rows(30, 1), 15);
        assert_eq!(site_rows(30, 2), 22);
        assert_eq!(site_rows(30, 3), 30);
        assert_eq!(site_rows(30, 9), 30, "capped at the whole picture");
        assert_eq!(site_rows(3, 0), 0);
    }

    #[test]
    fn a_site_draws_the_previous_stage_whole_and_its_own_from_the_bottom_up() {
        use std::sync::Arc;

        use s2_assets::{Ink, Origin, Remap, Sprite};
        use s2_sim::World;

        use crate::terrain::testing;

        let solid = |colour: u8| {
            let pixels = (0..16).map(move |_| Some((Ink::Colour, colour)));
            Sprite::build(Origin { x: 0, y: 3 }, 4, 4, pixels)
        };
        let mut slots = vec![None; SITE + 2 * PAIR];
        slots[SITE] = Some(solid(7));
        slots[SITE + PAIR] = Some(solid(9));
        let ships = Arc::new(SpriteSheet::from_slots(slots));
        let graphics =
            super::super::ObjectGraphics::new(Arc::default(), Remap::identity(), Remap::identity())
                .with_ships(Some(ships));
        let terrain = testing::terrain(
            testing::flat(4, 4, 1),
            testing::texture(|_, _| 1),
            testing::shading(|_, t| t),
        );
        let world = World::empty(4, 4);
        let pass = ObjectPass {
            terrain: &terrain,
            world: &world,
            graphics: &graphics,
            layers: Layers::ALL,
            anim: 0,
            step: 0,
            aid: None,
            marks: &[],
        };
        let draw = |site: ShipSite| {
            let mut pixels = vec![0; 16];
            let mut surface = Surface::of_parts(&mut pixels, None, 4, 4);
            pass.ship_site(&mut surface, 0, site, Point::new(0, 3));
            pixels
        };
        let first = draw(ShipSite {
            set: 0,
            part: 1,
            stage: 0,
        });
        assert_eq!(first, [[0; 8], [7; 8]].concat(), "two quarters of stage 0");
        let second = draw(ShipSite {
            set: 0,
            part: 0,
            stage: 1,
        });
        assert_eq!(
            second,
            [&[7; 12][..], &[9; 4]].concat(),
            "stage 0 whole under a quarter of stage 1"
        );
    }

    #[test]
    fn ships_show_rest_or_sail_cargo_and_an_expedition_pennant() {
        let owner = s2_sim::Player::new(2).unwrap();
        let mut ship = Ship::new(owner, 0);
        let look = ship_look(&ship, 3, 10);
        assert_eq!(look.body, 6, "at rest, direction 3");
        assert_eq!(look.cargo, None);
        assert_eq!(
            look.pennants,
            [Some((44, 0, colour_base(owner))), None],
            "frame 10 mod 6"
        );
        ship.sailing = true;
        ship.people[4] = 1;
        ship.mode = ShipMode::Expedition { target: None };
        let look = ship_look(&ship, 5, 4);
        assert_eq!((look.body, look.cargo), (22, Some(35)));
        assert_eq!(
            look.pennants,
            [
                Some((44, 0, colour_base(owner))),
                Some((40, 4, EXPEDITION_COLOURS))
            ],
            "the second pennant runs two frames ahead"
        );
    }
}
