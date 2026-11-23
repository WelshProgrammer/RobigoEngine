//! Placing controls when the window has no original to copy.
//!
//! A window rebuilt from the original places its children at explicit coordinates taken from
//! `S2.EXE`, because agreeing with `DOSBox` pixel for pixel is its test and a layout engine
//! would fight it. The network screens, the settings and the diagnostic windows have no original
//! and place their children with the helpers here. The widget set does not change between them.

use s2_render::terrain::Point;

use crate::geometry::Rect;

/// Which way a [`Flow`] hands out space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Top to bottom.
    Down,
    /// Left to right.
    Across,
}

/// Hands out space from a rectangle, one control at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flow {
    rest: Rect,
    axis: Axis,
    gap: u32,
    started: bool,
}

impl Flow {
    /// Hands out rows of `area`, `gap` pixels apart.
    #[must_use]
    pub const fn down(area: Rect, gap: u32) -> Self {
        Self {
            rest: area,
            axis: Axis::Down,
            gap,
            started: false,
        }
    }

    /// Hands out columns of `area`, `gap` pixels apart.
    #[must_use]
    pub const fn across(area: Rect, gap: u32) -> Self {
        Self {
            rest: area,
            axis: Axis::Across,
            gap,
            started: false,
        }
    }

    /// Takes the next `size` pixels along the axis, which is [`Rect::EMPTY`] once the space has
    /// run out. The gap goes before every control but the first.
    pub fn take(&mut self, size: u32) -> Rect {
        let lead = if self.started { self.gap } else { 0 };
        self.started = true;
        let span = |available: u32| available.checked_sub(lead).map_or(0, |left| size.min(left));
        match self.axis {
            Axis::Down => {
                let height = span(self.rest.height);
                if height == 0 {
                    self.rest = self.rest.with_height(0);
                    return Rect::EMPTY;
                }
                let taken = Rect::new(self.rest.x, self.rest.y, self.rest.width, height)
                    .offset(0, i32::try_from(lead).unwrap_or_default());
                self.rest = Rect::new(
                    self.rest.x,
                    taken.bottom(),
                    self.rest.width,
                    self.rest.height - lead - height,
                );
                taken
            }
            Axis::Across => {
                let width = span(self.rest.width);
                if width == 0 {
                    self.rest = Rect::new(self.rest.x, self.rest.y, 0, self.rest.height);
                    return Rect::EMPTY;
                }
                let taken = Rect::new(self.rest.x, self.rest.y, width, self.rest.height)
                    .offset(i32::try_from(lead).unwrap_or_default(), 0);
                self.rest = Rect::new(
                    taken.right(),
                    self.rest.y,
                    self.rest.width - lead - width,
                    self.rest.height,
                );
                taken
            }
        }
    }

    /// What is left to hand out.
    #[must_use]
    pub const fn rest(self) -> Rect {
        self.rest
    }
}

/// Cells of one size in rows of a fixed length, which is how the original arranges its pages of
/// icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    origin: Point,
    width: u32,
    height: u32,
    columns: u32,
}

impl Grid {
    /// Cells `width` × `height`, `columns` to a row, with the first cell's corner at `origin`.
    /// A `columns` of 0 is read as 1.
    #[must_use]
    pub const fn new(origin: Point, width: u32, height: u32, columns: u32) -> Self {
        Self {
            origin,
            width,
            height,
            columns: if columns == 0 { 1 } else { columns },
        }
    }

    /// How many cells fit on a row.
    #[must_use]
    pub const fn columns(self) -> u32 {
        self.columns
    }

    /// Cell `index`, counting along each row before starting the next.
    #[must_use]
    pub fn cell(self, index: u32) -> Rect {
        let column = index % self.columns;
        let row = index / self.columns;
        let across = i32::try_from(column.saturating_mul(self.width)).unwrap_or(i32::MAX);
        let down = i32::try_from(row.saturating_mul(self.height)).unwrap_or(i32::MAX);
        Rect::new(
            self.origin.x.saturating_add(across),
            self.origin.y.saturating_add(down),
            self.width,
            self.height,
        )
    }

    /// How many rows `count` cells need.
    #[must_use]
    pub const fn rows_for(self, count: u32) -> u32 {
        count.div_ceil(self.columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_come_off_the_top_with_the_gap_between_them() {
        let mut flow = Flow::down(Rect::new(5, 10, 40, 30), 2);
        assert_eq!(flow.take(10), Rect::new(5, 10, 40, 10));
        assert_eq!(
            flow.take(10),
            Rect::new(5, 22, 40, 10),
            "the gap goes before the second"
        );
        assert_eq!(flow.rest(), Rect::new(5, 32, 40, 8));
    }

    #[test]
    fn columns_come_off_the_left() {
        let mut flow = Flow::across(Rect::new(0, 0, 30, 12), 1);
        assert_eq!(flow.take(10), Rect::new(0, 0, 10, 12));
        assert_eq!(flow.take(10), Rect::new(11, 0, 10, 12));
        assert_eq!(flow.rest(), Rect::new(21, 0, 9, 12));
    }

    #[test]
    fn a_control_too_big_for_what_is_left_gets_what_remains() {
        let mut flow = Flow::down(Rect::new(0, 0, 10, 14), 0);
        assert_eq!(flow.take(10), Rect::new(0, 0, 10, 10));
        assert_eq!(flow.take(10), Rect::new(0, 10, 10, 4), "only four rows are left");
        assert_eq!(flow.take(10), Rect::EMPTY);
        assert!(flow.rest().is_empty());
    }

    #[test]
    fn a_gap_wider_than_the_space_left_hands_out_nothing() {
        let mut flow = Flow::across(Rect::new(0, 0, 12, 4), 20);
        assert_eq!(flow.take(4), Rect::new(0, 0, 4, 4));
        assert_eq!(flow.take(4), Rect::EMPTY);
    }

    #[test]
    fn cells_fill_each_row_before_the_next() {
        let grid = Grid::new(Point::new(4, 6), 10, 8, 3);
        assert_eq!(grid.cell(0), Rect::new(4, 6, 10, 8));
        assert_eq!(grid.cell(2), Rect::new(24, 6, 10, 8));
        assert_eq!(grid.cell(3), Rect::new(4, 14, 10, 8));
        assert_eq!(grid.cell(7), Rect::new(14, 22, 10, 8));
        assert_eq!(grid.columns(), 3);
        assert_eq!(grid.rows_for(0), 0);
        assert_eq!(grid.rows_for(3), 1);
        assert_eq!(grid.rows_for(4), 2);
    }

    #[test]
    fn a_grid_with_no_columns_lays_out_one_per_row() {
        let grid = Grid::new(Point::default(), 6, 6, 0);
        assert_eq!(grid.columns(), 1);
        assert_eq!(grid.cell(2), Rect::new(0, 12, 6, 6));
    }
}
