//! The view pass: each node's two triangles, up to six border triangles, its roads and its
//! border stones, in the original's order (`S2.EXE` `0x1f92c`, `0x1c9b4`, `0x1cc1c`).

use std::sync::Arc;

use s2_assets::{Landscape, LandscapeAssets, Sprite, TEXTURE_SIZE};
use s2_formats::gouraud::ShadingTable;
use s2_sim::{Direction, Edges, Node, NodeFlags, NodeMap, Player, RoadKind};

use super::filler::{Material, Surface, Vertex, fill};
use super::fog::{Fog, Sight};
use super::geometry::{
    BACKGROUND, NODE_WIDTH, ODD_ROW_SHIFT, ORIGIN_X, ROW_HEIGHT, ScreenNode, ScreenOffset, Walk, lift,
    world_size,
};
use super::tables::{TerrainTables, TerrainType, TextureEntry};
use super::units::{Fixed8, Point, Subpixel};
use crate::Layers;
use crate::sprite::{Paint, blit, colour_base};

/// How many rows below its starting row the pick tries (`0x1eadc`).
pub const PICK_ROWS: i64 = 20;

/// How many columns the pick tries in each row.
pub const PICK_COLUMNS: i64 = 3;

/// The squared distance the pick starts from, which every node in range beats.
const PICK_FAR: i64 = 1_000_000;

/// A map's terrain with the landscape it is drawn in: everything the view pass needs.
#[derive(Debug, Clone)]
pub struct Terrain {
    map: NodeMap,
    set: usize,
    texture: Box<[u8; TEXTURE_SIZE]>,
    shading: ShadingTable,
    tables: Arc<TerrainTables>,
    fog: Option<Fog>,
    layers: Layers,
    stones: [Option<Sprite>; Player::COUNT],
}

/// A triangle corner before the one-pixel push: unpushed position and 8.8 shading row.
#[derive(Debug, Clone, Copy)]
struct Corner {
    at: Point,
    light: Fixed8,
}

impl Corner {
    fn vertex(self, (dx, dy): (i32, i32), (u, v): (u16, u16), light: Fixed8) -> Vertex {
        let at = self.at.offset(dx, dy);
        Vertex {
            x: Subpixel::from_pixels(at.x),
            y: Subpixel::from_pixels(at.y),
            u: Fixed8::from_raw(i32::from(u)),
            v: Fixed8::from_raw(i32::from(v)),
            light,
        }
    }

    fn centre(a: Self, b: Self, c: Self) -> Self {
        Self {
            at: Point::new((a.at.x + b.at.x + c.at.x) / 3, (a.at.y + b.at.y + c.at.y) / 3),
            light: Fixed8::from_raw((a.light.raw() + b.light.raw() + c.light.raw()) / 3),
        }
    }
}

/// The colour the map screen paints a node of each tree type, by type
/// (`0x1fb696`). A 0 leaves the terrain's own colour showing.
pub const TREE_COLOURS: [u8; 9] = [41, 42, 40, 0, 0, 0, 41, 42, 40];

/// The colour a texture entry asking for [`TextureEntry::MAP_TEXEL`] paints with (`0x1fbe18`).
pub const MAP_FLAT: u8 = 61;

/// How many texels a row of the terrain texture holds.
const TEXTURE_WIDTH: usize = 256;

/// The shading row of unshaded textures and the water border strip.
const UNSHADED: Fixed8 = Fixed8::from_int(64);

/// The first texture row of a road type's band.
const fn band(kind: RoadKind) -> u8 {
    match kind {
        RoadKind::Normal => 0,
        RoadKind::Donkey => 16,
        RoadKind::Water => 32,
    }
}

/// Border strip texture coordinates: the strip's top row from x 192 to 255 along the shared
/// edge, its middle bottom texel pulled to the triangle's centre.
const STRIP_U: [u16; 3] = [192 << 8, 255 << 8, 224 << 8];

/// The strip that is drawn unshaded, like water.
const WATER_STRIP: u8 = 4;

/// The 8.8 fraction the original samples a road or marker texel at, just under a half.
const MID_TEXEL: u16 = 127;

