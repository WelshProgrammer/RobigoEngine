//! The four buttons along the bottom of the view.
//!
//! The original builds them as children of the view rather than as a window, so the kind
//! [`kind::TOOLBAR`] carries is this crate's own. Each one is a style 9 button: a face from the
//! third style record, no frame around it, and its picture centred.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Skin};
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// Which style record the buttons draw their face from.
const STYLE: u16 = 2;

/// How far left of the view's middle the run starts.
const HALF: i32 = 74;

/// How far above the view's bottom edge the run sits.
const BOTTOM: i32 = 36;

/// How far above the button's own bottom edge the waiting count prints.
const COUNT: i32 = 10;

/// One button of the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Press {
    /// The picture it draws.
    picture: Picture,
    /// The text slot the one-line help shows for it.
    help: u16,
    /// What a release on it asks for.
    asks: Outcome,
}

/// The four buttons, in the order the original builds them.
const BUTTONS: [Press; 4] = [
    Press {
        picture: Picture::MAP_WINDOW,
        help: 208,
        asks: Outcome::Ask(kind::MAP),
    },
    Press {
        picture: Picture::MAIN_SELECTION,
        help: 175,
        asks: Outcome::Ask(kind::SELECT),
    },
    Press {
        picture: Picture::CONSTRUCTION_AID,
        help: 253,
        asks: Outcome::ConstructionAid,
    },
    Press {
        picture: Picture::MESSAGES,
        help: 179,
        asks: Outcome::Ask(kind::POST),
    },
];

/// Which place the messages button takes, the one that carries the waiting count and shows the
/// bird while a letter is unread.
const WAITING: usize = 3;

/// The toolbar along the bottom of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toolbar;

impl Toolbar {
    /// Which window kind it draws as.
    pub const KIND: WindowId = kind::TOOLBAR;

    /// How wide and tall one button is.
    pub const BUTTON: (u32, u32) = (37, 34);

    /// How many buttons there are.
    pub const PLACES: usize = BUTTONS.len();

    /// How wide the run is, which is one button for each place.
    pub const WIDTH: u32 = 148;

    /// How tall it is.
    pub const HEIGHT: u32 = Self::BUTTON.1;

    /// How big the run is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the run sits in a view of `bounds`.
    #[must_use]
    pub fn area(bounds: Rect) -> Rect {
        let across = i32::try_from(bounds.width / 2).unwrap_or_default();
        let down = i32::try_from(bounds.height).unwrap_or_default();
        Rect::new(
            bounds.x + across - HALF,
            bounds.y + down - BOTTOM,
            Self::WIDTH,
            Self::HEIGHT,
        )
    }

    /// Where button `place` sits, for a run whose corner is `at`.
    #[must_use]
    pub fn button(place: usize, at: Point) -> Rect {
        let step = i32::try_from(place.min(Self::PLACES - 1)).unwrap_or_default();
        let (width, height) = Self::BUTTON;
        Rect::new(
            at.x + step * i32::try_from(width).unwrap_or_default(),
            at.y,
            width,
            height,
        )
    }

    /// The text slot the one-line help shows for button `place`.
    #[must_use]
    pub fn help(place: usize) -> u16 {
        BUTTONS[place.min(Self::PLACES - 1)].help
    }

