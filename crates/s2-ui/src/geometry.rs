//! Rectangles on the original's pixel grid.

use s2_render::terrain::Point;

/// A rectangle of pixels on the original's grid, as `0x7ea30` builds one.
///
/// Coordinates are signed because a window may sit partly off the left or top edge, and the
/// extents are unsigned because a rectangle never has negative size. Every operation saturates
/// rather than wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    /// Leftmost column.
    pub x: i32,
    /// Topmost row.
    pub y: i32,
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
}

impl Rect {
    /// A rectangle with no area.
    pub const EMPTY: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// The `width` × `height` rectangle whose top-left corner is (`x`, `y`).
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// The column one past the right edge.
    #[must_use]
    pub fn right(self) -> i32 {
        self.x.saturating_add(clamp_extent(self.width))
    }

    /// The row one past the bottom edge.
    #[must_use]
    pub fn bottom(self) -> i32 {
        self.y.saturating_add(clamp_extent(self.height))
    }

    /// The middle of the rectangle, where `0x4e599` puts a control's picture.
    #[must_use]
    pub fn middle(self) -> Point {
        Point::new(
            self.x.saturating_add(clamp_extent(self.width) / 2),
            self.y.saturating_add(clamp_extent(self.height) / 2),
        )
    }

    /// Whether the rectangle covers no pixels.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Whether (`x`, `y`) falls inside.
    #[must_use]
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// The pixels the two rectangles share, which is [`Rect::EMPTY`] when they miss each other.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let width = extent(self.right().min(other.right()).saturating_sub(x));
        let height = extent(self.bottom().min(other.bottom()).saturating_sub(y));
        if width == 0 || height == 0 {
            Self::EMPTY
        } else {
            Self::new(x, y, width, height)
        }
    }

    /// The same rectangle moved by (`dx`, `dy`).
    #[must_use]
    pub fn offset(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x.saturating_add(dx),
            y: self.y.saturating_add(dy),
            ..self
        }
    }

    /// The same rectangle at `height` rows, which is how a roll-up keeps a window position
    /// and restores its height.
    #[must_use]
    pub const fn with_height(self, height: u32) -> Self {
        Self { height, ..self }
    }

    /// The same rectangle at `width` columns.
    #[must_use]
    pub const fn with_width(self, width: u32) -> Self {
        Self { width, ..self }
    }

    /// The same rectangle shrunk by `inset` pixels on every side.
    #[must_use]
    pub fn shrink(self, inset: u32) -> Self {
        let twice = inset.saturating_mul(2);
        if self.width <= twice || self.height <= twice {
            return Self::EMPTY;
        }
        Self {
            x: self.x.saturating_add(clamp_extent(inset)),
            y: self.y.saturating_add(clamp_extent(inset)),
            width: self.width - twice,
            height: self.height - twice,
        }
    }
}

/// An extent as a coordinate, held inside the positive range.
fn clamp_extent(extent: u32) -> i32 {
    i32::try_from(extent).unwrap_or(i32::MAX)
}

/// A coordinate difference as an extent, with anything negative counting as none.
fn extent(span: i32) -> u32 {
    u32::try_from(span).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_containment_run_from_the_top_left() {
        let rect = Rect::new(3, 4, 10, 6);
        assert_eq!((rect.right(), rect.bottom()), (13, 10));
        assert!(rect.contains(3, 4));
        assert!(rect.contains(12, 9));
        assert!(!rect.contains(13, 9));
        assert!(!rect.contains(3, 10));
        assert!(!rect.contains(2, 4));
        assert!(Rect::EMPTY.is_empty());
        assert!(!rect.is_empty());
    }

    #[test]
    fn an_intersection_keeps_only_shared_pixels() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.intersect(Rect::new(5, 5, 10, 10)), Rect::new(5, 5, 5, 5));
        assert_eq!(a.intersect(Rect::new(-5, -5, 7, 7)), Rect::new(0, 0, 2, 2));
        assert_eq!(
            a.intersect(Rect::new(10, 0, 5, 5)),
            Rect::EMPTY,
            "touching edges share nothing"
        );
        assert_eq!(a.intersect(Rect::new(20, 20, 5, 5)), Rect::EMPTY);
        assert_eq!(a.intersect(a), a);
    }

    #[test]
    fn moving_and_resizing_leave_the_other_fields_alone() {
        let rect = Rect::new(3, 4, 10, 6);
        assert_eq!(rect.offset(-5, 2), Rect::new(-2, 6, 10, 6));
        assert_eq!(rect.with_height(32), Rect::new(3, 4, 10, 32));
        assert_eq!(rect.shrink(2), Rect::new(5, 6, 6, 2));
        assert_eq!(
            rect.shrink(3),
            Rect::EMPTY,
            "shrinking past the height leaves nothing"
        );
    }

    #[test]
    fn extents_saturate_instead_of_wrapping() {
        let huge = Rect::new(i32::MAX - 1, 0, u32::MAX, 4);
        assert_eq!(huge.right(), i32::MAX);
        assert_eq!(Rect::new(0, 0, u32::MAX, 4).offset(i32::MIN, 0).x, i32::MIN);
    }
}
