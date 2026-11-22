//! What the game draws for one frame: the world, and the interface in a layer of its own at its
//! own scale. The window presents the two through the GPU; a screenshot takes the same two.

use std::time::Duration;

use s2_assets::PaletteCycle;
use s2_render::terrain::{Point, TileCache};
use s2_render::{Clocks, Frame, Overlay, Patch, Provenance, Renderer, Zoom, overlay};
use s2_sim::{Node, Player};
use s2_ui::Rect;
use s2_ui::border::Border;

use super::build::Builder;
use super::camera::Camera;
use super::desktop::{Asks, Desktop};
use crate::scene::Scene;

/// How the world layer is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drawing {
    /// Terrain from cached tiles with the objects drawn over them, fast enough for every frame
    /// of a window. A tile cuts triangles in other places than the original's map view does
    /// (see [`TileCache`]), so a few pixels near the view's edges differ from its first frame.
    Cached,
    /// The map view redrawn whole, cut at its own edges as the original draws its first frame,
    /// with the layer each pixel came from traced.
    Full,
}

/// The frame's two layers, what the world is drawn from, and the cycling palette.
pub(crate) struct Picture {
    drawing: Drawing,
    cycle: PaletteCycle,
    cycle_steps: Option<u64>,
    tiles: TileCache,
    frame: Frame,
    /// The interface's own layer of palette indices, drawn at one pixel per original pixel.
    interface: Frame,
    /// How many output pixels one interface pixel covers.
    scale: u8,
    /// The scale the picture opened at, which Ctrl+0 goes back to.
    first_scale: u8,
    patches: Vec<Patch>,
    /// The part of the world layer the interface leaves the map view, or `None` for all of it.
    map_view: Option<(u32, u32, u32, u32)>,
    /// With [`Drawing::Full`], the layer each world pixel came from.
    provenance: Option<Provenance>,
    /// Whether the cheat's `F7` had the fog lifted on the last frame.
    fog_lifted: bool,
}

impl Picture {
    /// A picture of `scene` with the world drawn as `drawing` says and the interface at `scale`.
    pub(crate) fn new(scene: &Scene, drawing: Drawing, scale: u8) -> Self {
        Self {
            drawing,
            cycle: PaletteCycle::new(scene.palette.clone()),
            cycle_steps: None,
            tiles: TileCache::new(scene.view.terrain()),
            frame: Frame::new(1, 1, 0),
            interface: Frame::new(1, 1, 0),
            scale,
            first_scale: scale,
            patches: Vec::new(),
            map_view: None,
            provenance: None,
            fog_lifted: false,
        }
    }

    /// Follows the cheat's `F7`, which shows every node as seen (`0x1d509`, `0x1eed5`): the fog
    /// layer goes off when the option comes on and back on when it goes off. Only a change moves
    /// the layer, so the viewer's own fog key keeps working in between.
    fn lift_fog(&mut self, scene: &mut Scene, lifted: bool) {
        if lifted != self.fog_lifted {
            self.fog_lifted = lifted;
            let layers = scene.view.layers();
            let layers = if lifted {
                layers - s2_render::Layers::FOG
            } else {
                layers | s2_render::Layers::FOG
            };
            scene.view.set_layers(layers);
        }
    }

    /// How many output pixels one interface pixel covers.
    pub(crate) fn scale(&self) -> u8 {
        self.scale
    }

    /// The scale the picture opened at.
    pub(crate) fn first_scale(&self) -> u8 {
        self.first_scale
    }

    /// Sets the interface's scale to `scale`, held between 1 and [`overlay::LARGEST_SCALE`].
    pub(crate) fn set_scale(&mut self, scale: u8) {
        let scale = scale.clamp(1, overlay::LARGEST_SCALE);
        if scale != self.scale {
            self.scale = scale;
            log::info!("interface scale {scale}");
        }
    }

