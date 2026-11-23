//! The object pass (`S2.EXE` `0x1fa5c`, walker `0x1edcc`): everything on the map view
//! after terrain, redrawn every frame. Per visible node, in walk order: the node's object
//! (shadow first, then bitmap), then its figures from the head of the list. Fog decides only
//! whether a node is drawn, never how dark.

mod buildings;
mod doors;
mod extras;
mod flags;
mod settlers;
mod ships;

pub use doors::{Door, Doors, DoorsError, MAX_DOOR_TYPES};
pub use extras::{Extra, Extras, ExtrasError, Offset, Yard};
pub use flags::FlagSlots;
pub use settlers::{Walking, draw_walking, job_item};
pub use ships::ShipFlags;

/// The tables the object pass draws with, which a mod can replace: by nation, the doors, what
/// finished buildings draw over themselves and where yards put their animals, and for every
/// nation where flags hold their wares and ships hang their pennants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTables {
    /// Each nation's doors, by nation number.
    pub doors: [Doors; Nation::ALL.len()],
    /// What each nation's finished buildings draw over themselves, by nation number.
    pub extras: [Extras; Nation::ALL.len()],
    /// Where each nation's pig farms, donkey breeders and harbours draw beside them.
    pub yards: [Yard; Nation::ALL.len()],
    /// Where a flag draws its waiting wares.
    pub flag_slots: FlagSlots,
    /// Where a ship hangs its pennant.
    pub ship_flags: ShipFlags,
}

impl ObjectTables {
    /// The tables of `S2.EXE`.
    pub const ORIGINAL: Self = Self {
        doors: Doors::ORIGINAL,
        extras: Extras::ORIGINAL,
        yards: Yard::ORIGINAL,
        flag_slots: FlagSlots::ORIGINAL,
        ship_flags: ShipFlags::ORIGINAL,
    };
}

impl Default for ObjectTables {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

use std::sync::Arc;

use s2_assets::{Nation, Remap, Settlers, SpriteSheet};

use crate::Layers;
use crate::sprite::{Paint, blit};
use crate::terrain::{
    HEIGHT_STEP, NODE_WIDTH, ODD_ROW_SHIFT, ORIGIN_X, Point, ROW_HEIGHT, ScreenNode, ScreenOffset, Surface,
    Terrain, Walk, lift,
};
use s2_sim::{
    AnimalState, BuildingKind, Figure, FigureKind, MapObject, Node, NodeFlags, NodeMap, Player, SiteSize,
    Species, Tree, World,
};

/// A ware's bitmap counts from this MAP item.
const WARE_ITEMS: usize = 2200;

/// The first of the construction aid's symbols in the landscape set, the one a node with room
/// for a flag alone takes.
const AID_ITEMS: usize = 50;

/// The symbol a large site the map marks as a harbour takes instead.
const AID_HARBOUR: usize = 55;

/// Where a [`Mark`] goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    /// On a node.
    Node(Node),
    /// Halfway between two nodes' screen positions, each coordinate halved towards zero.
    Between(Node, Node),
}

/// An item of the landscape set drawn over every object, as the road icons round the head and
/// the marker under the mouse are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mark {
    /// Where it goes.
    pub place: Place,
    /// The item.
    pub item: usize,
}

/// The clocks the object pass draws by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Clocks {
    /// The animation counter (`0x200c3c`), which runs at the screen's pace: trees, water,
    /// flags and walking settlers.
    pub anim: u32,
    /// The simulation step counter (`0x200c9c`): a walking builder's frame and a ship's pennant.
    pub step: u32,
}

/// Where a species' bitmaps sit, as offsets from item 1600 of the landscape's object archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SpeciesSprites {
    /// First walking frame of direction 0.
    walk: u16,
    /// Walking frames per direction.
    frames: u16,
    /// Resting frame of direction 0.
    rest: u16,
    /// Distance between resting frames of neighbouring directions.
    rest_step: u16,
    /// The dead animal.
    dead: u16,
    /// The shadow of direction 0.
    shadow: u16,
    /// Added to [`Self::shadow`] per direction.
    shadow_offset: [u16; 6],
}

