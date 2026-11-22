//! The interface layer: a frame of its own drawn over the view at its own whole scale.
//!
//! The view's zoom and the interface's scale are separate numbers. A window is drawn at one
//! pixel per original pixel into a layer of [`Overlay::size`] and scaled up by whole steps, so
//! the numbers in the window modules stay the numbers `S2.EXE` uses however far the view is
//! zoomed.

use crate::Frame;
use crate::scale::Scissor;

/// The scale the interface draws at unless a setting asks for another.
pub const DEFAULT_SCALE: u8 = 2;

/// The largest scale the interface draws at.
pub const LARGEST_SCALE: u8 = 8;

/// A rectangle of the interface layer that holds something to draw, in layer pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patch {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width in layer pixels.
    pub width: u32,
    /// Height in layer pixels.
    pub height: u32,
}

impl Patch {
    /// The patch as a part of an `output` of this many pixels, with the layer at `scale`. Gives
    /// `None` for a patch that lands wholly outside the output.
    pub(crate) fn on_output(self, scale: u8, output: (u32, u32)) -> Option<Scissor> {
        let step = i64::from(scale);
        let left = i64::from(self.x) * step;
        let top = i64::from(self.y) * step;
        let right = left + i64::from(self.width) * step;
        let bottom = top + i64::from(self.height) * step;
        let clamp = |value: i64, limit: u32| u32::try_from(value.clamp(0, i64::from(limit))).unwrap_or(limit);
        let (x, y) = (clamp(left, output.0), clamp(top, output.1));
        let (width, height) = (clamp(right, output.0) - x, clamp(bottom, output.1) - y);
        (width > 0 && height > 0).then_some(Scissor { x, y, width, height })
    }
}

/// An interface layer to draw over the view.
#[derive(Debug, Clone, Copy)]
pub struct Overlay<'a> {
    /// The layer's palette indices, of [`Overlay::size`] for the output it goes on.
    pub frame: &'a Frame,
    /// How many output pixels one layer pixel covers.
    pub scale: u8,
    /// The parts of the layer that hold something. Everything else leaves the view as it is.
    pub patches: &'a [Patch],
}

impl Overlay<'_> {
    /// Draws the layer over `view` as the renderer does when the view is at zoom 1 and as large
    /// as the output: inside each patch, every output pixel takes the layer pixel it falls on.
    pub fn draw_onto(&self, view: &mut Frame) {
        let step = u32::from(self.scale.max(1));
        let (across, down) = (self.frame.width(), self.frame.height());
        let pixels = self.frame.pixels();
        let output = (view.width(), view.height());
        let out = view.pixels_mut();
        for patch in self.patches {
            let Some(cut) = patch.on_output(self.scale.max(1), output) else {
                continue;
            };
            for y in cut.y..cut.y + cut.height {
                let row = (y / step).min(down.saturating_sub(1)) * across;
                for x in cut.x..cut.x + cut.width {
                    let column = (x / step).min(across.saturating_sub(1));
                    out[(y * output.0 + x) as usize] = pixels[(row + column) as usize];
                }
            }
        }
    }
}

/// The layer size for an `output` of this many pixels at `scale`, rounded up so the layer covers
/// the output and overhangs it rather than leaving a border.
#[must_use]
pub fn size(output: (u32, u32), scale: u8) -> (u32, u32) {
    let step = u32::from(scale.max(1));
    (output.0.div_ceil(step).max(1), output.1.div_ceil(step).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_at_scale_one_gets_a_layer_of_its_own_size() {
        assert_eq!(size((320, 200), 1), (320, 200));
    }

    #[test]
    fn a_layer_that_does_not_divide_evenly_rounds_up() {
        assert_eq!(size((1920, 1080), 2), (960, 540));
        assert_eq!(size((1921, 1081), 2), (961, 541));
    }

    #[test]
    fn a_patch_scales_and_clips_to_the_output() {
        let patch = Patch {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        };
        assert_eq!(
            patch.on_output(2, (1000, 1000)),
            Some(Scissor {
                x: 20,
                y: 40,
                width: 60,
                height: 80,
            })
        );
        assert_eq!(
            patch.on_output(2, (50, 1000)),
            Some(Scissor {
                x: 20,
                y: 40,
                width: 30,
                height: 80,
            })
        );
    }

    #[test]
    fn a_patch_off_the_output_draws_nothing() {
        let off = Patch {
            x: -40,
            y: 0,
            width: 20,
            height: 20,
        };
        assert_eq!(off.on_output(2, (100, 100)), None);
    }

    #[test]
    fn a_layer_drawn_onto_a_view_replaces_only_its_patches() {
        let mut layer = Frame::new(4, 3, 0);
        layer.set(1, 1, 7);
        layer.set(2, 1, 8);
        let patches = [Patch {
            x: 1,
            y: 1,
            width: 2,
            height: 1,
        }];
        let overlay = Overlay {
            frame: &layer,
            scale: 2,
            patches: &patches,
        };
        let mut view = Frame::new(8, 6, 1);
        overlay.draw_onto(&mut view);
        let row = |y: usize| &view.pixels()[y * 8..][..8];
        assert_eq!(row(1), [1; 8]);
        assert_eq!(row(2), [1, 1, 7, 7, 8, 8, 1, 1]);
        assert_eq!(row(3), [1, 1, 7, 7, 8, 8, 1, 1]);
        assert_eq!(row(4), [1; 8]);
    }

    #[test]
    fn a_patch_over_the_left_edge_keeps_the_part_that_shows() {
        let over = Patch {
            x: -10,
            y: -10,
            width: 30,
            height: 30,
        };
        assert_eq!(
            over.on_output(3, (100, 100)),
            Some(Scissor {
                x: 0,
                y: 0,
                width: 60,
                height: 60,
            })
        );
    }
}
