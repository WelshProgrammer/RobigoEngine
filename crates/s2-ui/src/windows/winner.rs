//! The window that names the player who won.

use s2_render::colour_base;
use s2_render::terrain::Point;
use s2_sim::{Player, World};

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The text slot the band shows for a single winner.
const ONE_WINNER: u16 = 564;
/// The text slot the band shows when a side won together.
const SEVERAL_WINNERS: u16 = 565;
/// The style the band's label takes.
const BAND_STYLE: u16 = 0;

/// Who won, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinnerView {
    /// The player the game was decided for.
    pub winner: Player,
    /// The winner and every player allied with them, in player order.
    pub side: Vec<Player>,
    /// The face of each player of [`Self::side`], in the same order.
    pub faces: Vec<u8>,
}

impl WinnerView {
    /// The winner's side: the winner and every ally in the game (`0x6658e`).
    #[must_use]
    pub fn read(world: &World, winner: Player) -> Self {
        let side: Vec<Player> = Player::ALL
            .into_iter()
            .filter(|&player| {
                player == winner || (world.slots().in_game(player) && world.allied(winner, player))
            })
            .collect();
        let faces = side
            .iter()
            .map(|&player| world.slots().get(player).map_or(0, |slot| slot.portrait.0))
            .collect();
        Self { winner, side, faces }
    }

    /// The text slot the band over the portraits shows.
    #[must_use]
    pub fn band(&self) -> u16 {
        if self.side.len() > 1 {
            SEVERAL_WINNERS
        } else {
            ONE_WINNER
        }
    }
}

/// The window that names the winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinnerWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl WinnerWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 240;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 240;
    /// Where the band over the portraits sits and how tall it is.
    const BAND: (i32, u32) = (40, 14);
    /// Where the row of portraits starts and how big one is.
    const PORTRAIT: (i32, u32, u32) = (70, 36, 48);
    /// Where a colour bar sits and how tall it is.
    const BAR: (i32, u32) = (118, 8);
    /// Where the two buttons sit across the window and how big they are.
    const BUTTON: (i32, u32, u32) = (50, 140, 20);

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

    /// Where the band over the portraits sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn band(at: Point) -> Rect {
        let (y, height) = Self::BAND;
        Rect::new(at.x, at.y + y, Self::WIDTH, height)
    }

    /// Where the portrait of the `place`th player of a side of `count` sits.
    ///
    /// The row is centred on the window, with both halves truncating toward zero as the
    /// original's divisions do. A side of seven is wider than the window and starts outside it.
    #[must_use]
    pub fn portrait(place: usize, count: usize, at: Point) -> Rect {
        let (y, width, height) = Self::PORTRAIT;
        Rect::new(at.x + Self::column(place, count), at.y + y, width, height)
    }

    /// Where the colour bar under that portrait sits.
    #[must_use]
    pub fn bar(place: usize, count: usize, at: Point) -> Rect {
        let (y, height) = Self::BAR;
        let (_, width, _) = Self::PORTRAIT;
        Rect::new(at.x + Self::column(place, count), at.y + y, width, height)
    }

    /// Where button `press` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn button(press: Press, at: Point) -> Rect {
        let (x, width, height) = Self::BUTTON;
        Rect::new(at.x + x, at.y + press.top(), width, height)
    }

    /// How far across the window the `place`th portrait of a side of `count` starts.
    fn column(place: usize, count: usize) -> i32 {
        let (_, width, _) = Self::PORTRAIT;
        let width = i32::try_from(width).unwrap_or_default();
        let count = i32::try_from(count).unwrap_or_default();
        let place = i32::try_from(place).unwrap_or_default();
        let span = i32::try_from(Self::WIDTH).unwrap_or_default();
        (span - count * width) / 2 + place * width
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &WinnerView,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        let count = view.side.len();
        for (place, (&player, &face)) in view.side.iter().zip(&view.faces).enumerate() {
            if let Some(item) = skin.picture(Picture::portrait(u16::from(face))) {
                painter.item_centred(item, Self::portrait(place, count, at));
            }
            painter.fill(Self::bar(place, count, at), colour_base(player) + 1);
        }
        crate::windows::label(painter, Self::band(at), BAND_STYLE, Label::Slot(view.band()));
        let mut outcome = None;
        for press in Press::ALL {
            let place = Self::button(press, at);
            let widget = WidgetId::new(self.id, press.slot());
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            let style = press.style();
            if let Some(face) = skin.face(style, look) {
                painter.frame_edges(face, skin.edge(style), place, look.edges());
            }
            crate::windows::label(painter, place, style, Label::Slot(press.slot()));
            if acted && outcome.is_none() {
                outcome = Some(match press {
                    Press::CarryOn => Outcome::Close(self.id),
                    Press::End => Outcome::EndGame,
                });
            }
        }
        painter.pop_clip();
        outcome
    }
}

