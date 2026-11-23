//! The frame the desktop draws round the map view: a stone border on every side, a boulder in each
//! corner of the view and the tray the toolbar's buttons sit in.
//!
//! `S2.EXE` keeps one border bitmap per screen size it runs at. A layer of one of those sizes
//! draws that bitmap as it stands. Any other size takes the smallest bitmap that covers it, keeps
//! its four corners where they are and cuts the middle out, or repeats the middle half when the
//! layer is larger than every bitmap.

use s2_assets::Sprite;
use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::items::Items;
use crate::paint::{ItemId, Painter};

/// How wide the border is on every side, at every screen size.
pub const MARGIN: u32 = 12;

/// The border bitmaps in `RESOURCE.DAT`, one per screen size, smallest first. The largest comes
/// as a left and a right half that sit side by side.
const MASKS: [&[&str]; 4] = [
    &["msk_6_1"],
    &["msk_8_1"],
    &["msk10_1"],
    &["msk12_1l", "msk12_1r"],
];

/// The boulders over the view's corners, in [`Corner`] order.
const CORNERS: [&str; 4] = ["dskbobol", "dskbobor", "dskbobul", "dskbobur"];

/// The tray the toolbar's buttons sit in, centred along the bottom edge.
const TRAY: &str = "dskbobic";

/// The corner of the view a boulder sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    const ALL: [Self; 4] = [Self::TopLeft, Self::TopRight, Self::BottomLeft, Self::BottomRight];

    const fn right(self) -> bool {
        matches!(self, Self::TopRight | Self::BottomRight)
    }

    const fn bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::BottomRight)
    }
}

/// One bitmap of the frame, with the rectangles its drawn pixels cover.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Piece {
    item: ItemId,
    width: u32,
    height: u32,
    /// Rectangles covering every drawn pixel, relative to where the item is drawn.
    cover: Vec<Rect>,
}

impl Piece {
    fn new(items: &Items, name: &str) -> Option<Self> {
        let item = items.id(name)?;
        let sprite = items.sheet().get(usize::from(item.0))?;
        Some(Self {
            item,
            width: u32::from(sprite.width()),
            height: u32::from(sprite.height()),
            cover: cover(sprite),
        })
    }
}

/// A border bitmap: its size and the pieces it is drawn from, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mask {
    width: u32,
    height: u32,
    parts: Vec<Piece>,
}

impl Mask {
    fn new(items: &Items, names: &[&str]) -> Option<Self> {
        let parts = names
            .iter()
            .map(|name| Piece::new(items, name))
            .collect::<Option<Vec<_>>>()?;
        let width = parts.iter().map(|part| part.width).sum();
        let height = parts.iter().map(|part| part.height).max()?;
        Some(Self { width, height, parts })
    }

    fn covers(&self, (width, height): (u32, u32)) -> bool {
        self.width >= width && self.height >= height
    }

    /// Draws the whole bitmap with its top-left corner at `at`.
    fn draw(&self, painter: &mut Painter, at: Point) {
        let mut x = at.x;
        for part in &self.parts {
            painter.item(part.item, Point::new(x, at.y));
            x = x.saturating_add(i32::try_from(part.width).unwrap_or(i32::MAX));
        }
    }
}

/// The desktop's frame, read from the `RESOURCE` items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Border {
    masks: Vec<Mask>,
    corners: Vec<(Corner, Piece)>,
    tray: Option<Piece>,
}

impl Border {
    /// Finds the frame's bitmaps among `items`. A bitmap that is missing is left out.
    #[must_use]
    pub fn from_items(items: &Items) -> Self {
        Self {
            masks: MASKS.iter().filter_map(|names| Mask::new(items, names)).collect(),
            corners: Corner::ALL
                .into_iter()
                .zip(CORNERS)
                .filter_map(|(corner, name)| Some((corner, Piece::new(items, name)?)))
                .collect(),
            tray: Piece::new(items, TRAY),
        }
    }

    /// The part of `bounds` the map view shows through.
    #[must_use]
    pub fn inner(bounds: Rect) -> Rect {
        bounds.shrink(MARGIN)
    }

    /// Draws the stone border round a desktop of `bounds`, which goes under everything else.
    pub fn draw_border(&self, painter: &mut Painter, bounds: Rect) {
        if let Some(mask) = self.mask_for(bounds) {
            for (across, down) in spans(bounds.width, mask.width)
                .into_iter()
                .flat_map(|x| spans(bounds.height, mask.height).into_iter().map(move |y| (x, y)))
            {
                painter.push_clip(Rect::new(
                    bounds.x + across.start,
                    bounds.y + down.start,
                    across.len,
                    down.len,
                ));
                mask.draw(
                    painter,
                    Point::new(bounds.x + across.shift, bounds.y + down.shift),
                );
                painter.pop_clip();
            }
        }
    }

    /// Draws the boulders and the tray. They go over the toolbar's buttons, whose bottom two
    /// rows the tray covers, and under the windows the player opens.
    pub fn draw_pieces(&self, painter: &mut Painter, bounds: Rect) {
        for (piece, at) in self.pieces(bounds) {
            painter.item(piece.item, at);
        }
    }

    /// The rectangles of `bounds` the frame draws over, so the renderer shows the layer there and
    /// the view everywhere else.
    #[must_use]
    pub fn cover(&self, bounds: Rect) -> Vec<Rect> {
        let mut cover = Vec::new();
        if !self.masks.is_empty() {
            let inner = Self::inner(bounds);
            let side = inner.height;
            cover.extend([
                Rect::new(bounds.x, bounds.y, bounds.width, MARGIN),
                Rect::new(bounds.x, inner.bottom(), bounds.width, MARGIN),
                Rect::new(bounds.x, inner.y, MARGIN, side),
                Rect::new(inner.right(), inner.y, MARGIN, side),
            ]);
        }
        for (piece, at) in self.pieces(bounds) {
            cover.extend(
                piece
                    .cover
                    .iter()
                    .map(|rect| Rect::new(rect.x + at.x, rect.y + at.y, rect.width, rect.height)),
            );
        }
        cover
    }