    /// Brings the palette up to date for `played` time of unpaused play, draws the frame as
    /// [`Picture::draw`] does and presents it.
    #[allow(clippy::too_many_arguments, reason = "one frame needs all of it")]
    pub(crate) fn show(
        &mut self,
        renderer: &mut Renderer,
        scene: &mut Scene,
        builder: &Builder,
        camera: &Camera,
        output: (u32, u32),
        (played, anim): (Duration, u32),
        paused: bool,
        desktop: Option<(&mut Desktop, Player)>,
    ) -> s2_render::Result<Asks> {
        let steps = PaletteCycle::steps_after(played);
        if self.cycle_steps != Some(steps) {
            renderer.set_palette(&self.cycle.at(steps));
            self.cycle_steps = Some(steps);
        }
        let asks = self.draw(scene, Some(builder), camera, output, anim, paused, desktop);
        renderer.render(&self.frame, camera.zoom(), self.layer())?;
        Ok(asks)
    }

    /// Draws what `camera` sees on an `output` window, with the animation counter at `anim`,
    /// the build controls drawn over it given a `builder` and, with a desktop, the interface in
    /// its layer. Gives back what the interface's windows asked for.
    ///
    /// A `paused` view goes through [`Scene::dim`] before the interface draws, which leaves the
    /// windows and the toolbar at their own brightness: the interface is a layer of its own and
    /// the dimming never reaches it.
    #[allow(clippy::too_many_arguments, reason = "one frame needs all of it")]
    pub(crate) fn draw(
        &mut self,
        scene: &mut Scene,
        builder: Option<&Builder>,
        camera: &Camera,
        output: (u32, u32),
        anim: u32,
        paused: bool,
        desktop: Option<(&mut Desktop, Player)>,
    ) -> Asks {
        if let Some((desktop, owner)) = &desktop {
            let aid = desktop.options() & s2_ui::keys::flag::SPACE != 0;
            scene.view.set_aid(aid.then_some(*owner));
            self.lift_fog(scene, desktop.options() & s2_ui::keys::flag::F7 != 0);
        }
        let menu = desktop.as_ref().and_then(|(desktop, _)| desktop.menu_node());
        self.map_view = desktop
            .is_some()
            .then(|| map_view(output, self.scale, camera.zoom()));
        self.compose(
            scene,
            builder.map(|builder| (builder, menu)),
            camera,
            output,
            anim,
        );
        let period = scene.view.terrain().world_size();
        let mut desktop = desktop;
        if let Some((desktop, owner)) = &mut desktop {
            let at = (camera.position(), period);
            desktop.label_buildings(&scene.game, scene.view.terrain(), *owner, at, &mut self.frame);
        }
        if paused {
            self.frame.remap(&scene.dim);
        }
        self.patches.clear();
        let Some((desktop, owner)) = desktop else {
            return Asks::default();
        };
        let clocks = Clocks {
            anim,
            step: scene.game.clock().step(),
        };
        let centre = super::middle(camera.position(), (self.frame.width(), self.frame.height()));
        desktop.set_view_centre(scene.view.terrain().node_at(centre));
        super::observe(desktop, &scene.game, &scene.view, clocks);
        let size = overlay::size(output, self.scale);
        if (self.interface.width(), self.interface.height()) == size {
            self.interface.clear(0);
        } else {
            self.interface = Frame::new(size.0, size.1, 0);
        }
        let asks = desktop.run(&scene.game, owner, scene.view.terrain(), &mut self.interface);
        self.patches.extend(desktop.patches());
        asks
    }

    /// The world layer as the last frame left it.
    pub(crate) fn world(&self) -> &Frame {
        &self.frame
    }

