//! The graph the merchandise and statistics screens draw.
//!
//! Both screens hold a graph widget built the same way and drawn by the same shape of code, so
//! the placement and the arithmetic live here once. Every coordinate is in the control's own
//! pixels.

use s2_render::terrain::Point;
use s2_sim::{SAMPLES, SERIES};

use crate::geometry::Rect;
use crate::paint::{Align, ItemId, Painter};
use crate::text::{FontId, Label, Pen};

/// The palette index the axes, the ticks and the labels take.
pub const AXIS: u8 = 26;

/// The font the labels draw in, which is the one the drawing context's pen carries. A capture of
/// S2's merchandise screen matches `font11` glyph for glyph.
const LABEL_FONT: FontId = FontId::Medium;

/// The font a title draws in, which `0x50444` takes from the pen the context carries. A capture
/// of S2 shows it in mixed case, which the smallest font does not hold.
const TITLE_FONT: FontId = FontId::Medium;

/// Where the plot's left edge sits.
const LEFT: i32 = 25;
/// Where its right edge sits, which is also the vertical axis.
const RIGHT: i32 = 203;
/// Where a value of zero draws.
const ZERO: i32 = 118;
/// Where the largest value draws.
const TOP: i32 = 38;
/// Where the horizontal axis sits, two pixels under the zero line.
const AXIS_ROW: i32 = 120;
/// Where the vertical axis starts, five pixels under the zero line.
const AXIS_FOOT: i32 = 123;
/// Where the vertical axis ends, which is also where its upper tick sits.
const AXIS_HEAD: i32 = 40;
/// Where its other tick sits, halfway up.
const AXIS_MIDDLE: i32 = 80;
/// How far the two ticks beside the vertical axis reach.
const TICK_SPAN: (i32, i32) = (200, 206);
/// Where the labels along the bottom sit.
const LABEL_ROW: i32 = 124;
/// The two rows a tick between two labels covers.
const TICK_ROWS: (i32, i32) = (121, 122);
/// How far short of the vertical axis the largest value's label stops.
const MOST_GAP: i32 = 4;
/// Where a title sits.
const TITLE_ROW: i32 = 20;
/// Where the background item's own origin lands below the control's top edge.
const BACKGROUND_ROW: i32 = 80;
/// What a line is measured against: a value of the largest draws [`TOP`] rows up.
const FULL: u32 = 1_600_000;
/// What the measure is divided by again as a value is turned into rows.
const UNIT: u32 = 20_000;
/// How far apart two samples sit, in hundredths of a pixel.
const SAMPLE_STEP: i32 = 456;
/// How wide the labels along the bottom spread, in hundredths of a pixel.
const LABEL_SPREAD: i32 = 17_800;
/// The minutes between labels for each series (`0x1fd458`).
const MINUTES: [i32; SERIES] = [3, 10, 60, 240];
/// How many labels each series draws (`0x1fd45c`).
const COLUMNS: [usize; SERIES] = [6, 7, 5, 5];

/// The plot inside one graph control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plot {
    /// Where the control sits.
    pub area: Rect,
    /// Which series it draws, which decides the labels along the bottom.
    pub series: usize,
}

impl Plot {
    /// The plot of the control at `area` drawing `series`.
    #[must_use]
    pub const fn new(area: Rect, series: usize) -> Self {
        Self { area, series }
    }

    /// Where the background item's origin lands.
    #[must_use]
    pub fn background(&self) -> Point {
        let across = i32::try_from(self.area.width / 2).unwrap_or_default();
        Point::new(self.area.x + across, self.area.y + BACKGROUND_ROW)
    }

    /// Draws `item` as the paper the graph runs on, held inside the graph's own rectangle so
    /// the rows the paper reaches past it are cut.
    pub fn draw_paper(&self, painter: &mut Painter, item: ItemId) {
        painter.push_clip(self.area);
        painter.item(item, self.background());
        painter.pop_clip();
    }

    /// How many labels the series draws along the bottom.
    #[must_use]
    pub fn columns(&self) -> usize {
        COLUMNS.get(self.series).copied().unwrap_or_default()
    }

    /// How many minutes the label for `column` stands for, counting back from the newest sample.
    #[must_use]
    pub fn minutes(&self, column: usize) -> i32 {
        let step = MINUTES.get(self.series).copied().unwrap_or_default();
        let back = self.columns().saturating_sub(column + 1);
        -(i32::try_from(back).unwrap_or_default() * step)
    }

    /// Where the label for `column` is centred.
    #[must_use]
    pub fn division(&self, column: usize) -> i32 {
        let spread = i32::try_from(self.columns().saturating_sub(1))
            .ok()
            .and_then(|gaps| LABEL_SPREAD.checked_div(gaps))
            .unwrap_or_default();
        let across = i32::try_from(column).unwrap_or_default() * spread / 100;
        self.area.x + LEFT + across
    }

    /// Where sample `place` sits, counting from the oldest.
    #[must_use]
    pub fn place(&self, place: usize) -> i32 {
        let across = i32::try_from(place).unwrap_or_default() * SAMPLE_STEP / 100;
        self.area.x + LEFT + across
    }

    /// What a value is multiplied by before it becomes a row, so that `most` draws at the top.
    #[must_use]
    pub fn scale(most: u32) -> u32 {
        FULL.checked_div(most).unwrap_or_default()
    }

