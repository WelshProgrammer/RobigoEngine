//! The map list the settings screen's Scenario button opens: the maps of one folder in a list
//! with a scroll bar, the chosen map's title, author and size under it, and Ok and Back.

use s2_render::terrain::Point;

use crate::chrome::Chrome;
use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{FRAME_SLOTS, Pointer, WidgetId, WindowId};

/// The window's size (`0x51bec`).
pub const SIZE: (u32, u32) = (213, 263);

/// The title (`0x51c3a`).
const TITLE: u16 = 359;

/// The list: its rectangle and how far in from it the rows start.
const LIST: (i32, i32, u32, u32) = (16, 25, 74, 172);
const LIST_INSET: i32 = 2;

/// The cell every glyph of a row is centred in (`0x5321c`).
const CELL: u16 = 8;

/// The colour a row's text starts from: the chosen row's and the rest's (`0x5324f`, `0x53258`).
const CHOSEN: u8 = 1;
const OTHER: u8 = 3;

/// The rows the scroll bar counts as one page (`0x520b9`). The list scrolls until this many are
/// left, which leaves blank rows at its foot since it shows more.
pub const PAGE: usize = 12;

/// The scale the scroll bar's grip arithmetic works at (`0x52d38`).
const PER_MILLE: u32 = 1000;

/// The scroll bar: its rectangle, and the palette index its track fills with (`0x52c3d`).
const BAR: (i32, i32, u32, u32) = (94, 25, 16, 172);
const TRACK: u8 = 252;

/// Ok and Back (`0x51c82`, `0x51ccb`).
const OK: (i32, i32, u32, u32) = (114, 133, 83, 30);
const BACK_DOWN: i32 = 32;
const OK_TEXT: u16 = 382;
const BACK_TEXT: u16 = 383;

/// The three lines under the list: the first's rectangle and how far apart they are.
const INFO: (i32, i32, u32, u32) = (16, 197, 190, 20);
const INFO_STEP: i32 = 15;

/// The words of the second and third lines, which a caller puts into [`MapListView::lines`]
/// through `%s: %s` and `%s:%d %s:%d` (`0x170cf7`, `0x170d04`).
pub const BY: u16 = 578;
/// See [`BY`].
pub const WIDTH: u16 = 579;
/// See [`BY`].
pub const HEIGHT: u16 = 580;

/// The last slot a control of the window may take. The buttons and the scroll bar count down
/// from it, and the rows count up from 0.
const LAST_SLOT: u16 = FRAME_SLOTS - 1;

/// The faces: grey for the scroll bar's boxes and Ok, red for Back, and the lines' style.
const GREY: u16 = 0;
const RED: u16 = 1;
const INFO_STYLE: u16 = 1;

/// The text of each line for a map whose header cannot be read (`0x51d5a`).
pub const NOTHING: &[u8] = b"---";

/// What the window shows.
#[derive(Debug, Clone, Copy)]
pub struct MapListView<'a> {
    /// The maps, as the list names them.
    pub names: &'a [Vec<u8>],
    /// The row chosen.
    pub chosen: usize,
    /// The rows scrolled past.
    pub first: usize,
    /// The three lines under the list: the chosen map's title in quotes, its author and its
    /// size, or [`NOTHING`] each.
    pub lines: &'a [Vec<u8>; 3],
    /// The line height of the font the rows draw in, `font11`.
    pub line: u32,
}

/// What a release on the window asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapPick {
    /// Choose the map in this row of the list.
    Row(usize),
    /// Take the chosen map.
    Ok,
    /// Close the list and keep the map the settings had.
    Back,
}

/// The map list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapList {
    /// The id the window opens as.
    pub id: WindowId,
}

impl Default for MapList {
    fn default() -> Self {
        Self::new()
    }
}