/// Item of the object archive that species offsets count from.
const SPECIES_BASE: usize = 1600;

const SAME: [u16; 6] = [0; 6];
const TURNING: [u16; 6] = [0, 1, 2, 3, 4, 5];

/// A species' bitmaps (`0x1fbb24`, 56 bytes per species).
const fn species_sprites(species: Species) -> SpeciesSprites {
    let (walk, frames, dead, shadow, shadow_offset) = match species {
        Species::PolarBear => (0, 8, 48, 49, SAME),
        Species::LightRabbit => (100, 6, 136, 137, SAME),
        Species::GreyRabbit => (140, 6, 176, 177, SAME),
        Species::Fox => (200, 6, 236, 240, TURNING),
        Species::Stag => (250, 8, 298, 300, TURNING),
        Species::Deer => (310, 8, 358, 360, TURNING),
        Species::Duck => (370, 1, 370, 376, SAME),
        Species::Donkey => (400, 8, 400, 448, [0, 1, 2, 0, 1, 2]),
        Species::Sheep => (460, 2, 472, 473, SAME),
    };
    SpeciesSprites {
        walk,
        frames,
        rest: walk,
        rest_step: frames,
        dead,
        shadow,
        shadow_offset,
    }
}

/// The sprites and remap tables the object pass draws with.
#[derive(Debug, Clone)]
pub struct ObjectGraphics {
    map: Arc<SpriteSheet>,
    mission: Option<Arc<SpriteSheet>>,
    nations: [Option<(Nation, Arc<SpriteSheet>)>; Player::COUNT],
    settlers: Option<Arc<Settlers>>,
    ships: Option<Arc<SpriteSheet>>,
    tables: ObjectTables,
    shadow: Remap,
    grey_shadow: Remap,
    work: Remap,
}

impl ObjectGraphics {
    /// Graphics from the landscape's object sheet `map` (`MAP_<set>_Z.LST`) and its normal and
    /// grey shadow tables, with no mission set and no nation for any player.
    #[must_use]
    pub fn new(map: Arc<SpriteSheet>, shadow: Remap, grey_shadow: Remap) -> Self {
        Self {
            map,
            mission: None,
            nations: Default::default(),
            settlers: None,
            ships: None,
            tables: ObjectTables::ORIGINAL,
            shadow,
            grey_shadow,
            work: Remap::identity(),
        }
    }

    /// The table a working building's glow goes through ([`s2_assets::Tint::WORK`],
    /// `0x200c66`); without it the glow changes nothing.
    #[must_use]
    pub fn with_work_table(mut self, work: Remap) -> Self {
        self.work = work;
        self
    }

    /// The mission set's sheet (`MIS<n>BOBS.LST`), the source of every decoration not in the MAP
    /// archive.
    #[must_use]
    pub fn with_mission(mut self, mission: Option<Arc<SpriteSheet>>) -> Self {
        self.mission = mission;
        self
    }

    /// What settlers are drawn from; without it no settler is drawn.
    #[must_use]
    pub fn with_settlers(mut self, settlers: Option<Arc<Settlers>>) -> Self {
        self.settlers = settlers;
        self
    }

    /// The ship sheet (`BOOT_Z.LST`), which ships and part-built ships draw from; without it
    /// neither is drawn.
    #[must_use]
    pub fn with_ships(mut self, ships: Option<Arc<SpriteSheet>>) -> Self {
        self.ships = ships;
        self
    }

    /// Gives `player` a nation and that nation's sheet for its buildings, flags and stones.
    pub fn set_nation(&mut self, player: Player, nation: Nation, sheet: Arc<SpriteSheet>) {
        self.nations[player.index()] = Some((nation, sheet));
    }

    /// Draws with `tables` from now on.
    #[must_use]
    pub fn with_tables(mut self, tables: ObjectTables) -> Self {
        self.tables = tables;
        self
    }

    /// Where `nation`'s pig farms, donkey breeders and harbours draw what they draw beside them.
    pub(crate) const fn yard(&self, nation: Nation) -> Yard {
        self.tables.yards[nation.index()]
    }

    /// What `nation`'s buildings of type `kind` draw over themselves.
    pub(crate) fn extras(&self, nation: Nation, kind: BuildingKind) -> Extra {
        self.tables.extras[nation.index()].get(kind)
    }

