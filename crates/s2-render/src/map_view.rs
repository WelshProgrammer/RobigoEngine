//! A map as the main map view draws it: terrain, then objects, buildings, flags and figures.

use std::sync::Arc;

use rayon::prelude::*;
use s2_assets::{Nation, SpriteSheet};

use crate::objects::{Clocks, Mark, ObjectGraphics, ObjectPass};
use crate::terrain::{Point, Surface, Terrain, TileCache};
use crate::{Frame, Layers, Provenance};
use s2_sim::{Node, NodeMap, NodeRect, Player, World};

/// Rows of the frame each worker draws the object pass into.
const BAND: usize = 64;

/// A map's terrain and the graphics its objects and figures are drawn with. The objects and
/// figures come from the [`World`] passed to each draw, so the view holds no copy of a running
/// game and draws it as it changes.
#[derive(Debug, Clone)]
pub struct MapView {
    terrain: Terrain,
    graphics: ObjectGraphics,
    aid: Option<Player>,
    marks: Vec<Mark>,
}

impl MapView {
    /// `terrain` drawn with `graphics`. Each player that `graphics` gives a nation gets that
    /// nation's border stone.
    #[must_use]
    pub fn new(mut terrain: Terrain, graphics: ObjectGraphics) -> Self {
        for index in 0..Player::COUNT {
            let Some(player) = u8::try_from(index).ok().and_then(Player::new) else {
                continue;
            };
            let stone = graphics
                .nation(player)
                .and_then(|(_, sheet)| sheet.get(0).cloned());
            terrain.set_border_stone(player, stone);
        }
        Self {
            terrain,
            graphics,
            aid: None,
            marks: Vec::new(),
        }
    }

    /// Writes the construction aid over `player`'s own land, or nothing to take it off again.
    pub const fn set_aid(&mut self, player: Option<Player>) {
        self.aid = player;
    }

    /// Sets what goes over every object, drawn in order.
    pub fn set_marks(&mut self, marks: Vec<Mark>) {
        self.marks = marks;
    }

    /// Gives `player` a nation, for its buildings, flags and border stones. Cached tiles need
    /// [`TileCache::invalidate_all`].
    pub fn set_nation(&mut self, player: Player, nation: Nation, sheet: Arc<SpriteSheet>) {
        self.terrain.set_border_stone(player, sheet.get(0).cloned());
        self.graphics.set_nation(player, nation, sheet);
    }

    /// The terrain.
    #[must_use]
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// The terrain, to change (fog, layers, nodes).
    pub fn terrain_mut(&mut self) -> &mut Terrain {
        &mut self.terrain
    }

    /// Brings the terrain's nodes up to date with `nodes`, a running game's, and drops the tiles
    /// over every node that changed. Returns how many changed.
    pub fn sync(&mut self, nodes: &NodeMap, tiles: &mut TileCache) -> usize {
        let map = self.terrain.map_mut();
        if (map.width(), map.height()) != (nodes.width(), nodes.height()) {
            return 0;
        }
        let mut changed = 0;
        for y in 0..nodes.height() {
            for x in 0..nodes.width() {
                let node = Node { x, y };
                let data = nodes.get(node);
                if map.get(node) != data {
                    map.set(node, data);
                    tiles.invalidate_nodes(NodeRect {
                        x: i32::from(x),
                        y: i32::from(y),
                        width: 1,
                        height: 1,
                    });
                    changed += 1;
                }
            }
        }
        if let Some(fog) = self.terrain.fog_mut() {
            fog.follow(nodes);
        }
        changed
    }

    /// The layers drawn.
    #[must_use]
    pub fn layers(&self) -> Layers {
        self.terrain.layers()
    }

    /// Draws only `layers` from now on.
    pub fn set_layers(&mut self, layers: Layers) {
        self.terrain.set_layers(layers);
    }

    /// Draws both passes of the view of `world` into `frame`, its top-left corner at world pixel
    /// `scroll` and the clocks at `clocks`. This is the reference output. With a
    /// `provenance` of the frame's size, also records which layers wrote each pixel.
    pub fn draw(
        &self,
        world: &World,
        scroll: Point,
        clocks: Clocks,
        frame: &mut Frame,
        mut provenance: Option<&mut Provenance>,
    ) {
        let scroll = self.wrap(scroll);
        let size = (frame.width(), frame.height());
        match provenance
            .as_deref_mut()
            .filter(|p| (p.width(), p.height()) == size)
        {
            Some(p) => {
                if let Some(mut surface) = frame.traced_surface(p) {
                    self.terrain.draw(scroll.x, scroll.y, &mut surface);
                }
            }
            None => self.terrain.draw(scroll.x, scroll.y, &mut frame.surface()),
        }
        self.draw_objects(world, scroll, clocks, frame, provenance);
    }

    /// Fills `frame` with the view at `scroll` from `tiles`, then draws the object pass of
    /// `world` over it.
    pub fn compose(
        &self,
        world: &World,
        tiles: &mut TileCache,
        scroll: Point,
        clocks: Clocks,
        frame: &mut Frame,
    ) {
        let scroll = self.wrap(scroll);
        tiles.compose(&self.terrain, scroll.x, scroll.y, &mut frame.surface());
        self.draw_objects(world, scroll, clocks, frame, None);
    }

    /// The object pass alone, over a frame that already holds the terrain, in bands drawn in
    /// parallel. Every band walks the same nodes in the same order, so the result is the same
    /// as one pass over the whole frame.
    pub fn draw_objects(
        &self,
        world: &World,
        scroll: Point,
        clocks: Clocks,
        frame: &mut Frame,
        provenance: Option<&mut Provenance>,
    ) {
        let scroll = self.wrap(scroll);
        let pass = ObjectPass {
            terrain: &self.terrain,
            world,
            graphics: &self.graphics,
            layers: self.terrain.layers(),
            anim: clocks.anim,
            step: clocks.step,
            aid: self.aid,
            marks: &self.marks,
        };
        let view = (frame.width(), frame.height());
        let width = view.0 as usize;
        if width == 0 {
            return;
        }
        let draw = |(index, pixels, trace): (usize, &mut [u8], Option<&mut [u16]>)| {
            let rows = u32::try_from(pixels.len() / width).unwrap_or_default();
            let top = i32::try_from(index * BAND).unwrap_or(i32::MAX);
            let mut surface = Surface::of_parts(pixels, trace, view.0, rows);
            pass.draw(scroll, &mut surface, top, view);
        };
        let pixels = frame.pixels_mut().par_chunks_mut(BAND * width);
        match provenance {
            Some(p) if (p.width(), p.height()) == view => pixels
                .zip(p.marks_mut().par_chunks_mut(BAND * width))
                .enumerate()
                .for_each(|(i, (pixels, trace))| draw((i, pixels, Some(trace)))),
            _ => pixels.enumerate().for_each(|(i, pixels)| draw((i, pixels, None))),
        }
    }

    fn wrap(&self, scroll: Point) -> Point {
        let (width, height) = self.terrain.world_size();
        let wrap = |v: i32, period: u32| v.rem_euclid(i32::try_from(period.max(1)).unwrap_or(i32::MAX));
        Point::new(wrap(scroll.x, width), wrap(scroll.y, height))
    }
}

#[cfg(test)]
mod tests;
