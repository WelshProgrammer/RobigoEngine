//! Where nodes sit on screen, as `S2.EXE`'s map view places them.
//!
//! The metrics come from the view's metrics object (built at `0x161e4`).

use s2_sim::{Direction, Node};

use super::units::Point;

/// Horizontal distance between neighbouring nodes in a row, in pixels.
pub const NODE_WIDTH: i32 = 56;

/// Vertical distance between rows, in pixels.
pub const ROW_HEIGHT: i32 = 28;

/// How far odd rows sit right of even rows, in pixels.
pub const ODD_ROW_SHIFT: i32 = 28;

/// How far node (0, 0) sits right of the scroll origin, in pixels.
pub const ORIGIN_X: i32 = 28;

/// Pixels a node rises per unit of height.
pub const HEIGHT_STEP: i32 = 5;

/// The highest altitude a node has.
pub const MAX_ALTITUDE: u8 = 127;

/// The palette index the view is cleared to before terrain is drawn.
pub const BACKGROUND: u8 = 160;

/// Rows the view pass walks beyond those it covers, for high ground lifted into view.
pub const EXTRA_ROWS: i32 = 16;

/// Columns the view pass walks beyond those it covers.
pub const EXTRA_COLUMNS: i32 = 5;

/// Rows the object pass walks beyond those the view covers.
pub const OBJECT_EXTRA_ROWS: i32 = 9;

/// Columns the object pass walks beyond those the view covers.
pub const OBJECT_EXTRA_COLUMNS: i32 = 4;

/// Where a node sits on screen.
pub trait ScreenNode {
    /// The scroll position that centres a `width` × `height` view on this node at height
    /// `height`, as the game does for the local player's headquarters when a game starts
    /// (`0x48ad4`). Not wrapped.
    ///
    /// The position is `56·x − width / 2`, `28·y − 5·h − height / 2`. Neither the odd-row shift
    /// nor [`ORIGIN_X`] is added, so the node lands right of the centre: by 28 pixels on even
    /// rows, 56 on odd.
    fn start_scroll(self, height: u8, width: u32, view_height: u32) -> Point;

    /// The node's position in world pixels at altitude `altitude`: where it is drawn when the
    /// view is scrolled to (0, 0).
    fn world_position(self, altitude: u8) -> Point;
}

impl ScreenNode for Node {
    fn start_scroll(self, height: u8, width: u32, view_height: u32) -> Point {
        let half = |n: u32| i32::try_from(n >> 1).unwrap_or(i32::MAX);
        lift(
            Point::new(
                NODE_WIDTH * i32::from(self.x) - half(width),
                ROW_HEIGHT * i32::from(self.y) - half(view_height),
            ),
            height,
        )
    }

    fn world_position(self, height: u8) -> Point {
        let shift = if self.odd_row() { ODD_ROW_SHIFT } else { 0 };
        lift(
            Point::new(
                NODE_WIDTH * i32::from(self.x) + shift + ORIGIN_X,
                ROW_HEIGHT * i32::from(self.y),
            ),
            height,
        )
    }
}

/// How far a direction moves on screen.
pub trait ScreenOffset {
    /// Screen offset from a node's unlifted position to this neighbour's, in pixels.
    fn offset(self) -> (i32, i32);
}

impl ScreenOffset for Direction {
    fn offset(self) -> (i32, i32) {
        let half = NODE_WIDTH / 2;
        match self {
            Direction::East => (NODE_WIDTH, 0),
            Direction::SouthEast => (half, ROW_HEIGHT),
            Direction::SouthWest => (-half, ROW_HEIGHT),
            Direction::West => (-NODE_WIDTH, 0),
            Direction::NorthWest => (-half, -ROW_HEIGHT),
            Direction::NorthEast => (half, -ROW_HEIGHT),
        }
    }
}