    /// The door of a building of type `kind` owned by `player`, or `None` while the player has
    /// no nation.
    #[must_use]
    pub fn door(&self, player: Player, kind: BuildingKind) -> Option<Door> {
        let (nation, _) = self.nation(player)?;
        Some(self.tables.doors[nation.index()].get(kind))
    }

    /// `player`'s nation and sheet.
    #[must_use]
    pub fn nation(&self, player: Player) -> Option<(Nation, &SpriteSheet)> {
        self.nations[player.index()]
            .as_ref()
            .map(|(nation, sheet)| (*nation, sheet.as_ref()))
    }
}

/// Which archive an item comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sheet {
    Map,
    Mission,
}

/// Which shadow table a shadow uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shade {
    Normal,
    Grey,
}

/// A static object's bitmap and shadow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Look {
    sheet: Sheet,
    body: usize,
    shadow: Option<(usize, Shade)>,
}

impl Look {
    const fn map(body: usize, shadow: usize) -> Self {
        Self {
            sheet: Sheet::Map,
            body,
            shadow: Some((shadow, Shade::Normal)),
        }
    }
}

/// A decoration's archive and its bitmap and shadow numbers (`0x1fb8f4`, 9 bytes per entry,
/// entries 0 to 50): MAP numbers count from item 500.
const fn decoration(detail: u8) -> Option<(Sheet, u16, u16)> {
    let d = detail as u16;
    Some(match detail {
        0..=15 => (Sheet::Map, d, 100 + d),
        16..=20 => (Sheet::Map, 42 + d - 16, 142 + d - 16),
        22 | 23 => (Sheet::Map, 60 + d - 22, 160 + d - 22),
        34..=43 => (Sheet::Map, 50 + d - 34, 150 + d - 34),
        21 | 49 | 50 => (Sheet::Mission, 0, 1),
        24..=30 => (Sheet::Mission, 2 * (d - 24), 2 * (d - 24) + 1),
        31 => (Sheet::Mission, 20, 21),
        32 => (Sheet::Mission, 22, 23),
        33 => (Sheet::Mission, 30, 31),
        44..=48 => (Sheet::Mission, 2 * (d - 44), 2 * (d - 44) + 1),
        _ => return None,
    })
}

/// What a static object (class 3) draws, by slot 5 `group` and slot 4 `detail`. Part-built ships
/// (slot 5 from 228 to 231) draw in [`ObjectPass::ship_site`].
fn static_look(object: MapObject, node: Node, anim: u32) -> Option<Look> {
    let anim = anim as usize;
    let phase = |offset: u8| (anim + usize::from(offset)) % ANIMATION_PHASES;
    Some(match object {
        MapObject::Tree(tree) => {
            let first = TREE_ITEMS * usize::from(tree.kind);
            let stage = usize::from(tree.stage);
            let frame = if tree.felling {
                FELLING_FRAMES + stage
            } else if tree.stage == Tree::GROWN {
                phase(tree.phase)
            } else {
                GROWING_FRAMES + stage
            };
            Look::map(200 + first + frame, 350 + first + frame)
        }
        MapObject::Decoration(decoration) => {
            let (sheet, body, shadow) = self::decoration(decoration.index)?;
            let base = if sheet == Sheet::Map { 500 } else { 0 };
            Look {
                sheet,
                body: base + usize::from(body),
                shadow: Some((base + usize::from(shadow), Shade::Normal)),
            }
        }
        MapObject::Granite(granite) => {
            let item = usize::from(granite.size) + 6 * usize::from(granite.kind);
            Look::map(515 + item, 615 + item)
        }
        MapObject::Field(field) => {
            let item = usize::from(field.stage.index()) + 5 * usize::from(field.kind);
            Look::map(532 + item, 632 + item)
        }
        MapObject::Remnant(remnant) => {
            let look = usize::from(remnant.look.index());
            Look::map(500 + look, 600 + look)
        }
        MapObject::Sign(sign) => Look::map(680 + 3 * usize::from(sign.found) + usize::from(sign.amount), 699),
        MapObject::Fire(fire) => {
            let low = usize::from(fire.count) % ANIMATION_PHASES;
            let q = (3 * usize::from(node.x) + 2 * usize::from(node.y) + anim + low) % ANIMATION_PHASES;
            let set = 8 * usize::from(fire.set);
            Look {
                sheet: Sheet::Map,
                body: 2500 + set + q,
                shadow: Some((2530 + set + q, Shade::Grey)),
            }
        }
        MapObject::Remains(remains) => Look {
            sheet: Sheet::Map,
            body: 2524 + usize::from(remains.set),
            shadow: None,
        },
        _ => return None,
    })
}

