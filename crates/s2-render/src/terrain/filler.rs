//! The textured, Gouraud-shaded polygon filler, matching the output of `S2.EXE`'s filler at
//! `0xd0000` pixel for pixel.
//!
//! Small pure functions over plain integers decide which pixels are written and with which
//! index, one per stage: [`clip_point`] and [`clip_edge`] (1/16-pixel clipping), [`edge_setup`]
//! (per-edge steps), [`Packed::step`] (one row down an edge, carry leaks included), [`span_setup`]
//! (per-span steps) and [`SpanCursor::step`] (one pixel). [`fill`] is a thin loop over them.
//! This module reproduces the original's results with its own layout: rows are read straight off
//! each edge's walk, with no scan-line table.

use s2_assets::Remap;
pub use s2_assets::TEXTURE_SIZE;
use s2_formats::gouraud::ShadingTable;

use super::units::{EdgeEntry, Fixed8, Fixed16, Packed, Subpixel, low8, low16, low32};
use crate::Layers;

/// Most corners a polygon passed to [`fill`] may have.
pub const MAX_CORNERS: usize = 4;

/// Clipping can add one corner per boundary crossed; each edge emits at most two.
const MAX_CLIPPED: usize = MAX_CORNERS * 4;

/// Bits 8 to 15 of a span's packed u step, where the v integer step goes.
const V_STEP_BITS: u32 = ((1 << 8) - 1) << 8;

/// A polygon corner: position in 1/16 pixels, texture position and shading row in 8.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Vertex {
    /// Column, 1/16 pixels.
    pub x: Subpixel,
    /// Row, 1/16 pixels.
    pub y: Subpixel,
    /// Texture column.
    pub u: Fixed8,
    /// Texture row.
    pub v: Fixed8,
    /// Shading-table row.
    pub light: Fixed8,
}

/// Which coordinate a clipping pass works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// Left and right boundaries.
    X,
    /// Top and bottom boundaries.
    Y,
}

impl Axis {
    const fn of(self, vertex: Vertex) -> Subpixel {
        match self {
            Self::X => vertex.x,
            Self::Y => vertex.y,
        }
    }
}

/// One axis of the clip window. A coordinate is inside when strictly between the bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bounds {
    /// The low boundary.
    pub low: Subpixel,
    /// The high boundary.
    pub high: Subpixel,
}

impl Bounds {
    /// The bounds for a surface `pixels` wide or tall: 0 and `16 · pixels`.
    #[must_use]
    pub const fn for_extent(pixels: u32) -> Self {
        Self {
            low: Subpixel::from_raw(0),
            high: Subpixel::from_pixels(pixels.cast_signed()),
        }
    }
}

/// The point where the edge from `inside` to `outside` meets `bound` on `axis`.
///
/// The clipped coordinate becomes `bound` exactly; every other attribute `q` becomes
/// `q_in + (q_out − q_in) · (bound − c_in) / (c_out − c_in)` with a 64-bit product and a
/// truncating division.
#[must_use]
pub fn clip_point(inside: Vertex, outside: Vertex, axis: Axis, bound: Subpixel) -> Vertex {
    let c_in = axis.of(inside).raw();
    let num = i64::from(bound.raw().wrapping_sub(c_in));
    let den = i64::from(axis.of(outside).raw().wrapping_sub(c_in));
    let lerp = |a: i32, b: i32| low32(i64::from(b.wrapping_sub(a)) * num / den).wrapping_add(a);
    let mut out = Vertex {
        x: Subpixel::from_raw(lerp(inside.x.raw(), outside.x.raw())),
        y: Subpixel::from_raw(lerp(inside.y.raw(), outside.y.raw())),
        u: Fixed8::from_raw(lerp(inside.u.raw(), outside.u.raw())),
        v: Fixed8::from_raw(lerp(inside.v.raw(), outside.v.raw())),
        light: Fixed8::from_raw(lerp(inside.light.raw(), outside.light.raw())),
    };
    match axis {
        Axis::X => out.x = bound,
        Axis::Y => out.y = bound,
    }
    out
}