/// One of the two buttons along the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Closes the window and leaves the game running.
    CarryOn,
    /// Ends the game, in style 1 where the other draws in style 0.
    End,
}

impl Press {
    /// The two buttons, top first.
    pub const ALL: [Self; 2] = [Self::CarryOn, Self::End];

    /// How far down the window it sits.
    const fn top(self) -> i32 {
        match self {
            Self::CarryOn => 174,
            Self::End => 196,
        }
    }

    /// The style its face and text take.
    const fn style(self) -> u16 {
        match self {
            Self::CarryOn => 0,
            Self::End => 1,
        }
    }

    /// The text slot it shows, which is also its widget slot.
    const fn slot(self) -> u16 {
        match self {
            Self::CarryOn => 562,
            Self::End => 563,
        }
    }
}

/// The kind the window opens as.
pub const KIND: WindowId = kind::WINNER;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::skin::{Control, Look};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        for style in 0..2 {
            skin.set_face(style, Look::Idle, ItemId(2));
        }
        skin
    }

    fn open() -> (WinnerWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = WinnerWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (
            WinnerWindow::new(KIND),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn view(players: &[u8]) -> WinnerView {
        let side: Vec<_> = players
            .iter()
            .map(|&index| Player::new(index).expect("a player"))
            .collect();
        WinnerView {
            winner: side[0],
            faces: vec![0; side.len()],
            side,
        }
    }

    fn click(
        window: WinnerWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: &WinnerView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), view, on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), view, on.releasing())
    }

    #[test]
    fn a_winner_with_no_allies_stands_alone() {
        let world = World::empty(32, 32);
        let winner = Player::new(3).expect("player 3");
        let view = WinnerView::read(&world, winner);
        assert_eq!(view.side, vec![winner]);
        assert_eq!(view.band(), ONE_WINNER);
    }

    #[test]
    fn the_band_says_whether_one_player_won_or_several() {
        assert_eq!(view(&[0]).band(), ONE_WINNER);
        assert_eq!(view(&[2, 0, 5]).band(), SEVERAL_WINNERS);
    }

    #[test]
    fn the_parts_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(WinnerWindow::band(at), Rect::new(0, 40, 240, 14));
        assert_eq!(WinnerWindow::portrait(0, 1, at), Rect::new(102, 70, 36, 48));
        assert_eq!(WinnerWindow::bar(0, 1, at), Rect::new(102, 118, 36, 8));
        assert_eq!(WinnerWindow::portrait(0, 2, at), Rect::new(84, 70, 36, 48));
        assert_eq!(WinnerWindow::portrait(1, 2, at), Rect::new(120, 70, 36, 48));
        assert_eq!(
            WinnerWindow::portrait(0, 7, at),
            Rect::new(-6, 70, 36, 48),
            "a side of seven starts outside the window"
        );
        assert_eq!(
            WinnerWindow::button(Press::CarryOn, at),
            Rect::new(50, 174, 140, 20)
        );
        assert_eq!(WinnerWindow::button(Press::End, at), Rect::new(50, 196, 140, 20));
    }

    #[test]
    fn each_colour_bar_takes_the_players_own_shade() {
        let (window, mut ui, mut painter) = open();
        let view = view(&[1, 3]);
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view,
            Pointer::at(Point::new(-1, -1)),
        );
        let fills: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                crate::paint::OpKind::Fill { rect, index } => Some((rect, index)),
                _ => None,
            })
            .collect();
        assert_eq!(
            fills,
            vec![
                (Rect::new(84, 118, 36, 8), 133),
                (Rect::new(120, 118, 36, 8), 141),
            ]
        );
    }

    #[test]
    fn one_button_closes_and_the_other_ends_the_game() {
        let (window, mut ui, mut painter) = open();
        let view = view(&[0]);
        let at = Point::new(0, 0);
        assert_eq!(
            click(
                window,
                &mut ui,
                &mut painter,
                &view,
                WinnerWindow::button(Press::CarryOn, at)
            ),
            Some(Outcome::Close(KIND))
        );
        assert_eq!(
            click(
                window,
                &mut ui,
                &mut painter,
                &view,
                WinnerWindow::button(Press::End, at)
            ),
            Some(Outcome::EndGame)
        );
    }

    #[test]
    fn a_rolled_up_window_draws_nothing_and_takes_no_press() {
        let (window, mut ui, mut painter) = open();
        ui.roll(KIND);
        let view = view(&[0]);
        let place = WinnerWindow::button(Press::End, Point::new(0, 0));
        assert_eq!(click(window, &mut ui, &mut painter, &view, place), None);
        assert!(
            painter.ops().is_empty(),
            "a rolled window draws nothing of its own"
        );
    }
}
