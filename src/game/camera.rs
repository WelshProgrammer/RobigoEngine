//! Where the viewer looks: a world position that wraps with the map, and a zoom.

use s2_render::Zoom;
use s2_render::terrain::Point;

/// The zoom steps the viewer moves through, in quarters: 0.5 to 4.
const STEPS: [u8; 7] = [2, 3, 4, 6, 8, 12, 16];

/// The view's top-left corner in world pixels, kept fractional so slow scrolling and drags
/// accumulate, and wrapped onto one copy of the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Camera {
    x: f64,
    y: f64,
    zoom: Zoom,
    period: (f64, f64),
}

impl Camera {
    /// A camera at the world origin for a map repeating every `period` world pixels.
    pub(crate) fn new(period: (u32, u32), zoom: Zoom) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom,
            period: (f64::from(period.0.max(1)), f64::from(period.1.max(1))),
        }
    }

    /// Puts world pixel (`x`, `y`) at the output's top-left corner.
    pub(crate) fn place(&mut self, x: i32, y: i32) {
        self.x = f64::from(x);
        self.y = f64::from(y);
        self.wrap();
    }

    /// Moves the view by (`dx`, `dy`) output pixels.
    pub(crate) fn scroll(&mut self, dx: f64, dy: f64) {
        let factor = f64::from(self.zoom.factor());
        self.x += dx / factor;
        self.y += dy / factor;
        self.wrap();
    }

    /// Changes the zoom, keeping the world point under output pixel `anchor` where it is.
    pub(crate) fn zoom_to(&mut self, zoom: Zoom, anchor: (f64, f64)) {
        let (before, after) = (f64::from(self.zoom.factor()), f64::from(zoom.factor()));
        self.x += anchor.0 / before - anchor.0 / after;
        self.y += anchor.1 / before - anchor.1 / after;
        self.zoom = zoom;
        self.wrap();
    }

    /// Follows the output going from `before` to `after` pixels, keeping the world point at its
    /// centre where it is.
    pub(crate) fn resize(&mut self, before: (u32, u32), after: (u32, u32)) {
        let factor = f64::from(self.zoom.factor());
        let shift = |from: u32, to: u32| (f64::from(from) - f64::from(to)) / 2.0 / factor;
        self.x += shift(before.0, after.0);
        self.y += shift(before.1, after.1);
        self.wrap();
    }

    /// The current zoom.
    pub(crate) fn zoom(&self) -> Zoom {
        self.zoom
    }

    /// The scroll position to draw: the top-left corner rounded down to a whole world pixel.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the position is wrapped into one map period, far inside i32"
    )]
    pub(crate) fn position(&self) -> Point {
        Point::new(self.x.floor() as i32, self.y.floor() as i32)
    }

    /// The world pixel under output pixel `at`, as the frame drawn at [`Self::position`] puts it
    /// there. Not wrapped.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "output pixels over the zoom factor stay far inside i32"
    )]
    pub(crate) fn world_point(&self, at: (f64, f64)) -> Point {
        let factor = f64::from(self.zoom.factor());
        let position = self.position();
        Point::new(
            position.x + (at.0 / factor).floor() as i32,
            position.y + (at.1 / factor).floor() as i32,
        )
    }

    fn wrap(&mut self) {
        self.x = self.x.rem_euclid(self.period.0);
        self.y = self.y.rem_euclid(self.period.1);
    }
}

/// The next zoom step in, or `None` at the closest.
pub(crate) fn zoom_in(zoom: Zoom) -> Option<Zoom> {
    STEPS
        .iter()
        .find(|&&q| q > zoom.quarters())
        .and_then(|&q| Zoom::from_quarters(q))
}

/// The next zoom step out, or `None` at the farthest.
pub(crate) fn zoom_out(zoom: Zoom) -> Option<Zoom> {
    STEPS
        .iter()
        .rev()
        .find(|&&q| q < zoom.quarters())
        .and_then(|&q| Zoom::from_quarters(q))
}

/// The largest whole zoom at which a 640x480 world area still fills an output of `size`.
pub(crate) fn fitting_zoom(size: (u32, u32)) -> Zoom {
    let whole = (size.0 / 640).min(size.1 / 480).clamp(1, 4);
    Zoom::from_quarters(u8::try_from(whole * 4).unwrap_or(4)).unwrap_or(Zoom::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom(q: u8) -> Zoom {
        Zoom::from_quarters(q).unwrap()
    }

    #[test]
    fn output_pixels_map_to_world_pixels_through_the_zoom() {
        let mut camera = Camera::new((1000, 1000), zoom(8));
        camera.place(100, 40);
        let point = camera.world_point((11.0, 4.0));
        assert_eq!((point.x, point.y), (105, 42));
    }

    #[test]
    fn position_wraps_with_the_map() {
        let mut camera = Camera::new((100, 50), Zoom::ONE);
        camera.place(-10, 120);
        assert_eq!((camera.position().x, camera.position().y), (90, 20));
        camera.scroll(15.0, -25.0);
        assert_eq!((camera.position().x, camera.position().y), (5, 45));
    }

    #[test]
    fn scrolling_is_in_output_pixels() {
        let mut camera = Camera::new((1000, 1000), zoom(8));
        camera.scroll(10.0, 3.0);
        assert_eq!((camera.position().x, camera.position().y), (5, 1));
        camera.scroll(1.0, 0.0);
        assert_eq!(camera.position().x, 5, "half a world pixel accumulates");
        camera.scroll(1.0, 0.0);
        assert_eq!(camera.position().x, 6);
    }

    #[test]
    fn zooming_keeps_the_anchor_still() {
        let mut camera = Camera::new((10_000, 10_000), Zoom::ONE);
        camera.place(100, 100);
        camera.zoom_to(zoom(8), (200.0, 100.0));
        assert_eq!((camera.position().x, camera.position().y), (200, 150));
        assert_eq!(camera.zoom(), zoom(8));
    }

    #[test]
    fn resizing_keeps_the_centre_still() {
        let mut camera = Camera::new((10_000, 10_000), zoom(8));
        camera.place(1000, 1000);
        let centre = |camera: &Camera, (w, h): (u32, u32)| {
            camera.world_point((f64::from(w) / 2.0, f64::from(h) / 2.0))
        };
        let before = centre(&camera, (1280, 960));
        camera.resize((1280, 960), (1920, 1080));
        assert_eq!(centre(&camera, (1920, 1080)), before);
        camera.resize((1920, 1080), (640, 480));
        assert_eq!(centre(&camera, (640, 480)), before);
    }

    #[test]
    fn steps_and_fitting() {
        assert_eq!(zoom_in(Zoom::ONE), Some(zoom(6)));
        assert_eq!(zoom_out(Zoom::ONE), Some(zoom(3)));
        assert_eq!(zoom_in(zoom(16)), None);
        assert_eq!(zoom_out(zoom(2)), None);
        assert_eq!(zoom_in(zoom(5)), Some(zoom(6)));
        assert_eq!(fitting_zoom((1280, 960)), zoom(8));
        assert_eq!(fitting_zoom((1000, 700)), Zoom::ONE);
        assert_eq!(fitting_zoom((3840, 2160)), zoom(16));
    }
}