/// Road texture columns, 8.8: 192.5 at the node, 239.5 at the neighbour.
const ROAD_U: [u16; 2] = [(192 << 8) + MID_TEXEL, (239 << 8) + MID_TEXEL];

/// The node marker's texture columns, 8.8: 240.5 on its upper side, 254.5 on its lower.
const MARKER_U: [u16; 2] = [(240 << 8) + MID_TEXEL, (254 << 8) + MID_TEXEL];

/// The band every road of a node uses next to mountain ([`s2_sim::NodeData::mountain_roads`]).
const ROAD_BAND_SWITCHED: u8 = 160;

/// Rows below a band's top that a road quad's far side samples: the band's last row.
const ROAD_DEPTH: u16 = 15;

/// Rows below a band's top that the node marker's far side samples.
const MARKER_DEPTH: u16 = 14;

/// One corner of a road quad: whether it sits at the neighbour, its offset from that end's
/// lifted position, and whether it samples the band's last row rather than its first.
type RoadCorner = (bool, (i32, i32), bool);

/// A road quad's corners in the order `S2.EXE` hands them to the filler (`0x1e33a`–`0x1e7cb`).
const fn road_corners(direction: Direction) -> [RoadCorner; 4] {
    match direction {
        Direction::SouthEast => [
            (false, (-5, 0), false),
            (false, (0, -5), true),
            (true, (5, 0), true),
            (true, (0, 5), false),
        ],
        Direction::SouthWest => [
            (false, (0, -5), false),
            (false, (5, 0), true),
            (true, (0, 5), true),
            (true, (-5, 0), false),
        ],
        _ => [
            (false, (-3, 3), false),
            (false, (-3, -3), true),
            (true, (3, -3), true),
            (true, (3, 3), false),
        ],
    }
}

/// `value` wrapped into `0..period`.
fn wrap(value: i32, period: u32) -> u32 {
    u32::try_from(i64::from(value).rem_euclid(i64::from(period.max(1)))).unwrap_or_default()
}

/// How the original cuts `len` pixels from wrapped scroll position `scroll` at the map's edge
/// `period`: (offset into the view, scroll, length) for each part.
fn split(scroll: u32, len: u32, period: u32) -> impl Iterator<Item = (u32, u32, u32)> {
    let first = len.min(period.saturating_sub(scroll));
    [(0, scroll, first), (first, 0, len - first)]
        .into_iter()
        .filter(|&(_, _, len)| len > 0)
}

/// A texture row of a road band in 8.8, `depth` rows below its top, sampled mid-texel.
const fn band_row(top: u8, depth: u16) -> u16 {
    ((top as u16 + depth) << 8) + MID_TEXEL
}

impl Terrain {
    /// `map` drawn with `landscape`'s 256 × 256 `texture` sheet and `shading` table, through
    /// [`TerrainTables::ORIGINAL`].
    #[must_use]
    pub fn new(
        map: NodeMap,
        landscape: Landscape,
        texture: Box<[u8; TEXTURE_SIZE]>,
        shading: ShadingTable,
    ) -> Self {
        Self {
            map,
            set: landscape.set(),
            texture,
            shading,
            tables: Arc::new(TerrainTables::ORIGINAL),
            fog: None,
            layers: Layers::ALL,
            stones: Default::default(),
        }
    }

    /// The layers drawn, of which the terrain pass uses [`Layers::TERRAIN`].
    #[must_use]
    pub fn layers(&self) -> Layers {
        self.layers
    }

    /// Draws only `layers` from now on. Cached tiles notice the change on their own.
    pub fn set_layers(&mut self, layers: Layers) {
        self.layers = layers;
    }

    /// The border stone drawn for `player`'s territory: item 0 of the player's nation sheet.
    /// Without one, that player's stones are not drawn. Cached tiles need
    /// [`super::TileCache::invalidate_all`].
    pub fn set_border_stone(&mut self, player: Player, stone: Option<Sprite>) {
        self.stones[player.index()] = stone;
    }

    /// The same terrain drawn through `tables` instead. Cached tiles need
    /// [`super::TileCache::invalidate_all`].
    #[must_use]
    pub fn with_tables(mut self, tables: Arc<TerrainTables>) -> Self {
        self.tables = tables;
        self
    }