    /// The interface layer as the last frame left it, or `None` when it holds nothing.
    pub(crate) fn layer(&self) -> Option<Overlay<'_>> {
        (!self.patches.is_empty()).then(|| Overlay {
            frame: &self.interface,
            scale: self.scale,
            patches: &self.patches,
        })
    }

    /// With [`Drawing::Full`], the layer each pixel of the last frame's world came from.
    pub(crate) fn provenance(&self) -> Option<&Provenance> {
        self.provenance.as_ref()
    }

    /// The last frame with the interface drawn over the world, as the window shows it at zoom 1.
    pub(crate) fn flattened(&self) -> Frame {
        let mut frame = self.frame.clone();
        if let Some(layer) = self.layer() {
            layer.draw_onto(&mut frame);
        }
        frame
    }

    /// Composes what `camera` sees on an `output` window into the world layer, with the
    /// animation counter at `anim` and, given a builder and the node of the open action menu,
    /// the build controls' marks over the objects.
    pub(crate) fn compose(
        &mut self,
        scene: &mut Scene,
        builder: Option<(&Builder, Option<Node>)>,
        camera: &Camera,
        output: (u32, u32),
        anim: u32,
    ) -> &Frame {
        let marks = builder.map(|(builder, menu)| builder.marks(&scene.game, menu));
        scene.view.set_marks(marks.unwrap_or_default());
        let size = camera.zoom().world_size(output.0, output.1);
        if (self.frame.width(), self.frame.height()) != size {
            self.frame = Frame::new(size.0, size.1, 0);
        }
        let clocks = Clocks {
            anim,
            step: scene.game.clock().step(),
        };
        let at = camera.position();
        match self.drawing {
            Drawing::Cached => {
                scene.view.sync(scene.game.nodes(), &mut self.tiles);
                scene
                    .view
                    .compose(scene.game.world(), &mut self.tiles, at, clocks, &mut self.frame);
            }
            Drawing::Full => {
                let (x, y, width, height) = self.map_view.unwrap_or((0, 0, size.0, size.1));
                let mut inner = Frame::new(width, height, 0);
                let mut traced = Provenance::new(width, height);
                let from = Point::new(at.x + x.cast_signed(), at.y + y.cast_signed());
                scene
                    .view
                    .draw(scene.game.world(), from, clocks, &mut inner, Some(&mut traced));
                self.frame.clear(0);
                self.frame.paste(&inner, x, y);
                let mut provenance = Provenance::new(size.0, size.1);
                provenance.paste(&traced, x, y);
                self.provenance = Some(provenance);
            }
        }
        &self.frame
    }
}

/// The map view of an `output` window with the interface at `scale`, in pixels of the world
/// layer at `zoom`: the part of the desktop inside its border, widened to whole world pixels.
fn map_view(output: (u32, u32), scale: u8, zoom: Zoom) -> (u32, u32, u32, u32) {
    let (across, down) = overlay::size(output, scale);
    let world = zoom.world_size(output.0, output.1);
    let inner = Border::inner(Rect::new(0, 0, across, down));
    let quarters = u32::from(zoom.quarters());
    let step = u32::from(scale.max(1)) * 4;
    let start = |desktop: i32| (desktop.unsigned_abs() * step) / quarters;
    let end = |desktop: i32, limit: u32| (desktop.unsigned_abs() * step).div_ceil(quarters).min(limit);
    let (left, top) = (start(inner.x), start(inner.y));
    let (right, bottom) = (end(inner.right(), world.0), end(inner.bottom(), world.1));
    (left, top, right.saturating_sub(left), bottom.saturating_sub(top))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_screen_shows_the_world_inside_a_twelve_pixel_border() {
        assert_eq!(map_view((640, 480), 1, Zoom::ONE), (12, 12, 616, 456));
    }

    #[test]
    fn the_map_view_follows_the_scale_and_the_zoom() {
        assert_eq!(map_view((1280, 960), 2, Zoom::ONE), (24, 24, 1232, 912));
        let two = Zoom::from_quarters(8).unwrap();
        assert_eq!(map_view((1280, 960), 2, two), (12, 12, 616, 456));
    }
}
