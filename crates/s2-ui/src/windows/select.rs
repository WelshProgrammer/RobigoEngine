//! The screen that opens the others: eleven buttons, each naming a window kind.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// One of the buttons, as the 20-byte records at `0x1fbe48` hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opener {
    /// Where it sits in the window and how big it is.
    pub place: Rect,
    /// The `IO.DAT` item that draws it.
    pub picture: Picture,
    /// The window it opens.
    pub opens: WindowId,
    /// The text slot the line of help shows while the pointer is over it.
    pub help: u16,
}

impl Opener {
    /// The button at `place`, drawn from `item`, opening `opens` and telling `help`.
    const fn new(place: Rect, picture: Picture, opens: WindowId, help: u16) -> Self {
        Self {
            place,
            picture,
            opens,
            help,
        }
    }
}

/// The eleven buttons, in the order the original's table holds them.
pub const OPENERS: [Opener; 11] = [
    Opener::new(
        Rect::new(12, 22, 54, 44),
        Picture::OPEN_DISTRIBUTION,
        kind::DISTRIBUTION,
        139,
    ),
    Opener::new(
        Rect::new(68, 22, 54, 44),
        Picture::OPEN_TRANSPORT,
        kind::TRANSPORT,
        260,
    ),
    Opener::new(Rect::new(124, 22, 54, 44), Picture::OPEN_TOOLS, kind::TOOLS, 184),
    Opener::new(
        Rect::new(12, 70, 40, 44),
        Picture::OPEN_STATISTICS,
        kind::STATISTICS,
        185,
    ),
    Opener::new(
        Rect::new(54, 70, 40, 44),
        Picture::OPEN_MERCHANDISE,
        kind::MERCHANDISE,
        143,
    ),
    Opener::new(
        Rect::new(96, 70, 40, 44),
        Picture::OPEN_BUILDINGS,
        kind::BUILDINGS,
        142,
    ),
    Opener::new(Rect::new(138, 70, 40, 44), Picture::OPEN_STOCK, kind::STOCK, 148),
    Opener::new(
        Rect::new(12, 118, 54, 44),
        Picture::OPEN_PRODUCTION,
        kind::PRODUCTION,
        147,
    ),
    Opener::new(
        Rect::new(68, 118, 54, 44),
        Picture::OPEN_MILITARY,
        kind::MILITARY,
        140,
    ),
    Opener::new(Rect::new(124, 118, 54, 44), Picture::OPEN_SHIPS, kind::SHIPS, 187),
    Opener::new(Rect::new(12, 183, 166, 32), Picture::OPEN_MENU, kind::MENU, 205),
];