/// Frames of the map animation counter that objects cycle through.
const ANIMATION_PHASES: usize = 8;

/// Items each tree type owns in the map archive.
const TREE_ITEMS: usize = 15;

/// The first item of a growing tree's frames, by stage.
const GROWING_FRAMES: usize = 8;

/// The first item of a falling tree's frames, by stage.
const FELLING_FRAMES: usize = 11;

/// The item a plain object word (class 0) draws.
const PLAIN_OBJECT: usize = 59;

/// The symbol the construction aid writes on a node with room for `size`, where `harbour` says
/// the map marks a harbour there. A node with room for nothing takes none.
const fn aid_item(size: SiteSize, harbour: bool) -> Option<usize> {
    Some(match size {
        SiteSize::Nothing => return None,
        SiteSize::Flag => AID_ITEMS,
        SiteSize::Small => AID_ITEMS + 1,
        SiteSize::Medium => AID_ITEMS + 2,
        SiteSize::Large if harbour => AID_HARBOUR,
        SiteSize::Large => AID_ITEMS + 3,
        SiteSize::Mine => AID_ITEMS + 4,
    })
}

/// Paints a shadow through `table`.
const fn darken(table: &Remap) -> Paint<'_> {
    Paint {
        base: 0,
        shadow: Some(table),
    }
}

/// One frame of the object pass.
pub(crate) struct ObjectPass<'a> {
    pub terrain: &'a Terrain,
    pub world: &'a World,
    pub graphics: &'a ObjectGraphics,
    pub layers: Layers,
    pub anim: u32,
    pub step: u32,
    /// The player whose land carries the construction aid, or nothing while it is off.
    pub aid: Option<Player>,
    /// What goes over every object, in order.
    pub marks: &'a [Mark],
}

