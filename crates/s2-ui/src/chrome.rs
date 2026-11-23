//! The frame around a window: the title bar, the side and bottom strips and the four corner
//! buttons.
//!
//! Every window kind in the factory table is built on the same base class, so the frame is drawn
//! once here rather than in each window module. A window lays its controls out from its own
//! top-left corner and the original's rectangles already allow for the frame, so the layouts in
//! the window modules need no offset.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, ItemId, Painter, Place};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::Outcome;

/// How far the frame's art sits from the window's own edge.
const INSET: i32 = 1;

/// How wide the side strips are.
pub const SIDE: u32 = 10;

/// How tall the title bar is.
pub const TITLE: u32 = 20;

/// How tall the bottom strip is.
pub const BOTTOM: u32 = 11;

/// How wide and tall a corner button is.
pub const BUTTON: u32 = 16;

/// How wide the item at the title bar's left end is.
const LEFT_CAP: u32 = 35;

/// How wide the item at its right end is.
const RIGHT_CAP: u32 = 34;

/// Which style record the title takes its font and colour from.
const TITLE_STYLE: u16 = 0;

/// How far down the title bar the title prints.
const TITLE_ROW: i32 = 5;

/// Which corner of the window a button sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Corner {
    /// The one that closes the window.
    Close,
    /// The one that rolls it up and down.
    Roll,
    /// The one at the bottom left, which carries no button in either shipped build.
    Spare,
    /// The one that resizes the window.
    Resize,
}

impl Corner {
    /// Every corner, in the order `0x8e92c` builds them.
    pub const ALL: [Self; 4] = [Self::Close, Self::Roll, Self::Spare, Self::Resize];

    /// The flag bit that puts a button in it rather than a blank.
    #[must_use]
    pub const fn flag(self) -> u16 {
        match self {
            Self::Close => 1 << 0,
            Self::Roll => 1 << 1,
            Self::Resize => 1 << 2,
            Self::Spare => 0,
        }
    }

    /// The control it draws when the flag is set.
    #[must_use]
    pub const fn control(self) -> Control {
        match self {
            Self::Close => Control::Close,
            Self::Roll => Control::Roll,
            Self::Resize => Control::Resize,
            Self::Spare => Control::Blank,
        }
    }

    /// Its place in [`Corner::ALL`], which is also the widget slot it takes.
    #[must_use]
    pub const fn slot(self) -> u16 {
        self as u16
    }
}

/// The flags most of the original's windows carry: a close button and a roll button, no resize.
pub const ORIGINAL_FLAGS: u16 = Corner::Close.flag() | Corner::Roll.flag();

/// The frame a window draws around itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    /// Which window it belongs to.
    pub id: WindowId,
    /// Which corner buttons it carries.
    pub flags: u16,
    /// The text the title shows, or nothing for a window with no title.
    pub title: Option<Label>,
    /// What the client area fills with, or nothing for the one the base class draws.
    pub backdrop: Option<Picture>,
    /// What the window's own client widget covers, or nothing for the whole of
    /// [`Chrome::inside`]. A panel that takes whole lines leaves the rows under it to the
    /// frame, which is what shows along the foot of a help page.
    pub client: Option<Rect>,
}

impl Chrome {
    /// The frame of `id` with the corner buttons its own window carries.
    #[must_use]
    pub const fn new(id: WindowId, title: Option<Label>) -> Self {
        Self {
            id,
            flags: crate::windows::corner_flags(id),
            title,
            backdrop: crate::windows::backdrop(id),
            client: None,
        }
    }

    /// The part of a window of `area` its own controls draw in.
    #[must_use]
    pub fn inside(area: Rect) -> Rect {
        let side = i32::try_from(SIDE).unwrap_or_default() + INSET;
        let top = i32::try_from(TITLE).unwrap_or_default() + INSET;
        Rect::new(
            area.x + side,
            area.y + top,
            area.width.saturating_sub(2 * SIDE + 2),
            area.height.saturating_sub(TITLE + BOTTOM + 1),
        )
    }

    /// The part of the title bar the repeating item covers, between the two ends.
    #[must_use]
    fn between_the_ends(bar: Rect) -> Rect {
        Rect::new(
            bar.x + i32::try_from(LEFT_CAP).unwrap_or_default(),
            bar.y,
            bar.width.saturating_sub(LEFT_CAP + RIGHT_CAP),
            bar.height,
        )
    }

    /// Where the title bar sits in a window of `area`.
    #[must_use]
    pub fn bar(area: Rect) -> Rect {
        Rect::new(
            area.x + INSET,
            area.y + INSET,
            area.width.saturating_sub(2),
            TITLE,
        )
    }