    /// Takes `mouse`, draws the run into `painter` with `waiting` letters on the messages button,
    /// and gives back what a release asks for.
    pub fn run(
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        waiting: u16,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let area = ui.window(Self::KIND)?.area;
        painter.push_clip(area);
        let at = Point::new(area.x, area.y);
        let mut asked = None;
        for (place, button) in BUTTONS.iter().enumerate() {
            let rect = Self::button(place, at);
            let widget = WidgetId::new(Self::KIND, u16::try_from(place).unwrap_or_default());
            let acted = ui.press(widget, rect, mouse, true);
            ui.note_help(rect, mouse, Label::Slot(button.help));
            let look = ui.button(widget).look(true, false);
            if let Some(face) = skin.item(Control::Toolbar, look) {
                painter.tile(face, rect);
            }
            let picture = if place == WAITING && waiting > 0 {
                Picture::MESSAGES_WAITING
            } else {
                button.picture
            };
            if let Some(item) = skin.picture(picture) {
                painter.item(item, rect.middle());
            }
            if acted && asked.is_none() {
                asked = Some(button.asks);
            }
        }
        if waiting > 0 {
            let rect = Self::button(WAITING, at);
            let line = rect.y + i32::try_from(rect.height).unwrap_or_default() - COUNT;
            let look = painter
                .lettering(STYLE)
                .unwrap_or_else(|| painter.first_lettering());
            painter.text_aligned(
                Point::new(rect.middle().x, line),
                look.font,
                look.colour,
                Label::Number(i32::from(waiting)),
                Pen::TIGHT,
                Align::Centre,
            );
        }
        painter.pop_clip();
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{ItemId, OpKind};
    use crate::skin::Look;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Toolbar, Look::Idle, ItemId(1));
        skin.set(Control::Toolbar, Look::Pressed, ItemId(2));
        for (place, button) in BUTTONS.iter().enumerate() {
            let number = u16::try_from(place).unwrap_or_default();
            skin.set_picture(button.picture, ItemId(10 + number));
        }
        skin.set_picture(Picture::MESSAGES_WAITING, ItemId(20));
        skin
    }

    fn open(bounds: Rect) -> (Ui, Painter) {
        let mut ui = Ui::new();
        ui.open(Toolbar::KIND, Toolbar::area(bounds));
        (ui, Painter::new(bounds))
    }

    #[test]
    fn the_run_sits_centred_on_the_views_bottom_edge() {
        let area = Toolbar::area(Rect::new(0, 0, 640, 480));
        assert_eq!(area, Rect::new(246, 444, 148, 34));
        let at = Point::new(area.x, area.y);
        assert_eq!(Toolbar::button(0, at), Rect::new(246, 444, 37, 34));
        assert_eq!(Toolbar::button(3, at), Rect::new(246 + 3 * 37, 444, 37, 34));
        assert_eq!(
            Toolbar::WIDTH,
            Toolbar::BUTTON.0 * u32::try_from(Toolbar::PLACES).unwrap()
        );
    }

    #[test]
    fn each_button_asks_for_its_own_window() {
        let bounds = Rect::new(0, 0, 640, 480);
        for (place, want) in [
            (0, Outcome::Ask(kind::MAP)),
            (1, Outcome::Ask(kind::SELECT)),
            (2, Outcome::ConstructionAid),
            (3, Outcome::Ask(kind::POST)),
        ] {
            let (mut ui, mut painter) = open(bounds);
            let area = Toolbar::area(bounds);
            let rect = Toolbar::button(place, Point::new(area.x, area.y));
            let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
            Toolbar::run(&mut ui, &mut painter, &skin(), 0, on.pressing());
            painter.reset();
            assert_eq!(
                Toolbar::run(&mut ui, &mut painter, &skin(), 0, on.releasing()),
                Some(want)
            );
        }
    }

    #[test]
    fn a_button_draws_its_face_and_picture_and_no_frame() {
        let bounds = Rect::new(0, 0, 640, 480);
        let (mut ui, mut painter) = open(bounds);
        Toolbar::run(&mut ui, &mut painter, &skin(), 0, Pointer::default());
        let kinds: Vec<_> = painter.ops().iter().map(|op| &op.kind).collect();
        assert_eq!(kinds.len(), 2 * Toolbar::PLACES);
        assert!(
            kinds
                .iter()
                .all(|kind| matches!(kind, OpKind::Tile { .. } | OpKind::Item { .. })),
            "{kinds:?}"
        );
    }

    #[test]
    fn the_button_under_the_pointer_names_its_line_of_help() {
        let bounds = Rect::new(0, 0, 640, 480);
        let (mut ui, mut painter) = open(bounds);
        let area = Toolbar::area(bounds);
        let rect = Toolbar::button(2, Point::new(area.x, area.y));
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        Toolbar::run(&mut ui, &mut painter, &skin(), 0, on);
        assert_eq!(ui.help(), Some(Label::Slot(Toolbar::help(2))));
    }

    #[test]
    fn the_waiting_letters_print_over_the_messages_button() {
        let bounds = Rect::new(0, 0, 640, 480);
        let (mut ui, mut painter) = open(bounds);
        Toolbar::run(&mut ui, &mut painter, &skin(), 3, Pointer::default());
        let printed: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Text { label, .. } => Some(label),
                _ => None,
            })
            .collect();
        assert_eq!(printed, vec![Label::Number(3)]);
    }

    #[test]
    fn the_messages_button_shows_the_bird_only_while_a_letter_waits() {
        let bounds = Rect::new(0, 0, 640, 480);
        let items = |waiting| {
            let (mut ui, mut painter) = open(bounds);
            Toolbar::run(&mut ui, &mut painter, &skin(), waiting, Pointer::default());
            painter
                .ops()
                .iter()
                .filter_map(|op| match op.kind {
                    OpKind::Item { item, .. } => Some(item),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(items(0), vec![ItemId(10), ItemId(11), ItemId(12), ItemId(13)]);
        assert_eq!(items(1), vec![ItemId(10), ItemId(11), ItemId(12), ItemId(20)]);
    }
}