    /// The tables the terrain is drawn through.
    #[must_use]
    pub fn tables(&self) -> &TerrainTables {
        &self.tables
    }

    /// `map` drawn with `assets`.
    #[must_use]
    pub fn with_assets(map: NodeMap, assets: &LandscapeAssets) -> Self {
        Self::new(
            map,
            assets.landscape(),
            Box::new(*assets.texture()),
            assets.shading().clone(),
        )
    }

    /// The map's layers.
    #[must_use]
    pub fn map(&self) -> &NodeMap {
        &self.map
    }

    /// The map's layers, to change. Cached tiles over changed nodes need
    /// [`super::TileCache::invalidate_nodes`].
    pub fn map_mut(&mut self) -> &mut NodeMap {
        &mut self.map
    }

    /// The map's size in world pixels.
    #[must_use]
    pub fn world_size(&self) -> (u32, u32) {
        world_size(self.map.width(), self.map.height())
    }

    /// The node `S2.EXE` picks for world pixel `point` (`0x1eadc`), which the cursor and the map
    /// window's marker both use. The search starts at the node column and row half a node left
    /// of and half a row above the point, tries [`PICK_ROWS`] rows down and [`PICK_COLUMNS`]
    /// columns across, and keeps the first node drawn nearest to the point, heights included.
    /// Distances wrap with the world, so any point names a node.
    #[must_use]
    pub fn node_at(&self, point: Point) -> Node {
        let (width, height) = (self.map.width(), self.map.height());
        let (period_x, period_y) = self.world_size();
        let (period_x, period_y) = (i64::from(period_x), i64::from(period_y));
        let px = i64::from(point.x).rem_euclid(period_x);
        let py = i64::from(point.y).rem_euclid(period_y);
        let column = (px - i64::from(ORIGIN_X)).rem_euclid(period_x) / i64::from(NODE_WIDTH);
        let row = (py - i64::from(ROW_HEIGHT / 2)).rem_euclid(period_y) / i64::from(ROW_HEIGHT);
        let apart = |from: i64, to: i64, period: i64| {
            let gap = (to - from).rem_euclid(period);
            if gap > period / 2 { gap - period } else { gap }
        };
        let mut best = (PICK_FAR, Node::wrapped(0, 0, width, height));
        for down in 0..PICK_ROWS {
            for across in 0..PICK_COLUMNS {
                let node = Node::wrapped(
                    i32::try_from(column + across).unwrap_or_default(),
                    i32::try_from(row + down).unwrap_or_default(),
                    width,
                    height,
                );
                let at = node.world_position(self.map.get(node).altitude);
                let dx = apart(px, i64::from(at.x), period_x);
                let dy = apart(py, i64::from(at.y), period_y);
                let distance = dx * dx + dy * dy;
                if distance < best.0 {
                    best = (distance, node);
                }
            }
        }
        best.1
    }

    /// The viewing player's fog, or `None` when everything is visible (the default).
    #[must_use]
    pub fn fog(&self) -> Option<&Fog> {
        self.fog.as_ref()
    }

    /// The fog, to reveal more of the map. Cached tiles over changed nodes need
    /// [`super::TileCache::invalidate_nodes`]; [`Fog::reveal`] returns the nodes to pass.
    pub fn fog_mut(&mut self) -> Option<&mut Fog> {
        self.fog.as_mut()
    }

    /// Turns the fog on with `fog`, or off with `None`. Cached tiles need
    /// [`super::TileCache::invalidate_all`].
    pub fn set_fog(&mut self, fog: Option<Fog>) {
        self.fog = fog;
    }

    /// The fog the view is drawn with, or none when there is no fog or [`Layers::FOG`] is off.
    /// Either way every triangle then counts as seen.
    #[must_use]
    pub fn active_fog(&self) -> Option<&Fog> {
        self.fog.as_ref().filter(|_| self.layers.contains(Layers::FOG))
    }

    /// How `node`'s two triangles are drawn under the current fog.
    #[must_use]
    pub fn sight(&self, node: Node) -> [Sight; 2] {
        self.active_fog()
            .map_or([Sight::Visible; 2], |fog| fog.sight(node))
    }