/// The screen that opens the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl SelectWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 190;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 238;
    /// The strip above the bottom button: x, y and size.
    const STRIP: (i32, i32, u32, u32) = (12, 166, 166, 12);

    /// The window at `id`.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self { id }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where button `place` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn button(place: usize, at: Point) -> Option<Rect> {
        OPENERS.get(place).map(|opener| {
            Rect::new(
                at.x + opener.place.x,
                at.y + opener.place.y,
                opener.place.width,
                opener.place.height,
            )
        })
    }

    /// Where the one-line label sits in a window whose top-left corner is `at`.
    ///
    /// `0x4f128` builds it showing nothing and it draws no background of its own, so the window's
    /// own background is what shows until there is a line to put in it.
    #[must_use]
    pub fn strip(at: Point) -> Rect {
        let (x, y, width, height) = Self::STRIP;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// What a press on the button for `opens` does, which depends on whether that window is open
    /// already (`0x46254`).
    #[must_use]
    pub fn acts(ui: &Ui, opens: WindowId) -> Outcome {
        match ui.window(opens) {
            None => Outcome::Open(opens),
            Some(window) if window.rolled() => Outcome::Roll(opens),
            Some(_) => Outcome::Close(opens),
        }
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    ///
    /// While the game is paused only the button that opens the game menu acts, since no other
    /// window the hub opens may open then. The rest still draw: the original's disabled
    /// controls, those with a nonzero `+0x1d`, draw in their own face and swallow the click, and
    /// `IO.DAT` carries a separate disabled face for the corner buttons alone, so the rest fall
    /// back to the face they always draw.
    pub fn run(&self, ui: &mut Ui, painter: &mut Painter, skin: &Skin, mouse: Pointer) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        let mut outcome = None;
        for (slot, opener) in OPENERS.iter().enumerate() {
            let Some(place) = Self::button(slot, at) else {
                continue;
            };
            let widget = WidgetId::new(self.id, u16::try_from(slot).unwrap_or_default());
            let enabled = !ui.paused() || opener.opens == kind::MENU;
            ui.note_help(place, mouse, Label::Slot(opener.help));
            let acted = ui.press(widget, place, mouse, enabled);
            let look = ui.button(widget).look(enabled, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if let Some(item) = skin.picture(opener.picture) {
                painter.item_centred(item, place);
            }
            if !enabled {
                painter.disable(place);
            }
            if acted && outcome.is_none() {
                outcome = Some(Self::acts(ui, opener.opens));
            }
        }
        painter.pop_clip();
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;

    const WINDOW: WindowId = kind::SELECT;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Title, Look::Idle, ItemId(3));
        skin
    }

    fn open() -> (SelectWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = SelectWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        (
            SelectWindow::new(WINDOW),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(window: SelectWindow, ui: &mut Ui, painter: &mut Painter, place: Rect) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), on.releasing())
    }

    #[test]
    fn every_button_fits_the_window_and_opens_its_own_kind() {
        let at = Point::new(0, 0);
        let width = i32::try_from(SelectWindow::WIDTH).expect("fits");
        let height = i32::try_from(SelectWindow::HEIGHT).expect("fits");
        let mut kinds: Vec<_> = OPENERS.iter().map(|opener| opener.opens).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), OPENERS.len(), "no kind is opened by two buttons");
        for slot in 0..OPENERS.len() {
            let place = SelectWindow::button(slot, at).expect("a button");
            assert!(place.x >= 0 && place.right() <= width, "{place:?}");
            assert!(place.y >= 0 && place.bottom() <= height, "{place:?}");
        }
        assert_eq!(SelectWindow::button(OPENERS.len(), at), None);
        assert_eq!(SelectWindow::strip(at), Rect::new(12, 166, 166, 12));
    }

    #[test]
    fn the_three_rows_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        let tops: Vec<_> = (0..OPENERS.len())
            .filter_map(|slot| SelectWindow::button(slot, at))
            .map(|place| place.y)
            .collect();
        assert_eq!(tops, [22, 22, 22, 70, 70, 70, 70, 118, 118, 118, 183]);
        let wide = SelectWindow::button(10, at).expect("the bottom button");
        assert_eq!(wide, Rect::new(12, 183, 166, 32));
    }

    #[test]
    fn a_button_opens_a_window_that_is_shut() {
        let (window, mut ui, mut painter) = open();
        let place = SelectWindow::button(2, Point::new(0, 0)).expect("the tools button");
        assert_eq!(
            click(window, &mut ui, &mut painter, place),
            Some(Outcome::Open(kind::TOOLS))
        );
    }

    #[test]
    fn a_paused_game_leaves_only_the_game_menu_button() {
        let (window, mut ui, mut painter) = open();
        ui.set_paused(true);
        for (slot, opener) in OPENERS.iter().enumerate() {
            let place = SelectWindow::button(slot, Point::new(0, 0)).expect("a button");
            let asked = click(window, &mut ui, &mut painter, place);
            if opener.opens == kind::MENU {
                assert_eq!(
                    asked,
                    Some(Outcome::Open(kind::MENU)),
                    "the game menu still opens"
                );
                ui.close(kind::MENU);
            } else {
                assert_eq!(asked, None, "{:?} acted while paused", opener.opens);
            }
        }
    }

    #[test]
    fn a_refused_button_draws_the_disabled_face_the_skin_has() {
        let (window, mut ui, mut painter) = open();
        let tools = OPENERS
            .iter()
            .position(|opener| opener.opens == kind::TOOLS)
            .expect("the tools button");
        let mut skin = skin();
        skin.set(Control::Button, Look::Disabled, ItemId(9));
        ui.set_paused(true);
        window.run(&mut ui, &mut painter, &skin, Pointer::default());
        let place = SelectWindow::button(tools, Point::new(0, 0)).expect("a button");
        let disabled = painter.ops().iter().any(|op| {
            matches!(
                op.kind,
                crate::paint::OpKind::Frame { face, within, .. }
                    if face == ItemId(9) && within == place
            )
        });
        assert!(disabled, "the tools button draws in its disabled face");
        let plain = Skin::new();
        let mut bare = plain;
        bare.set(Control::Button, Look::Idle, ItemId(2));
        painter.reset();
        window.run(&mut ui, &mut painter, &bare, Pointer::default());
        let normal = painter.ops().iter().any(|op| {
            matches!(
                op.kind,
                crate::paint::OpKind::Frame { face, within, .. }
                    if face == ItemId(2) && within == place
            )
        });
        assert!(normal, "with no disabled face it draws the one it always draws");
    }

    #[test]
    fn a_button_closes_a_window_that_is_already_showing() {
        let (window, mut ui, mut painter) = open();
        ui.open(kind::TOOLS, Rect::new(200, 0, 166, 432));
        let place = SelectWindow::button(2, Point::new(0, 0)).expect("the tools button");
        assert_eq!(
            click(window, &mut ui, &mut painter, place),
            Some(Outcome::Close(kind::TOOLS))
        );
    }

    #[test]
    fn a_button_unrolls_a_window_that_is_rolled_up() {
        let (window, mut ui, mut painter) = open();
        ui.open(kind::TOOLS, Rect::new(200, 0, 166, 432));
        ui.roll(kind::TOOLS);
        let place = SelectWindow::button(2, Point::new(0, 0)).expect("the tools button");
        assert_eq!(
            click(window, &mut ui, &mut painter, place),
            Some(Outcome::Roll(kind::TOOLS))
        );
    }

    #[test]
    fn a_rolled_up_window_draws_nothing_and_takes_no_press() {
        let (window, mut ui, mut painter) = open();
        ui.roll(WINDOW);
        let place = SelectWindow::button(0, Point::new(0, 0)).expect("a button");
        assert_eq!(click(window, &mut ui, &mut painter, place), None);
        assert!(
            painter.ops().is_empty(),
            "a rolled window draws nothing of its own"
        );
    }
}