/// What the edge from `a` to `b` contributes to a clipping pass on `axis`: `a` (moved onto the
/// boundary when outside), then the point where the edge leaves the window when it does. Nothing
/// when both ends are beyond the same boundary. The low boundary is tested first, and a moved
/// `a` is what the high boundary sees.
#[must_use]
pub fn clip_edge(a: Vertex, b: Vertex, axis: Axis, bounds: Bounds) -> [Option<Vertex>; 2] {
    let mut a = a;
    let mut leaving = None;
    let cb = axis.of(b);
    if cb > bounds.low {
        if axis.of(a) <= bounds.low {
            a = clip_point(b, a, axis, bounds.low);
        }
    } else if axis.of(a) <= bounds.low {
        return [None, None];
    } else {
        leaving = Some(clip_point(a, b, axis, bounds.low));
    }
    if cb < bounds.high {
        if axis.of(a) >= bounds.high {
            a = clip_point(b, a, axis, bounds.high);
        }
    } else if axis.of(a) >= bounds.high {
        return [None, None];
    } else {
        leaving = Some(clip_point(a, b, axis, bounds.high));
    }
    [Some(a), leaving]
}

/// How one edge is walked: from row `first_row`, `rows` rows down, starting at `start` and
/// adding `step` per row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeWalk {
    /// The first row the edge fills.
    pub first_row: i32,
    /// How many rows it fills.
    pub rows: i32,
    /// Accumulators at the first row.
    pub start: Packed,
    /// Per-row step.
    pub step: Packed,
}

/// The per-row step for u, v or light over `rows` rows: `((q_b − q_a) << 15) / rows`, rotated
/// left by one bit, so the sign bit lands in the lowest fraction bit.
#[must_use]
pub fn attribute_step(a: Fixed8, b: Fixed8, rows: i32) -> Fixed16 {
    let t = low32(i64::from(b.raw().wrapping_sub(a.raw()).wrapping_shl(15)) / i64::from(rows));
    Fixed16::from_raw(t.rotate_left(1))
}

/// Sets up the walk down the edge from `top` to `bottom`, or `None` when the edge starts and
/// ends in the same row (or `bottom` is not lower).
#[must_use]
pub fn edge_setup(top: Vertex, bottom: Vertex) -> Option<EdgeWalk> {
    let rows = bottom.y.pixel() - top.y.pixel();
    if rows <= 0 {
        return None;
    }
    let dy = i64::from(bottom.y.raw().wrapping_sub(top.y.raw()));
    let dx = i64::from(bottom.x.raw().wrapping_sub(top.x.raw()).wrapping_shl(16));
    let x = Fixed16::from_raw(low32(dx / dy));
    Some(EdgeWalk {
        first_row: top.y.pixel(),
        rows,
        start: Packed::start(top.x, top.u, top.v, top.light),
        step: Packed::steps(
            x,
            attribute_step(top.u, bottom.u, rows),
            attribute_step(top.v, bottom.v, rows),
            attribute_step(top.light, bottom.light, rows),
        ),
    })
}

/// Where a span's texture, texel row and shading row stand at one pixel.
///
/// The u integer advances with the carry out of a 32-bit register. Its top 16 bits are the u
/// fraction. Its low 16 bits start as junk (the u integer again) and gather the v and u integer
/// steps, and their overflow still runs into the fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanCursor {
    /// Texture column.
    pub u: u8,
    /// The register carrying the u fraction.
    pub u_acc: u32,
    /// Texture row.
    pub v: u8,
    /// The v fraction.
    pub v_frac: u16,
    /// Shading-table row.
    pub light: u8,
    /// The light fraction.
    pub light_frac: u16,
}

/// What [`SpanCursor::step`] adds per pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanStep {
    /// Added to the texture column, with the register's carry.
    pub u: u8,
    /// Added to the u register.
    pub u_acc: u32,
    /// Added to the texture row, with the fraction's carry.
    pub v: u8,
    /// Added to the v fraction.
    pub v_frac: u16,
    /// Added to the shading row, with the fraction's carry.
    pub light: u8,
    /// Added to the light fraction.
    pub light_frac: u16,
}

impl SpanCursor {
    /// The texel this pixel samples: `(v << 8) | u`.
    #[must_use]
    pub fn texel(self) -> usize {
        usize::from(u16::from_le_bytes([self.u, self.v]))
    }