impl ObjectPass<'_> {
    /// Draws the pass for a view scrolled to `scroll` (wrapped into the map) into `surface`,
    /// which holds the view's rows from `top` down.
    pub fn draw(&self, scroll: Point, surface: &mut Surface<'_>, top: i32, view: (u32, u32)) {
        let map = self.terrain.map();
        let walk = Walk::objects(scroll.x, scroll.y, view.0, view.1);
        for row in walk.row..walk.row + walk.rows {
            for column in walk.column..walk.column + walk.columns {
                let node = Node::wrapped(column, row, map.width(), map.height());
                if !self.terrain.node_visible(node) {
                    continue;
                }
                let shift = if row & 1 == 1 { ODD_ROW_SHIFT } else { 0 };
                let base = Point::new(
                    NODE_WIDTH * column + shift + ORIGIN_X - scroll.x,
                    ROW_HEIGHT * row - scroll.y,
                );
                let at = lift(base, map.get(node).altitude);
                self.object(surface, top, node, at);
                self.aid(surface, top, node, at);
                if self.layers.contains(Layers::FIGURES) {
                    for (record, figure) in self.world.figures_at(node) {
                        self.figure(surface, top, node, at, record, figure);
                    }
                }
            }
        }
        let map = Some(self.graphics.map.as_ref());
        for mark in self.marks {
            let at = match mark.place {
                Place::Node(node) => self.place(node, scroll),
                Place::Between(from, to) => {
                    let (from, to) = (self.place(from, scroll), self.place(to, scroll));
                    Point::new(i32::midpoint(from.x, to.x), i32::midpoint(from.y, to.y))
                }
            };
            self.sprite(surface, top, map, mark.item, at, Layers::OBJECTS, Paint::PLAIN);
        }
    }

    /// Where `node` lies in a view scrolled to `scroll`, height included, taking whichever
    /// copy of the wrapping map lies from just left of and above the view onwards.
    fn place(&self, node: Node, scroll: Point) -> Point {
        let map = self.terrain.map();
        let (width, height) = (
            NODE_WIDTH * i32::from(map.width()),
            ROW_HEIGHT * i32::from(map.height()),
        );
        let shift = if node.y & 1 == 1 { ODD_ROW_SHIFT } else { 0 };
        let x = NODE_WIDTH * i32::from(node.x) + shift + ORIGIN_X - scroll.x;
        let y = ROW_HEIGHT * i32::from(node.y) - scroll.y;
        let near = |value: i32, period: i32, margin: i32| (value + margin).rem_euclid(period) - margin;
        lift(
            Point::new(near(x, width, NODE_WIDTH * 2), near(y, height, ROW_HEIGHT * 2)),
            map.get(node).altitude,
        )
    }

    fn sheet(&self, sheet: Sheet) -> Option<&SpriteSheet> {
        match sheet {
            Sheet::Map => Some(self.graphics.map.as_ref()),
            Sheet::Mission => self.graphics.mission.as_deref(),
        }
    }

    fn table(&self, shade: Shade) -> &Remap {
        match shade {
            Shade::Normal => &self.graphics.shadow,
            Shade::Grey => &self.graphics.grey_shadow,
        }
    }

    /// Draws item `item` of `sheet` at `at` with `paint`, on `layer`, and on the shadow layer too
    /// when `paint` carries a shadow table.
    #[allow(
        clippy::too_many_arguments,
        reason = "one call site per kind of sprite, all alike"
    )]
    fn sprite(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        sheet: Option<&SpriteSheet>,
        item: usize,
        at: Point,
        layer: Layers,
        paint: Paint<'_>,
    ) {
        let pen = if paint.shadow.is_some() {
            layer | Layers::SHADOWS
        } else {
            layer
        };
        if !self.layers.contains(pen) {
            return;
        }
        let Some(sprite) = sheet.and_then(|s| s.get(item)) else {
            return;
        };
        surface.set_pen(pen);
        blit(surface, top, sprite, at, paint);
    }

    fn object(&self, surface: &mut Surface<'_>, top: i32, node: Node, at: Point) {
        let map = Some(self.graphics.map.as_ref());
        match self.world.object(node) {
            MapObject::Nothing => {}
            MapObject::Plain(_) => {
                self.sprite(surface, top, map, PLAIN_OBJECT, at, Layers::OBJECTS, Paint::PLAIN);
            }
            MapObject::Flag(record) => self.flag(surface, top, record, at),
            MapObject::Building(record) => self.building(surface, top, record, at),
            MapObject::ShipSite(site) => self.ship_site(surface, top, site, at),
            object => {
                let Some(look) = static_look(object, node, self.anim) else {
                    return;
                };
                let sheet = self.sheet(look.sheet);
                if let Some((item, shade)) = look.shadow {
                    let paint = darken(self.table(shade));
                    self.sprite(surface, top, sheet, item, at, Layers::OBJECTS, paint);
                }
                self.sprite(surface, top, sheet, look.body, at, Layers::OBJECTS, Paint::PLAIN);
            }
        }
    }

    /// The construction aid's symbol for `node`, which `0x1ef8e` writes over every node of the
    /// view's own player's land that has room for something.
    fn aid(&self, surface: &mut Surface<'_>, top: i32, node: Node, at: Point) {
        let Some(player) = self.aid else {
            return;
        };
        let data = self.terrain.map().get(node);
        if data.owner != Some(player) {
            return;
        }
        let harbour = data.flags.contains(NodeFlags::HARBOUR);
        let Some(item) = aid_item(data.sites.size, harbour) else {
            return;
        };
        let map = Some(self.graphics.map.as_ref());
        self.sprite(surface, top, map, item, at, Layers::OBJECTS, Paint::PLAIN);
    }

    /// A figure (`0x1aee4`): an animal's shadow, then its body, moved back towards the node it
    /// is walking from in proportion to the distance still to go. Ships draw in [`Self::ship`]
    /// and settlers in [`Self::settler`].
    fn figure(
        &self,
        surface: &mut Surface<'_>,
        top: i32,
        node: Node,
        at: Point,
        record: u16,
        figure: &Figure,
    ) {
        let (species, state) = match figure.kind {
            FigureKind::Animal { species, state, .. } => (species, state),
            FigureKind::Ship { record: ship, .. } => {
                self.ship(surface, top, (node, at), (record, ship), figure);
                return;
            }
            _ => {
                self.settler(surface, top, node, at, record, figure);
                return;
            }
        };
        let at = self.walking_offset(node, at, figure, (0, 0));
        let looks = species_sprites(species);
        let d = figure.direction as usize;
        let item = |offset: u16| SPECIES_BASE + usize::from(offset);
        let shadow = item(looks.shadow + looks.shadow_offset[d]);
        let body = match state {
            AnimalState::Dead => item(looks.dead),
            AnimalState::Resting => item(looks.rest) + d * usize::from(looks.rest_step),
            AnimalState::Active | AnimalState::Shot => {
                let frames = looks.frames.max(1);
                item(looks.walk) + d * usize::from(frames) + usize::from(figure.counter % frames)
            }
        };
        let map = Some(self.graphics.map.as_ref());
        let paint = darken(&self.graphics.shadow);
        self.sprite(surface, top, map, shadow, at, Layers::FIGURES, paint);
        self.sprite(surface, top, map, body, at, Layers::FIGURES, Paint::PLAIN);
    }

    /// Where a walking figure is drawn: `at` plus `((V + door) · t) >> 8`, with the height change
    /// scaled the same way. `V` points back to the node it came from, `t` is the distance still to
    /// go, and `door` is the door offset of the building a settler walks out of.
    fn walking_offset(&self, node: Node, at: Point, figure: &Figure, door: (i32, i32)) -> Point {
        walking_offset(self.terrain.map(), node, at, figure, door)
    }
}