    /// The smallest border bitmap that covers `bounds`, or the largest when none does.
    fn mask_for(&self, bounds: Rect) -> Option<&Mask> {
        let size = (bounds.width, bounds.height);
        self.masks
            .iter()
            .find(|mask| mask.covers(size))
            .or_else(|| self.masks.last())
    }

    /// The boulders and the tray, each with where it is drawn on a desktop of `bounds`.
    fn pieces(&self, bounds: Rect) -> impl Iterator<Item = (&Piece, Point)> {
        let inner = Self::inner(bounds);
        let corners = self.corners.iter().map(move |(corner, piece)| {
            let x = if corner.right() {
                inner.right() - extent(piece.width)
            } else {
                inner.x
            };
            let y = if corner.bottom() {
                inner.bottom() - extent(piece.height)
            } else {
                inner.y
            };
            (piece, Point::new(x, y))
        });
        let tray = self.tray.iter().map(move |piece| {
            let x = bounds.x + extent(bounds.width) / 2 - extent(piece.width) / 2;
            (piece, Point::new(x, bounds.bottom() - extent(piece.height)))
        });
        corners.chain(tray)
    }
}

/// A run of the layer along one axis and how far the bitmap is shifted to fill it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: i32,
    len: u32,
    /// Where the bitmap's first pixel lands, so layer position `p` shows bitmap pixel `p - shift`.
    shift: i32,
}

/// Fills `target` pixels from a bitmap `source` pixels long. The first half of the target shows
/// the bitmap's first half and the last half its last half. A target longer than the bitmap
/// repeats the bitmap's middle half between them.
fn spans(target: u32, source: u32) -> Vec<Span> {
    let span = |start: u32, end: u32, shift: i64| Span {
        start: extent(start),
        len: end.saturating_sub(start),
        shift: i32::try_from(shift).unwrap_or_default(),
    };
    let right_shift = i64::from(target) - i64::from(source);
    if target <= source {
        let half = target / 2;
        return vec![span(0, half, 0), span(half, target, right_shift)];
    }
    let head = source / 2;
    let tail = target - (source - head);
    let period = (source / 2).max(1);
    let mut spans = vec![span(0, head, 0)];
    let mut at = head;
    while at < tail {
        let end = at.saturating_add(period).min(tail);
        spans.push(span(at, end, i64::from(at) - i64::from(source / 4)));
        at = end;
    }
    spans.push(span(tail, target, right_shift));
    spans
}

/// Rectangles covering every drawn pixel of `sprite`, relative to where it is drawn. Runs with
/// the same columns on consecutive rows merge into one rectangle.
fn cover(sprite: &Sprite) -> Vec<Rect> {
    let origin = sprite.origin();
    let (left, top) = (i32::from(origin.x), i32::from(origin.y));
    let mut done = Vec::new();
    let mut open: Vec<Rect> = Vec::new();
    for y in 0..sprite.height() {
        let row = i32::from(y) - top;
        let mut next = Vec::with_capacity(open.len());
        for run in sprite.row(y) {
            let x = i32::from(run.x) - left;
            let len = u32::from(run.len);
            match open.iter().position(|rect| rect.x == x && rect.width == len) {
                Some(found) => {
                    let mut rect = open.swap_remove(found);
                    rect.height += 1;
                    next.push(rect);
                }
                None => next.push(Rect::new(x, row, len, 1)),
            }
        }
        done.append(&mut open);
        open = next;
    }
    done.append(&mut open);
    done
}

/// A length as a coordinate.
fn extent(length: u32) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(spans: &[Span], target: u32) -> bool {
        let mut at = 0;
        for span in spans {
            if span.start != at {
                return false;
            }
            at += extent(span.len);
        }
        at == extent(target)
    }

    #[test]
    fn a_layer_the_size_of_the_bitmap_draws_it_as_it_stands() {
        let spans = spans(640, 640);
        assert!(covered(&spans, 640));
        assert!(spans.iter().all(|span| span.shift == 0));
    }

    #[test]
    fn a_smaller_layer_keeps_both_ends_of_the_bitmap() {
        let spans = spans(600, 640);
        assert!(covered(&spans, 600));
        assert_eq!(spans[0].shift, 0);
        assert_eq!(spans.last().map(|span| span.shift), Some(-40));
    }

    #[test]
    fn a_larger_layer_repeats_the_middle_half() {
        let spans = spans(2000, 640);
        assert!(covered(&spans, 2000));
        assert_eq!(
            spans[0],
            Span {
                start: 0,
                len: 320,
                shift: 0
            }
        );
        assert_eq!(spans.last().map(|span| span.shift), Some(1360));
        for span in &spans[1..spans.len() - 1] {
            let first = span.start - span.shift;
            assert!(first >= 160 && first + extent(span.len) <= 480, "{span:?}");
        }
    }

    #[test]
    fn a_layer_one_pixel_larger_still_closes_the_gap() {
        assert!(covered(&spans(641, 640), 641));
    }

    #[test]
    fn the_inner_rectangle_leaves_the_margin_on_every_side() {
        assert_eq!(
            Border::inner(Rect::new(0, 0, 640, 480)),
            Rect::new(12, 12, 616, 456)
        );
    }
}