/// `point` raised by a node of altitude `altitude`.
#[must_use]
pub fn lift(point: Point, altitude: u8) -> Point {
    point.offset(0, -HEIGHT_STEP * i32::from(altitude))
}

/// The size of a `width` × `height` map in world pixels: the period it repeats with.
#[must_use]
pub fn world_size(width: u16, height: u16) -> (u32, u32) {
    (
        u32::from(width) * NODE_WIDTH.unsigned_abs(),
        u32::from(height) * ROW_HEIGHT.unsigned_abs(),
    )
}

/// The nodes the view pass visits for a view of `size` scrolled to `scroll` (already wrapped
/// into the map's period): the first row, the first column and how many of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Walk {
    /// First row, unwrapped.
    pub row: i32,
    /// First column, unwrapped; may be negative.
    pub column: i32,
    /// Rows visited.
    pub rows: i32,
    /// Columns visited per row.
    pub columns: i32,
}

impl Walk {
    /// The terrain pass's walk for a `width` × `height` view scrolled to (`x`, `y`) world pixels.
    #[must_use]
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self::with_extra(x, y, width, height, (EXTRA_COLUMNS, EXTRA_ROWS))
    }

    /// The object pass's walk (`0x1edcc`): the same first row and column, fewer rows and
    /// columns ([`OBJECT_EXTRA_COLUMNS`], [`OBJECT_EXTRA_ROWS`]).
    #[must_use]
    pub fn objects(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self::with_extra(x, y, width, height, (OBJECT_EXTRA_COLUMNS, OBJECT_EXTRA_ROWS))
    }

    fn with_extra(x: i32, y: i32, width: u32, height: u32, (columns, rows): (i32, i32)) -> Self {
        let row = y / ROW_HEIGHT;
        let shift = if row & 1 == 1 { ODD_ROW_SHIFT } else { 0 };
        let extent = |pixels: u32, unit: i32| i32::try_from(pixels).unwrap_or(i32::MAX) / unit;
        Self {
            row,
            column: (x + shift) / NODE_WIDTH - 2,
            rows: extent(height, ROW_HEIGHT) + rows,
            columns: extent(width, NODE_WIDTH) + columns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_sit_28_pixels_right_of_the_origin() {
        let node = |x, y| Node { x, y };
        assert_eq!(node(0, 0).world_position(0), Point::new(28, 0));
        assert_eq!(node(0, 1).world_position(0), Point::new(56, 28));
        assert_eq!(node(2, 2).world_position(3), Point::new(140, 41));
    }

    #[test]
    fn the_start_view_puts_the_hq_right_of_centre() {
        for y in [10, 11] {
            let hq = Node { x: 20, y };
            let scroll = hq.start_scroll(7, 616, 456);
            let at = hq.world_position(7);
            let on_view = (at.x - scroll.x, at.y - scroll.y);
            assert_eq!(on_view, (336 + 28 * i32::from(y & 1), 228), "row {y}");
        }
    }

    #[test]
    fn neighbour_offsets_match_world_positions() {
        let node = Node { x: 4, y: 5 };
        for direction in [
            Direction::East,
            Direction::SouthEast,
            Direction::SouthWest,
            Direction::West,
            Direction::NorthWest,
            Direction::NorthEast,
        ] {
            let there = node.neighbour(direction, 64, 64).world_position(0);
            let here = node.world_position(0);
            assert_eq!((there.x - here.x, there.y - here.y), direction.offset());
        }
    }

    #[test]
    fn the_walk_starts_two_columns_left() {
        assert_eq!(
            Walk::new(0, 0, 640, 480),
            Walk {
                row: 0,
                column: -2,
                rows: 17 + 16,
                columns: 11 + 5
            }
        );
        let objects = Walk::objects(0, 0, 616, 456);
        assert_eq!((objects.rows, objects.columns), (25, 15));
        let odd = Walk::new(30, 28, 56, 28);
        assert_eq!((odd.row, odd.column), (1, -1));
    }
}