    /// The edges of `node` that roads and border stones may be drawn along.
    #[must_use]
    pub fn visible_edges(&self, node: Node) -> Edges {
        self.active_fog().map_or(Edges::all(), |fog| fog.edges(node))
    }

    /// Whether objects, flags, buildings and figures on `node` are drawn.
    #[must_use]
    pub fn node_visible(&self, node: Node) -> bool {
        self.active_fog().is_none_or(|fog| fog.node_visible(node))
    }

    /// The colour the map screen paints `node` with, before the overlays.
    ///
    /// `tree` is the type of a grown tree standing on the node, which colours it over the
    /// terrain when [`TREE_COLOURS`] gives that type a colour.
    #[must_use]
    pub fn map_colour(&self, node: Node, tree: Option<u8>) -> u8 {
        if let Some(colour) = tree
            .and_then(|kind| TREE_COLOURS.get(usize::from(kind)).copied())
            .filter(|&colour| colour != 0)
        {
            return colour;
        }
        let data = self.map.get(node);
        let Some(entry) = self.tables.texture(self.terrain(node, None, 1)) else {
            return MAP_FLAT;
        };
        let index = match entry.colour {
            0 => {
                let (u, v) = entry.uv[0];
                let at = usize::from(v >> 8) * TEXTURE_WIDTH + usize::from(u >> 8);
                self.texture.get(at).copied().unwrap_or(MAP_FLAT)
            }
            TextureEntry::MAP_TEXEL => MAP_FLAT,
            colour => colour,
        };
        if entry.flat {
            index
        } else {
            self.shading.shade(data.light, index)
        }
    }

    /// Draws the view whose top-left corner is world pixel (`x`, `y`) into `surface`, which
    /// is the view and the clip window. The map repeats in both directions.
    ///
    /// A view that runs past the map's right or bottom edge is cut there into up to four parts,
    /// as in the original's full redraw (`0x765b8`, `0x766d7`). The part past an edge is drawn as
    /// a view of its own, scrolled to 0 on that axis. Each part is cleared to [`BACKGROUND`] and
    /// its nodes walked row by row, so triangles straddling the seam are clipped at it.
    pub fn draw(&self, x: i32, y: i32, surface: &mut Surface<'_>) {
        let (period_x, period_y) = self.world_size();
        let x = wrap(x, period_x);
        let y = wrap(y, period_y);
        for (left, scroll_x, width) in split(x, surface.width(), period_x) {
            for (top, scroll_y, height) in split(y, surface.height(), period_y) {
                let mut part = surface.window(left, top, width, height);
                self.pass(scroll_x, scroll_y, &mut part);
            }
        }
    }

    /// One call of the original's terrain pass (`0x1f92c`) for a view scrolled to (`x`, `y`)
    /// within the map's first period.
    fn pass(&self, x: u32, y: u32, surface: &mut Surface<'_>) {
        surface.clear(BACKGROUND);
        let (x, y) = (x.cast_signed(), y.cast_signed());
        let walk = Walk::new(x, y, surface.width(), surface.height());
        for row in walk.row..walk.row + walk.rows {
            for column in walk.column..walk.column + walk.columns {
                let node = Node::wrapped(column, row, self.map.width(), self.map.height());
                let shift = if node.odd_row() { ODD_ROW_SHIFT } else { 0 };
                let base = Point::new(NODE_WIDTH * column + shift + ORIGIN_X - x, ROW_HEIGHT * row - y);
                self.draw_node(surface, node, base);
            }
        }
    }