impl MapList {
    /// The window at its own kind.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: crate::windows::kind::MAP_LIST,
        }
    }

    /// Where the window sits on a screen whose corner is `at`: the middle, rounded up and to
    /// the left.
    #[must_use]
    pub fn area(at: Point) -> Rect {
        let half = |screen: u32, size: u32| i32::try_from(screen / 2 - size / 2).unwrap_or_default();
        Rect::new(
            at.x + half(super::SIZE.0, SIZE.0),
            at.y + half(super::SIZE.1, SIZE.1),
            SIZE.0,
            SIZE.1,
        )
    }

    /// How many rows the list shows with `line` pixels a row (`0x531e5`).
    #[must_use]
    pub fn rows(line: u32) -> usize {
        let room = LIST.3.saturating_sub(2 * LIST_INSET.unsigned_abs());
        usize::try_from(room / line.max(1)).unwrap_or_default()
    }

    /// The furthest a list of `count` rows scrolls.
    #[must_use]
    pub const fn most(count: usize) -> usize {
        count.saturating_sub(PAGE)
    }

    /// How far down its travel of `travel` pixels the grip sits for `first` of `most` rows
    /// scrolled, as `0x52ce4` works it out: in thousandths, truncating twice.
    #[must_use]
    pub fn grip(first: usize, most: usize, travel: u32) -> u32 {
        if most == 0 || travel == 0 {
            return 0;
        }
        let most = u32::try_from(most).unwrap_or(u32::MAX);
        let first = u32::try_from(first).unwrap_or(u32::MAX).min(most);
        let step = (most.saturating_mul(PER_MILLE) / travel).max(1);
        (first.saturating_mul(PER_MILLE) / step).min(travel)
    }

    /// Where row `row` of those shown sits, in a window at `area`, with `line` pixels a row.
    #[must_use]
    pub fn row(row: usize, line: u32, area: Rect) -> Rect {
        let down = i32::try_from(row).unwrap_or_default() * i32::try_from(line).unwrap_or_default();
        Rect::new(
            area.x + LIST.0 + LIST_INSET,
            area.y + LIST.1 + LIST_INSET + down,
            LIST.2 - 2 * LIST_INSET.unsigned_abs(),
            line,
        )
    }

    /// Where Ok, or Back when `back`, sits in a window at `area`.
    #[must_use]
    pub fn button(back: bool, area: Rect) -> Rect {
        let down = if back { BACK_DOWN } else { 0 };
        Rect::new(area.x + OK.0, area.y + OK.1 + down, OK.2, OK.3)
    }

    /// Draws the window for `view` and gives back what a release asks for, and which way a
    /// press on the scroll bar's ends moves the list.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &MapListView<'_>,
        mouse: Pointer,
    ) -> (Option<MapPick>, Scrolled) {
        let first = view.first;
        let Some(window) = ui.window(self.id).copied() else {
            return (None, Scrolled::Stay);
        };
        let area = window.area;
        let chrome = Chrome::new(self.id, Some(Label::Slot(TITLE)));
        chrome.under(painter, skin, (area, true));
        let line = view.line;
        let shown = Self::rows(line);
        let list = Rect::new(area.x + LIST.0, area.y + LIST.1, LIST.2, LIST.3);
        painter.fill(list, 0);
        let mut picked = None;
        for (place, name) in view.names.iter().enumerate().skip(first).take(shown) {
            let rect = Self::row(place - first, line, area);
            let widget = WidgetId::new(
                self.id,
                u16::try_from(place).unwrap_or(LAST_SLOT - 5).min(LAST_SLOT - 5),
            );
            if ui.press(widget, rect, mouse, true) {
                picked = Some(MapPick::Row(place));
            }
            let colour = if place == view.chosen { CHOSEN } else { OTHER };
            let text = painter.run(name);
            painter.text_aligned(
                Point::new(rect.x, rect.y),
                FontId::Medium,
                colour,
                text,
                Pen::cells(CELL),
                Align::Left,
            );
        }
        let scrolled = self.bar(ui, painter, skin, area, (first, view.names.len()), mouse);
        for (back, text, style, pick) in [
            (false, OK_TEXT, GREY, MapPick::Ok),
            (true, BACK_TEXT, RED, MapPick::Back),
        ] {
            let rect = Self::button(back, area);
            let widget = WidgetId::new(self.id, LAST_SLOT - u16::from(back));
            if ui.press(widget, rect, mouse, true) {
                picked = Some(pick);
            }
            let look = ui.button(widget).look(true, false);
            if let Some(face) = skin.face(style, look) {
                painter.frame_edges(face, skin.edge(style), rect, look.edges());
            }
            crate::windows::label(painter, rect, style, Label::Slot(text));
        }
        for (place, text) in (0i32..).zip(view.lines) {
            let rect = Rect::new(
                area.x + INFO.0,
                area.y + INFO.1 + INFO_STEP * place,
                INFO.2,
                INFO.3,
            );
            let label = painter.run(text);
            crate::windows::label(painter, rect, INFO_STYLE, label);
        }
        chrome.over(ui, painter, skin, area, mouse);
        (picked, scrolled)
    }

    /// Draws the scroll bar for `first` of `count` rows scrolled past, and says which way a press
    /// on it scrolls.
    ///
    /// S2 draws the bar's two ends as blank boxes (`0x52924`). Robigo puts the arrows the text
    /// panels scroll by on them.
    fn bar(
        self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        area: Rect,
        (first, count): (usize, usize),
        mouse: Pointer,
    ) -> Scrolled {
        let bar = Rect::new(area.x + BAR.0, area.y + BAR.1, BAR.2, BAR.3);
        painter.fill(bar, TRACK);
        let side = bar.width;
        let grip = side / 2;
        let up = Rect::new(bar.x, bar.y, side, side);
        let down = Rect::new(
            bar.x,
            bar.y + i32::try_from(bar.height - side).unwrap_or_default(),
            side,
            side,
        );
        let travel = bar.height.saturating_sub(2 * side + grip);
        let along = Self::grip(first, Self::most(count), travel);
        let thumb = Rect::new(
            bar.x,
            bar.y + i32::try_from(side + along).unwrap_or_default(),
            side,
            grip,
        );
        let mut scrolled = Scrolled::Stay;
        for (place, rect, way, arrow) in [
            (0, up, Scrolled::Up, Some(Picture::LINE_UP)),
            (1, thumb, Scrolled::Stay, None),
            (2, down, Scrolled::Down, Some(Picture::LINE_DOWN)),
        ] {
            let widget = WidgetId::new(self.id, LAST_SLOT - 2 - place);
            if ui.press(widget, rect, mouse, true) {
                scrolled = way;
            }
            let look = ui.button(widget).look(true, false);
            if let Some(face) = skin.face(GREY, look) {
                painter.frame_edges(face, skin.edge(GREY), rect, look.edges());
            }
            if let Some(item) = arrow.and_then(|arrow| skin.picture(arrow)) {
                painter.item_centred(item, rect);
            }
        }
        scrolled
    }
}

/// Which way a press on the scroll bar moves the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scrolled {
    /// Nowhere.
    Stay,
    /// One row up.
    Up,
    /// One row down.
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grip_steps_as_the_original_draws_it() {
        let travel = 132;
        let most = MapList::most(18);
        assert_eq!(most, 6, "eighteen maps scroll until twelve are left");
        let steps: Vec<u32> = (0..=7).map(|first| MapList::grip(first, most, travel)).collect();
        assert_eq!(
            steps,
            [0, 22, 44, 66, 88, 111, 132, 132],
            "0 to 4 as measured in S2, 5 truncating to 111 and the end held at 132"
        );
        assert_eq!(
            MapList::grip(3, 0, travel),
            0,
            "a list that fits never moves the grip"
        );
    }

    #[test]
    fn fifteen_rows_fit_the_list() {
        assert_eq!(MapList::rows(11), 15);
        assert_eq!(MapList::area(Point::new(0, 0)), Rect::new(214, 109, 213, 263));
    }
}
