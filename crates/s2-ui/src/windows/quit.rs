//! The prompt that asks before leaving.
//!
//! Alt and `q` open it. In a campaign it asks about the chapter and outside one about the game,
//! which is the only difference between the two.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The text slot the title shows outside a campaign.
const GAME_SLOT: u16 = 190;

/// The text slot it shows in a campaign.
const CHAPTER_SLOT: u16 = 198;

/// The text slot the button that leaves shows.
const LEAVE_SLOT: u16 = 216;

/// The text slot the button that stays shows.
const STAY_SLOT: u16 = 217;

/// What one of the three buttons asks for. Each one closes the prompt as well (`0x4bdb2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Leave the game, or the chapter in a campaign (`0x4b740`).
    Leave,
    /// Carry on playing.
    Stay,
    /// Open the save screen in place of the load screen (`0x44d00`).
    Save,
}

/// One of the three buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Button {
    rect: (i32, i32, u32, u32),
    shows: Picture,
    style: u16,
    help: Option<u16>,
}

impl Button {
    const fn new(rect: (i32, i32, u32, u32), shows: Picture, style: u16, help: Option<u16>) -> Self {
        Self {
            rect,
            shows,
            style,
            help,
        }
    }

    /// Where it sits in a window whose top-left corner is `at`.
    fn place(self, at: Point) -> Rect {
        let (x, y, width, height) = self.rect;
        Rect::new(at.x + x, at.y + y, width, height)
    }
}

/// The prompt that asks before leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitWindow {
    /// Whether a campaign is in play, which asks about the chapter instead of the game.
    pub campaign: bool,
}

impl QuitWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 240;
    /// How tall it is.
    pub const HEIGHT: u32 = 100;
    /// Which window kind it opens as.
    pub const KIND: WindowId = kind::QUIT;

    /// What each button shows: where it sits, the item it draws, the window style its face
    /// takes and the slot its line of help shows. The third carries no line of its own.
    const BUTTONS: [Button; 3] = [
        Button::new((16, 24, 70, 56), Picture::CONFIRM, 3, Some(LEAVE_SLOT)),
        Button::new((88, 24, 70, 56), Picture::CANCEL, 1, Some(STAY_SLOT)),
        Button::new((160, 24, 64, 56), Picture::OPEN_SAVE_GAME, 0, None),
    ];

    /// The prompt for a game that is or is not a campaign.
    #[must_use]
    pub const fn new(campaign: bool) -> Self {
        Self { campaign }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// The text slot the title shows.
    #[must_use]
    pub const fn title(self) -> u16 {
        if self.campaign { CHAPTER_SLOT } else { GAME_SLOT }
    }

    /// Where button `place` sits.
    #[must_use]
    pub fn button(place: usize, at: Point) -> Rect {
        Self::BUTTONS[place.min(Self::BUTTONS.len() - 1)].place(at)
    }

    /// Takes `mouse`, draws the prompt into `painter` and gives back what a release asks for.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(Self::KIND)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        let mut asked = None;
        for (place, button) in Self::BUTTONS.iter().enumerate() {
            let rect = button.place(at);
            let widget = WidgetId::new(Self::KIND, u16::try_from(place).unwrap_or_default());
            if let Some(line) = button.help {
                ui.note_help(rect, mouse, Label::Slot(line));
            }
            let acted = ui.press(widget, rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.face(button.style, look) {
                painter.frame_edges(item, skin.edge(button.style), rect, look.edges());
            }
            if let Some(item) = skin.picture(button.shows) {
                painter.item_centred(item, rect);
            }
            if acted && asked.is_none() {
                asked = Some(match place {
                    0 => Outcome::Quit(Answer::Leave),
                    1 => Outcome::Quit(Answer::Stay),
                    _ => Outcome::Quit(Answer::Save),
                });
            }
        }
        painter.pop_clip();
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::skin::{Control, Look};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        for style in 0..3 {
            skin.set_face(style, Look::Idle, ItemId(3));
        }
        skin
    }

    fn open() -> (Ui, QuitWindow, Painter) {
        let (width, height) = QuitWindow::size();
        let mut ui = Ui::new();
        ui.open(QuitWindow::KIND, Rect::new(0, 0, width, height));
        (
            ui,
            QuitWindow::new(false),
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(place: Rect) -> Pointer {
        Pointer::at(Point::new(place.x + 1, place.y + 1))
    }

    #[test]
    fn a_campaign_asks_about_the_chapter() {
        assert_eq!(QuitWindow::new(false).title(), 190);
        assert_eq!(QuitWindow::new(true).title(), 198);
        assert_eq!(QuitWindow::size(), (240, 100));
        let at = Point::new(0, 0);
        assert_eq!(QuitWindow::button(0, at), Rect::new(16, 24, 70, 56));
        assert_eq!(QuitWindow::button(1, at), Rect::new(88, 24, 70, 56));
        assert_eq!(QuitWindow::button(2, at), Rect::new(160, 24, 64, 56));
    }

    #[test]
    fn each_button_asks_for_its_own_answer() {
        for (place, want) in [
            (0, Outcome::Quit(Answer::Leave)),
            (1, Outcome::Quit(Answer::Stay)),
            (2, Outcome::Quit(Answer::Save)),
        ] {
            let (mut ui, mut window, mut painter) = open();
            let on = click(QuitWindow::button(place, Point::new(0, 0)));
            window.run(&mut ui, &mut painter, &skin(), on.pressing());
            painter.reset();
            assert_eq!(
                window.run(&mut ui, &mut painter, &skin(), on.releasing()),
                Some(want)
            );
        }
    }
}