    /// Where the corner button `corner` sits in a window of `area`.
    #[must_use]
    pub fn button(corner: Corner, area: Rect) -> Rect {
        let right = i32::try_from(area.width).unwrap_or_default() - i32::try_from(BUTTON).unwrap_or_default();
        let down = i32::try_from(area.height).unwrap_or_default() - i32::try_from(BUTTON).unwrap_or_default();
        let (across, drop) = match corner {
            Corner::Close => (INSET, INSET),
            Corner::Roll => (right - INSET, INSET),
            Corner::Spare => (INSET, down - INSET),
            Corner::Resize => (right - INSET, down - INSET),
        };
        Rect::new(area.x + across, area.y + drop, BUTTON, BUTTON)
    }

    /// Draws the frame of a window of `area` and gives back what a release on a corner asks for.
    ///
    /// `front` picks the title bar's own look, which the original draws in a second colour for
    /// the window in front of the others.
    pub fn run(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        (area, front): (Rect, bool),
        mouse: Pointer,
    ) -> Option<Outcome> {
        self.under(painter, skin, (area, front));
        self.over(ui, painter, skin, area, mouse)
    }

    /// Draws what goes down before the window's own controls: the background, the title bar and
    /// the three strips.
    pub fn under(&self, painter: &mut Painter, skin: &Skin, (area, front): (Rect, bool)) {
        painter.push_clip(area);
        self.strips(painter, skin, area, front);
        self.client(painter, skin, area);
        painter.pop_clip();
    }

    /// Draws what goes down after them, which is the four corners, and gives back what a release
    /// on one asks for. A window whose own controls reach into a corner is drawn under it.
    pub fn over(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        area: Rect,
        mouse: Pointer,
    ) -> Option<Outcome> {
        painter.push_clip(area);
        let asked = self.corners(ui, painter, skin, area, mouse);
        painter.pop_clip();
        asked
    }

    /// Draws the window's own background, the title bar and the three strips.
    fn strips(self, painter: &mut Painter, skin: &Skin, area: Rect, front: bool) {
        // The frame starts a pixel inside the window on every side, so a one pixel band is left
        // over all the way round. Black is what the original leaves showing there.
        painter.fill(area, 0);
        let bar = Self::bar(area);
        let look = if front { Look::On } else { Look::Idle };
        if let Some(item) = skin.item(Control::Title, look) {
            // The ends go down first and the bar repeats between them, so the pattern starts at
            // the left end's inner edge rather than at the bar's own corner.
            painter.tile(item, Self::between_the_ends(bar));
        }
        if let Some(item) = skin.item(Control::TitleLeft, Look::Idle) {
            painter.item_placed(item, bar, Place::TopLeft);
        }
        if let Some(item) = skin.item(Control::TitleRight, Look::Idle) {
            painter.item_placed(item, bar, Place::TopRight);
        }
        if let Some(title) = self.title {
            // The title belongs to the bar's own widget, the strip between the two ends, which
            // centres it in itself and clips it there (`0x8f7a2`).
            let style = painter
                .lettering(TITLE_STYLE)
                .unwrap_or_else(|| painter.first_lettering());
            let strip = Self::between_the_ends(bar);
            painter.push_clip(strip);
            painter.text_aligned(
                Point::new(strip.x, strip.y + TITLE_ROW),
                style.font,
                style.colour,
                title,
                Pen::TIGHT,
                Align::Across(strip.width),
            );
            painter.pop_clip();
        }
        let inside = Self::inside(area);
        let sides = Rect::new(area.x + INSET, inside.y, SIDE, inside.height);
        if let Some(item) = skin.item(Control::SideLeft, Look::Idle) {
            painter.tile(item, sides);
        }
        if let Some(item) = skin.item(Control::SideRight, Look::Idle) {
            let right = area.x + i32::try_from(area.width).unwrap_or_default() - INSET;
            painter.tile(
                item,
                Rect::new(
                    right - i32::try_from(SIDE).unwrap_or_default(),
                    sides.y,
                    SIDE,
                    sides.height,
                ),
            );
        }
        if let Some(item) = skin.item(Control::BottomEdge, Look::Idle) {
            let down = area.y + i32::try_from(area.height).unwrap_or_default() - INSET;
            painter.tile(
                item,
                Rect::new(
                    area.x + INSET,
                    down - i32::try_from(BOTTOM).unwrap_or_default(),
                    area.width.saturating_sub(2),
                    BOTTOM,
                ),
            );
        }
    }

    /// Fills the client area, which the window's own widget does after the frame is down and
    /// which is why the strip along the bottom shows one row fewer than its item is tall.
    fn client(self, painter: &mut Painter, skin: &Skin, area: Rect) {
        let fill: Option<ItemId> = self
            .backdrop
            .and_then(|picture| skin.picture(picture))
            .or_else(|| skin.item(Control::Pattern, Look::Idle));
        if let Some(item) = fill {
            // The background repeats from the window's own corner and shows only inside it, so
            // the pattern does not shift when the frame's thickness changes.
            painter.push_clip(self.client.unwrap_or_else(|| Self::inside(area)));
            painter.tile(item, area);
            painter.pop_clip();
        }
    }