    fn material(&self) -> Material<'_> {
        Material {
            texture: &self.texture,
            shading: &self.shading,
        }
    }

    fn corner(&self, node: Node, base: Point, direction: Option<Direction>) -> Corner {
        let (target, (dx, dy)) = match direction {
            Some(d) => (node.neighbour(d, self.map.width(), self.map.height()), d.offset()),
            None => (node, (0, 0)),
        };
        let data = self.map.get(target);
        Corner {
            at: lift(base.offset(dx, dy), data.altitude),
            light: Fixed8::from_int(data.light),
        }
    }

    fn terrain(&self, node: Node, direction: Option<Direction>, slot: usize) -> TerrainType {
        let node = direction.map_or(node, |d| node.neighbour(d, self.map.width(), self.map.height()));
        TerrainType::from_byte(self.map.get(node).terrain[slot])
    }

    fn draw_node(&self, surface: &mut Surface<'_>, node: Node, base: Point) {
        let here = self.corner(node, base, None);
        let east = self.corner(node, base, Some(Direction::East));
        let south_east = self.corner(node, base, Some(Direction::SouthEast));
        let south_west = self.corner(node, base, Some(Direction::SouthWest));
        let first = self.terrain(node, None, 0);
        let second = self.terrain(node, None, 1);
        let [sight_first, sight_second] = self.sight(node);

        let first_corners = [(here, (0, -1)), (south_east, (1, 1)), (south_west, (-1, 1))];
        let second_corners = [(here, (-1, -1)), (east, (1, -1)), (south_east, (0, 1))];
        let first = (sight_first != Sight::Hidden).then_some(first);
        let second = (sight_second != Sight::Hidden).then_some(second);
        if self.layers.contains(Layers::TRIANGLES) {
            surface.set_pen(Layers::TRIANGLES);
            if let Some(kind) = first {
                self.triangle(surface, kind, 0, first_corners, sight_first);
            }
            if let Some(kind) = second {
                self.triangle(surface, kind, 3, second_corners, sight_second);
            }
        }
        let road = |direction, edge| {
            self.map
                .get(node.neighbour(direction, self.map.width(), self.map.height()))
                .roads
                .along(edge)
                .is_some()
        };
        if self.layers.contains(Layers::BORDERS) {
            surface.set_pen(Layers::BORDERS);
            if let Some(kind) = first {
                let centre = Corner::centre(here, south_east, south_west);
                let neighbours = [
                    Some(self.terrain(node, None, 1)),
                    (!road(Direction::SouthWest, Edges::EAST))
                        .then(|| self.terrain(node, Some(Direction::SouthWest), 1)),
                    Some(self.terrain(node, Some(Direction::West), 1)),
                ];
                for (edge, neighbour) in neighbours.into_iter().enumerate() {
                    let ends = [first_corners[edge], first_corners[(edge + 1) % 3]];
                    if let Some(neighbour) = neighbour {
                        self.border(surface, kind, neighbour, ends, centre, sight_first);
                    }
                }
            }
            if let Some(kind) = second {
                let centre = Corner::centre(here, east, south_east);
                let neighbours = [
                    Some(self.terrain(node, Some(Direction::NorthEast), 0)),
                    (!road(Direction::East, Edges::SOUTH_WEST))
                        .then(|| self.terrain(node, Some(Direction::East), 0)),
                    Some(self.terrain(node, None, 0)),
                ];
                for (edge, neighbour) in neighbours.into_iter().enumerate() {
                    let ends = [second_corners[edge], second_corners[(edge + 1) % 3]];
                    if let Some(neighbour) = neighbour {
                        self.border(surface, kind, neighbour, ends, centre, sight_second);
                    }
                }
            }
        }
        if self.layers.contains(Layers::ROADS) {
            surface.set_pen(Layers::ROADS);
            self.roads(surface, node, base, here);
        }
        if self.layers.contains(Layers::STONES) {
            surface.set_pen(Layers::STONES);
            self.stones(surface, node, base, here);
        }
    }

    /// The node's roads east, south-east and south-west along visible edges, then its marker:
    /// quads through the filler with colour key 0. Each corner takes its own end's light, never
    /// dimmed by fog. Type 3 is drawn unshaded unless the node's band switch is set.
    fn roads(&self, surface: &mut Surface<'_>, node: Node, base: Point, here: Corner) {
        let data = self.map.get(node);
        if data.roads.is_empty() {
            return;
        }
        let switched = data.flags.contains(NodeFlags::MOUNTAIN_ROADS);
        let top = |kind: RoadKind| if switched { ROAD_BAND_SWITCHED } else { band(kind) };
        let light = |kind: RoadKind, corner: Corner| {
            if kind == RoadKind::Water && !switched {
                UNSHADED
            } else {
                corner.light
            }
        };
        let edges = self.visible_edges(node);
        for (direction, edge) in Edges::DIRECTIONS {
            let Some(kind) = data.roads.along(edge) else {
                continue;
            };
            if !edges.contains(edge) {
                continue;
            }
            let there = self.corner(node, base, Some(direction));
            let top = top(kind);
            let vertices = road_corners(direction).map(|(far, push, deep)| {
                let end = if far { there } else { here };
                let v = band_row(top, if deep { ROAD_DEPTH } else { 0 });
                end.vertex(push, (ROAD_U[usize::from(far)], v), light(kind, end))
            });
            fill(surface, self.material(), &vertices, Some(0));
        }
        if let Some(kind) = data.roads.marker()
            && self.node_visible(node)
        {
            let (upper, lower) = (band_row(top(kind), 0), band_row(top(kind), MARKER_DEPTH));
            let light = light(kind, here);
            let vertices = [
                here.vertex((-3, -3), (MARKER_U[0], upper), light),
                here.vertex((3, -3), (MARKER_U[0], lower), light),
                here.vertex((3, 3), (MARKER_U[1], lower), light),
                here.vertex((-3, 3), (MARKER_U[1], upper), light),
            ];
            fill(surface, self.material(), &vertices, Some(0));
        }
    }

    /// Three stones along each visible border edge leaving the node, at the node, halfway and
    /// at the neighbour, in the owner's colour and without shadows.
    fn stones(&self, surface: &mut Surface<'_>, node: Node, base: Point, here: Corner) {
        let data = self.map.get(node);
        let Some(owner) = data.owner else {
            return;
        };
        let edges = data.borders & self.visible_edges(node);
        let Some(stone) = self.stones[owner.index()].as_ref() else {
            return;
        };
        let paint = Paint {
            base: colour_base(owner),
            shadow: None,
        };
        for (direction, edge) in Edges::DIRECTIONS {
            if !edges.contains(edge) {
                continue;
            }
            let there = self.corner(node, base, Some(direction)).at;
            let (dx, dy) = (there.x - here.at.x, there.y - here.at.y);
            for i in 0..3 {
                blit(surface, 0, stone, here.at.offset(i * dx / 2, i * dy / 2), paint);
            }
        }
    }

    fn triangle(
        &self,
        surface: &mut Surface<'_>,
        kind: TerrainType,
        first_uv: usize,
        corners: [(Corner, (i32, i32)); 3],
        sight: Sight,
    ) {
        let Some(entry) = self.tables.texture(kind) else {
            return;
        };
        let vertices = std::array::from_fn::<_, 3, _>(|i| {
            let (corner, push) = corners[i];
            let light = if entry.flat { UNSHADED } else { corner.light };
            corner.vertex(push, entry.uv[first_uv + i], light.shr(sight.shift()))
        });
        fill(surface, self.material(), &vertices, None);
    }

    fn border(
        &self,
        surface: &mut Surface<'_>,
        receiving: TerrainType,
        neighbour: TerrainType,
        [(a, push_a), (b, push_b)]: [(Corner, (i32, i32)); 2],
        centre: Corner,
        sight: Sight,
    ) {
        if neighbour == receiving {
            return;
        }
        let (Some(receiving), Some(neighbour)) = (self.tables.info(receiving), self.tables.info(neighbour))
        else {
            return;
        };
        if !self.tables.border_drawn(self.set, receiving, neighbour) {
            return;
        }
        let strip = neighbour.strip[self.set];
        // The strip's first and last texture rows in 8.8: 16 rows per strip from row 176.
        let top = (176 + 16 * u16::from(strip)) << 8;
        let bottom = (191 + 16 * u16::from(strip)) << 8;
        let light = |corner: Corner| {
            let light = if strip == WATER_STRIP {
                UNSHADED
            } else {
                corner.light
            };
            light.shr(sight.shift())
        };
        let vertices = [
            a.vertex(push_a, (STRIP_U[0], top), light(a)),
            b.vertex(push_b, (STRIP_U[1], top), light(b)),
            centre.vertex((0, 0), (STRIP_U[2], bottom), light(centre)),
        ];
        fill(surface, self.material(), &vertices, Some(0));
    }
}

#[cfg(test)]
mod tests;