    /// One pixel to the left.
    #[must_use]
    pub fn step(self, step: SpanStep) -> Self {
        let (u_acc, u_carry) = self.u_acc.overflowing_add(step.u_acc);
        let (v_frac, v_carry) = self.v_frac.overflowing_add(step.v_frac);
        let (light_frac, light_carry) = self.light_frac.overflowing_add(step.light_frac);
        Self {
            u: self.u.wrapping_add(step.u).wrapping_add(u8::from(u_carry)),
            u_acc,
            v: self.v.wrapping_add(step.v).wrapping_add(u8::from(v_carry)),
            v_frac,
            light: self
                .light
                .wrapping_add(step.light)
                .wrapping_add(u8::from(light_carry)),
            light_frac,
        }
    }
}

/// One row's span: `pixels` pixels written leftwards from column `right − 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// One past the first (rightmost) pixel written.
    pub right: i32,
    /// Pixels written, at least one.
    pub pixels: u32,
    /// State at the rightmost pixel.
    pub cursor: SpanCursor,
    /// Per-pixel step.
    pub step: SpanStep,
}

/// Sets up the span between a row's `right` and `left` edge entries, or `None` when the right
/// edge is not right of the left one. With a colour key the pixel count is taken modulo 256, as
/// the original's keyed loop does; a count of zero still draws one pixel.
#[must_use]
pub fn span_setup(right: EdgeEntry, left: EdgeEntry, keyed: bool) -> Option<Span> {
    let width = i32::from(right.x) - i32::from(left.x);
    if width <= 0 {
        return None;
    }
    let step = |l: u16, r: u16| {
        let delta = (i32::from(l) - i32::from(r)).wrapping_shl(8);
        Fixed16::from_raw(low32(i64::from(delta) / i64::from(width)))
    };
    let du = step(left.u, right.u);
    let dv = step(left.v, right.v);
    let dl = step(left.light, right.light);
    let count = if keyed {
        u32::from(low8(width.cast_unsigned()))
    } else {
        u32::from(low16(width.cast_unsigned()))
    };
    let pixels = if count == 0 { 1 } else { count };
    let [u_low, u_high] = right.u.to_le_bytes();
    let [v_low, v_high] = right.v.to_le_bytes();
    let [light_low, light_high] = right.light.to_le_bytes();
    Some(Span {
        right: i32::from(right.x),
        pixels,
        cursor: SpanCursor {
            u: u_high,
            u_acc: (u32::from(u_low) << 24) | u32::from(u_high),
            v: v_high,
            v_frac: u16::from(v_low) << 8,
            light: light_high,
            light_frac: u16::from(light_low) << 8,
        },
        step: SpanStep {
            u: du.int8(),
            // Top half: the u step's fraction; bits 8–15: the v integer step; bits 0–7: the u one.
            u_acc: (du.raw().cast_unsigned().rotate_left(16) & !V_STEP_BITS) | (u32::from(dv.int8()) << 8),
            v: dv.int8(),
            v_frac: low16(dv.low()),
            light: dl.int8(),
            light_frac: low16(dl.low()),
        },
    })
}

/// One row of a [`Surface`]: its pixels, its trace if the surface has one, and the current pen.
pub(crate) type Line<'s> = (&'s mut [u8], Option<&'s mut [u16]>, u16);

/// An indexed surface to draw into, optionally recording which [`Layers`] wrote each pixel.
/// It is either a whole buffer, row by row with no padding, or a [`window`](Self::window) on
/// part of one.
#[derive(Debug)]
pub struct Surface<'a> {
    pixels: &'a mut [u8],
    trace: Option<&'a mut [u16]>,
    pen: u16,
    width: u32,
    height: u32,
    stride: usize,
    origin: usize,
}

impl<'a> Surface<'a> {
    /// Wraps `pixels` as `width` × `height`; `None` if the lengths disagree.
    #[must_use]
    pub fn new(pixels: &'a mut [u8], width: u32, height: u32) -> Option<Self> {
        (pixels.len() as u64 == u64::from(width) * u64::from(height)).then_some(Self {
            pixels,
            trace: None,
            pen: 0,
            width,
            height,
            stride: width as usize,
            origin: 0,
        })
    }