    /// Which row `value` draws on.
    #[must_use]
    pub fn row(&self, value: u32, scale: u32) -> i32 {
        let up = i32::try_from(value.wrapping_mul(scale) / UNIT).unwrap_or_default();
        self.area.y + ZERO - up
    }

    /// Draws the axes, the ticks and the labels along the bottom.
    pub fn draw(&self, painter: &mut Painter) {
        let (x, y) = (self.area.x, self.area.y);
        let (from, to) = TICK_SPAN;
        painter.line(
            Point::new(x + LEFT, y + AXIS_ROW),
            Point::new(x + RIGHT, y + AXIS_ROW),
            AXIS,
        );
        painter.line(
            Point::new(x + RIGHT, y + AXIS_FOOT),
            Point::new(x + RIGHT, y + AXIS_HEAD),
            AXIS,
        );
        for row in [AXIS_MIDDLE, AXIS_HEAD] {
            painter.line(Point::new(x + from, y + row), Point::new(x + to, y + row), AXIS);
        }
        for column in 0..self.columns() {
            let at = self.division(column);
            painter.text_aligned(
                Point::new(at, y + LABEL_ROW),
                LABEL_FONT,
                AXIS,
                Label::Number(self.minutes(column)),
                Pen::TIGHT,
                Align::Centre,
            );
            if column + 1 < self.columns() {
                let (from, to) = TICK_ROWS;
                painter.line(Point::new(at, y + from), Point::new(at, y + to), AXIS);
            }
        }
    }

    /// Draws the largest value's label, which sits against the vertical axis.
    pub fn draw_most(&self, painter: &mut Painter, most: u32) {
        painter.text_aligned(
            Point::new(self.area.x + RIGHT - MOST_GAP, self.area.y + TOP),
            LABEL_FONT,
            AXIS,
            Label::Number(i32::try_from(most).unwrap_or_default()),
            Pen::TIGHT,
            Align::Right,
        );
    }

    /// Draws `label` centred across the control.
    pub fn draw_title(&self, painter: &mut Painter, label: Label) {
        painter.text_aligned(
            Point::new(self.area.x, self.area.y + TITLE_ROW),
            TITLE_FONT,
            AXIS,
            label,
            Pen::TIGHT,
            Align::CentredIn(self.area.width),
        );
    }

    /// Draws one line of `colour` through `values`, which hold the newest sample first. Each
    /// segment is drawn twice, the second a row lower, which is what makes the line two pixels
    /// thick.
    pub fn draw_line(&self, painter: &mut Painter, values: &[u32; SAMPLES], scale: u32, colour: u8) {
        for place in 1..SAMPLES {
            let older = values[SAMPLES - place];
            let newer = values[SAMPLES - place - 1];
            let from = Point::new(self.place(place - 1), self.row(older, scale));
            let to = Point::new(self.place(place), self.row(newer, scale));
            painter.line(from, to, colour);
            painter.line(from.offset(0, 1), to.offset(0, 1), colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot(series: usize) -> Plot {
        Plot::new(Rect::new(11, 24, 230, 262), series)
    }

    #[test]
    fn the_paper_is_held_inside_the_graph() {
        let plot = plot(0);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        plot.draw_paper(&mut painter, ItemId(1));
        let clips: Vec<_> = painter.ops().iter().map(|op| op.clip).collect();
        assert_eq!(clips, vec![plot.area], "the paper reaches a row above the graph");
    }

    #[test]
    fn the_labels_count_back_over_the_series_span() {
        let shortest = plot(0);
        assert_eq!(shortest.columns(), 6);
        assert_eq!(shortest.minutes(0), -15);
        assert_eq!(shortest.minutes(5), 0);
        let longest = plot(3);
        assert_eq!(longest.columns(), 5);
        assert_eq!(longest.minutes(0), -960);
        assert_eq!(longest.minutes(4), 0);
    }

    #[test]
    fn the_labels_spread_across_the_plot() {
        let plot = plot(0);
        assert_eq!(plot.division(0), 11 + 25);
        assert_eq!(plot.division(5), 11 + 25 + 178);
        assert_eq!(plot.place(0), 11 + 25);
        assert_eq!(plot.place(39), 11 + 25 + 177);
    }

    #[test]
    fn the_largest_value_draws_at_the_top_and_zero_on_the_zero_line() {
        let plot = plot(0);
        let scale = Plot::scale(500);
        assert_eq!(plot.row(0, scale), 24 + 118);
        assert_eq!(plot.row(500, scale), 24 + 38);
        assert_eq!(plot.row(250, scale), 24 + 78);
        assert_eq!(
            plot.row(500, Plot::scale(0)),
            24 + 118,
            "nothing counted keeps every sample on the zero line"
        );
    }

    #[test]
    fn a_line_draws_each_segment_twice_from_the_oldest_sample() {
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let plot = plot(0);
        let mut values = [0u32; SAMPLES];
        values[0] = 100;
        plot.draw_line(&mut painter, &values, Plot::scale(100), 128);
        assert_eq!(painter.ops().len(), (SAMPLES - 1) * 2);
        let last = painter.ops()[(SAMPLES - 1) * 2 - 2].kind;
        assert_eq!(
            last,
            crate::paint::OpKind::Line {
                from: Point::new(plot.place(38), plot.row(0, Plot::scale(100))),
                to: Point::new(plot.place(39), plot.row(100, Plot::scale(100))),
                index: 128,
            }
        );
    }
}