/// See [`ObjectPass::walking_offset`].
fn walking_offset(map: &NodeMap, node: Node, at: Point, figure: &Figure, door: (i32, i32)) -> Point {
    let t = i32::from(figure.remaining);
    if t == 0 {
        return at;
    }
    let (vx, vy) = figure.direction.offset();
    let origin = node.neighbour(figure.direction.opposite(), map.width(), map.height());
    let height = |n: Node| i32::from(map.get(n).altitude);
    let rise = t * HEIGHT_STEP * (height(node) - height(origin));
    at.offset(((door.0 - vx) * t) >> 8, (((door.1 - vy) * t) >> 8) + (rise >> 8))
}

/// Where `figure` is on `map` in world pixels, as the view that follows a figure asks
/// (`0x17f7f`): its node's place, and while it walks, back along the edge by the distance still
/// to go.
#[must_use]
pub fn figure_position(map: &NodeMap, figure: &Figure) -> Point {
    let node = figure.node();
    let at = node.world_position(map.get(node).altitude);
    walking_offset(map, node, at, figure, (0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use s2_sim::ObjectWord;

    #[test]
    fn the_construction_aid_picks_a_symbol_for_each_kind_of_site() {
        assert_eq!(aid_item(SiteSize::Nothing, false), None);
        assert_eq!(aid_item(SiteSize::Flag, false), Some(50));
        assert_eq!(aid_item(SiteSize::Small, false), Some(51));
        assert_eq!(aid_item(SiteSize::Medium, false), Some(52));
        assert_eq!(aid_item(SiteSize::Large, false), Some(53));
        assert_eq!(aid_item(SiteSize::Mine, false), Some(54));
        assert_eq!(
            aid_item(SiteSize::Large, true),
            Some(55),
            "a harbour site takes the castle with the anchor"
        );
        assert_eq!(
            aid_item(SiteSize::Medium, true),
            Some(52),
            "the mark only tells on a site a harbour fits"
        );
    }

    /// The look of the object word with slot 5 `group` and slot 4 `detail`.
    fn word_look(group: u8, detail: u8, node: Node, anim: u32) -> Option<Look> {
        static_look(ObjectWord(u16::from_le_bytes([detail, group])).into(), node, anim)
    }

    /// Slot 5 of a static object in `group` and `set`.
    const fn slot5(group: u8, set: u8) -> u8 {
        192 + 4 * group + set
    }

    const TREES: u8 = slot5(1, 0);
    const DECORATIONS: u8 = slot5(2, 0);
    const GRANITE: u8 = slot5(3, 0);
    const FIRE: u8 = slot5(7, 0);

    #[test]
    fn trees_animate_only_when_grown() {
        let node = Node { x: 0, y: 0 };
        let tree = |detail, anim| word_look(TREES, detail, node, anim).unwrap();
        assert_eq!(tree(48, 0).body, 200, "grown, phase 0");
        assert_eq!(tree(51, 6).body, 201, "phase (6 + 3) & 7");
        assert_eq!(tree(48, 6).shadow, Some((356, Shade::Normal)));
        assert_eq!(tree(16, 5).body, 209, "stage 1 stands still");
        assert_eq!(tree(24, 5).body, 212, "bit 3 picks the other stills");
        assert_eq!(word_look(TREES + 1, 64, node, 0).unwrap().body, 200 + 15 * 5 + 8);
    }

    #[test]
    fn decorations_and_granite() {
        let node = Node { x: 0, y: 0 };
        let look = |group, detail| word_look(group, detail, node, 0);
        assert_eq!(look(DECORATIONS, 5), Some(Look::map(505, 605)));
        assert_eq!(look(DECORATIONS, 18), Some(Look::map(544, 644)));
        assert_eq!(look(DECORATIONS, 23), Some(Look::map(561, 661)));
        assert_eq!(look(DECORATIONS, 43), Some(Look::map(559, 659)));
        let mission = look(DECORATIONS, 26).unwrap();
        assert_eq!(
            (mission.sheet, mission.body, mission.shadow),
            (Sheet::Mission, 4, Some((5, Shade::Normal)))
        );
        assert_eq!(look(DECORATIONS, 33).unwrap().body, 30);
        assert_eq!(look(DECORATIONS, 51), None);
        assert_eq!(look(GRANITE, 1), Some(Look::map(516, 616)));
        assert_eq!(look(GRANITE + 1, 6), Some(Look::map(527, 627)));
        assert_eq!(look(slot5(0, 0), 0), None);
    }

    #[test]
    fn walking_figures_are_drawn_back_towards_where_they_came_from() {
        use crate::terrain::testing;
        use s2_sim::Direction;
        use s2_sim::Species;
        let mut map = testing::flat(8, 8, 1);
        let destination = Node { x: 2, y: 2 };
        let mut high = map.get(destination);
        high.altitude = 4;
        map.set(destination, high);
        let terrain = testing::terrain(map, testing::texture(|_, _| 1), testing::shading(|_, t| t));
        let world = World::empty(8, 8);
        let graphics = ObjectGraphics::new(Arc::default(), Remap::identity(), Remap::identity());
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
        let mut fox = Figure::animal(Species::Fox);
        fox.direction = Direction::SouthEast;
        fox.remaining = 128;
        let at = Point::new(100, 100);
        assert_eq!(
            pass.walking_offset(destination, at, &fox, (0, 0)),
            Point::new(86, 96),
            "halfway back up-left, and 4 height units of climb still to come"
        );
        assert_eq!(
            pass.walking_offset(destination, at, &fox, (2, 4)),
            Point::new(87, 98),
            "a door offset scales with the distance left"
        );
        fox.direction = Direction::East;
        fox.remaining = 1;
        assert_eq!(
            pass.walking_offset(destination, at, &fox, (0, 0)),
            Point::new(99, 100),
            "shifts round down"
        );
        fox.remaining = 0;
        assert_eq!(pass.walking_offset(destination, at, &fox, (0, 0)), at);
    }

    #[test]
    fn fire_moves_with_the_position() {
        let at = |x, y| word_look(FIRE, 0, Node { x, y }, 0).unwrap().body;
        assert_eq!(at(0, 0), 2500);
        assert_eq!(at(1, 1), 2505);
        assert_eq!(
            word_look(FIRE, 0, Node { x: 0, y: 0 }, 0).unwrap().shadow,
            Some((2530, Shade::Grey))
        );
    }
}