    /// A buffer whose length is known to match its size, with an optional trace of the same
    /// length.
    pub(crate) fn of_parts(
        pixels: &'a mut [u8],
        trace: Option<&'a mut [u16]>,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            pixels,
            trace,
            pen: 0,
            width,
            height,
            stride: width as usize,
            origin: 0,
        }
    }

    /// The `width` × `height` part of this surface whose top-left corner is (`left`, `top`),
    /// as a surface of its own that clips to that rectangle. The rectangle is cut down to fit.
    pub fn window(&mut self, left: u32, top: u32, width: u32, height: u32) -> Surface<'_> {
        let left = left.min(self.width);
        let top = top.min(self.height);
        Surface {
            pixels: &mut *self.pixels,
            trace: self.trace.as_deref_mut(),
            pen: self.pen,
            width: width.min(self.width - left),
            height: height.min(self.height - top),
            stride: self.stride,
            origin: self.origin + top as usize * self.stride + left as usize,
        }
    }

    /// Records, from now on, that pixels are written by `layers`. Only matters on a surface
    /// with a trace.
    pub fn set_pen(&mut self, layers: Layers) {
        self.pen = layers.bits();
    }

    /// Whether the surface records provenance.
    #[must_use]
    pub fn traced(&self) -> bool {
        self.trace.is_some()
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Sends every pixel through `table`, leaving the trace as it is.
    pub fn remap(&mut self, table: &Remap) {
        for y in 0..self.height {
            if let Some((pixels, _, _)) = self.line(y.cast_signed()) {
                for pixel in pixels {
                    *pixel = table.get(*pixel);
                }
            }
        }
    }

    /// Sets every pixel to `index`, and records that nothing has written them yet.
    pub fn clear(&mut self, index: u8) {
        for y in 0..self.height {
            if let Some((pixels, trace, _)) = self.line(y.cast_signed()) {
                pixels.fill(index);
                if let Some(trace) = trace {
                    trace.fill(0);
                }
            }
        }
    }

    /// Writes `index` at (`x`, `y`), leaving coordinates outside the surface alone.
    pub fn pixel(&mut self, x: i32, y: i32, index: u8) {
        let Ok(x) = usize::try_from(x) else {
            return;
        };
        if x >= self.width as usize {
            return;
        }
        if let Some((pixels, trace, pen)) = self.line(y) {
            pixels[x] = index;
            if let Some(trace) = trace {
                trace[x] = pen;
            }
        }
    }

    pub(crate) fn row_mut(&mut self, y: u32) -> Option<&mut [u8]> {
        self.row(i32::try_from(y).ok()?)
    }

    fn row(&mut self, y: i32) -> Option<&mut [u8]> {
        self.line(y).map(|(pixels, _, _)| pixels)
    }

    /// Row `y`'s pixels, its trace if any, and the pen to mark them with.
    pub(crate) fn line(&mut self, y: i32) -> Option<Line<'_>> {
        let y = usize::try_from(y).ok().filter(|&y| y < self.height as usize)?;
        let start = self.origin + y * self.stride;
        let range = start..start + self.width as usize;
        let pixels = self.pixels.get_mut(range.clone())?;
        let trace = match &mut self.trace {
            Some(trace) => Some(trace.get_mut(range)?),
            None => None,
        };
        Some((pixels, trace, self.pen))
    }
}

/// What a polygon is painted with: a texture sheet and the shading table its texels go through.
#[derive(Debug, Clone, Copy)]
pub struct Material<'a> {
    /// 256 × 256 texels, row-major.
    pub texture: &'a [u8; TEXTURE_SIZE],
    /// Row = shading level, column = texel.
    pub shading: &'a ShadingTable,
}

/// A clipped polygon: at most [`MAX_CLIPPED`] corners.
#[derive(Debug, Clone, Copy)]
struct Polygon {
    corners: [Vertex; MAX_CLIPPED],
    len: usize,
}

impl Polygon {
    fn new(corners: &[Vertex]) -> Option<Self> {
        let mut polygon = Self {
            corners: [Vertex::default(); MAX_CLIPPED],
            len: 0,
        };
        if corners.len() > MAX_CORNERS {
            return None;
        }
        polygon.corners[..corners.len()].copy_from_slice(corners);
        polygon.len = corners.len();
        Some(polygon)
    }