    /// Draws the four corners and gives back what a release on one asks for.
    fn corners(
        self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        area: Rect,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let mut asked = None;
        for corner in Corner::ALL {
            let rect = Self::button(corner, area);
            let carried = corner.flag() != 0 && self.flags & corner.flag() != 0;
            if !carried {
                if let Some(item) = skin.item(Control::Blank, Look::Idle) {
                    painter.item(item, Point::new(rect.x, rect.y));
                }
                continue;
            }
            let widget = WidgetId::framed(self.id, corner.slot());
            let acted = ui.press(widget, rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(corner.control(), look) {
                painter.item(item, Point::new(rect.x, rect.y));
            }
            if acted && asked.is_none() {
                asked = match corner {
                    Corner::Close => Some(Outcome::Close(self.id)),
                    Corner::Roll => Some(Outcome::Roll(self.id)),
                    Corner::Resize | Corner::Spare => None,
                };
            }
        }
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;

    const WINDOW: WindowId = WindowId(28);

    fn skin() -> Skin {
        let mut skin = Skin::new();
        for control in Control::CHROME {
            skin.set(control, Look::Idle, ItemId(1));
        }
        skin
    }

    #[test]
    fn the_inside_leaves_room_for_the_frame() {
        let inside = Chrome::inside(Rect::new(0, 0, 250, 320));
        assert_eq!(inside, Rect::new(11, 21, 228, 288));
        assert_eq!(Chrome::bar(Rect::new(0, 0, 250, 320)), Rect::new(1, 1, 248, 20));
    }

    #[test]
    fn each_corner_sits_in_its_own_corner() {
        let area = Rect::new(10, 20, 250, 320);
        assert_eq!(Chrome::button(Corner::Close, area), Rect::new(11, 21, 16, 16));
        assert_eq!(Chrome::button(Corner::Roll, area), Rect::new(243, 21, 16, 16));
        assert_eq!(Chrome::button(Corner::Spare, area), Rect::new(11, 323, 16, 16));
        assert_eq!(Chrome::button(Corner::Resize, area), Rect::new(243, 323, 16, 16));
    }

    #[test]
    fn the_close_corner_closes_and_the_roll_corner_rolls() {
        let area = Rect::new(0, 0, 250, 320);
        for (corner, want) in [
            (Corner::Close, Outcome::Close(WINDOW)),
            (Corner::Roll, Outcome::Roll(WINDOW)),
        ] {
            let mut ui = Ui::new();
            ui.open(WINDOW, area);
            let mut painter = Painter::new(area);
            let chrome = Chrome::new(WINDOW, Some(Label::Slot(148)));
            let rect = Chrome::button(corner, area);
            let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
            chrome.run(&mut ui, &mut painter, &skin(), (area, true), on.pressing());
            painter.reset();
            assert_eq!(
                chrome.run(&mut ui, &mut painter, &skin(), (area, true), on.releasing()),
                Some(want)
            );
        }
    }

    #[test]
    fn a_window_numbering_its_own_controls_high_leaves_the_corners_alone() {
        let area = Rect::new(0, 0, 250, 320);
        let mut ui = Ui::new();
        ui.open(WINDOW, area);
        let mut painter = Painter::new(area);
        let chrome = Chrome::new(WINDOW, Some(Label::Slot(148)));
        let rect = Chrome::button(Corner::Close, area);
        let elsewhere = Rect::new(100, 100, 20, 20);
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        // The window's own control runs first each frame, as the desktop draws it under the
        // frame, and its highest slot must not reach the corner's.
        let own = WidgetId::new(WINDOW, crate::widget::FRAME_SLOTS - 1);
        ui.press(own, elsewhere, on.pressing(), true);
        chrome.run(&mut ui, &mut painter, &skin(), (area, true), on.pressing());
        painter.reset();
        ui.press(own, elsewhere, on.releasing(), true);
        assert_eq!(
            chrome.run(&mut ui, &mut painter, &skin(), (area, true), on.releasing()),
            Some(Outcome::Close(WINDOW))
        );
    }

    #[test]
    fn a_corner_with_no_flag_takes_no_press() {
        let area = Rect::new(0, 0, 250, 320);
        let mut ui = Ui::new();
        ui.open(WINDOW, area);
        let mut painter = Painter::new(area);
        let mut chrome = Chrome::new(WINDOW, None);
        chrome.flags = 0;
        let rect = Chrome::button(Corner::Close, area);
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        chrome.run(&mut ui, &mut painter, &skin(), (area, false), on.pressing());
        assert_eq!(
            chrome.run(&mut ui, &mut painter, &skin(), (area, false), on.releasing()),
            None
        );
    }
}