    fn corners(&self) -> &[Vertex] {
        &self.corners[..self.len]
    }

    fn edges(&self) -> impl Iterator<Item = (Vertex, Vertex)> + '_ {
        let corners = self.corners();
        corners
            .iter()
            .zip(corners.iter().cycle().skip(1))
            .map(|(&a, &b)| (a, b))
    }

    fn clip(&self, axis: Axis, bounds: Bounds) -> Self {
        let mut out = Self {
            corners: [Vertex::default(); MAX_CLIPPED],
            len: 0,
        };
        for (a, b) in self.edges() {
            for vertex in clip_edge(a, b, axis, bounds).into_iter().flatten() {
                if let Some(slot) = out.corners.get_mut(out.len) {
                    *slot = vertex;
                    out.len += 1;
                }
            }
        }
        out
    }
}

/// Which side of the span an edge bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Right,
    Left,
}

/// An edge being walked: its side, its walk and where it has got to.
#[derive(Debug, Clone, Copy)]
struct ActiveEdge {
    side: Side,
    walk: EdgeWalk,
    at: Packed,
}

/// Fills `corners` (clockwise on screen, at most [`MAX_CORNERS`]) into `surface` with
/// `material`. With a `key`, texels equal to it leave the surface unchanged.
///
/// The polygon is clipped to the surface. Edges running down bound each row on the right and
/// edges running up on the left, and each row's pixels are written from the right edge leftwards.
/// A polygon wound the other way draws nothing.
pub fn fill(surface: &mut Surface<'_>, material: Material<'_>, corners: &[Vertex], key: Option<u8>) {
    let Some(polygon) = Polygon::new(corners) else {
        return;
    };
    let polygon = polygon
        .clip(Axis::X, Bounds::for_extent(surface.width))
        .clip(Axis::Y, Bounds::for_extent(surface.height));
    if polygon.len < 2 {
        return;
    }
    let mut edges = [None; MAX_CLIPPED];
    let mut top = Subpixel::from_raw(i32::MAX - 7);
    let mut bottom = Subpixel::from_raw(0);
    for (slot, (a, b)) in edges.iter_mut().zip(polygon.edges()) {
        let (side, from, to) = match b.y.cmp(&a.y) {
            std::cmp::Ordering::Greater => (Side::Right, a, b),
            std::cmp::Ordering::Less => (Side::Left, b, a),
            std::cmp::Ordering::Equal => continue,
        };
        let Some(walk) = edge_setup(from, to) else {
            continue;
        };
        top = top.min(from.y);
        bottom = bottom.max(to.y);
        *slot = Some(ActiveEdge {
            side,
            walk,
            at: walk.start,
        });
    }
    for row in top.pixel()..bottom.pixel() {
        let mut right = None;
        let mut left = None;
        for edge in edges.iter_mut().flatten() {
            let first = edge.walk.first_row;
            if row < first || row >= first + edge.walk.rows {
                continue;
            }
            let entry = edge.at.entry();
            match edge.side {
                Side::Right => right = Some(entry),
                Side::Left => left = Some(entry),
            }
            edge.at = edge.at.step(edge.walk.step);
        }
        let (Some(right), Some(left)) = (right, left) else {
            continue;
        };
        if let Some(span) = span_setup(right, left, key.is_some()) {
            draw_span(surface, material, row, span, key);
        }
    }
}

fn draw_span(surface: &mut Surface<'_>, material: Material<'_>, row: i32, span: Span, key: Option<u8>) {
    let Some((line, mut trace, pen)) = surface.line(row) else {
        return;
    };
    let mut cursor = span.cursor;
    let mut x = i64::from(span.right) - 1;
    for _ in 0..span.pixels {
        let texel = material.texture[cursor.texel()];
        if key != Some(texel)
            && let Some(at) = usize::try_from(x).ok().filter(|&x| x < line.len())
        {
            line[at] = material.shading.shade(cursor.light, texel);
            if let Some(trace) = trace.as_deref_mut() {
                trace[at] |= pen;
            }
        }
        cursor = cursor.step(span.step);
        x -= 1;
    }
}

#[cfg(test)]
mod tests;
