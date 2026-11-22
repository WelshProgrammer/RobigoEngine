//! The transport priorities window: the fourteen ware groups in the order the player put them,
//! with the buttons that move the chosen one.

use s2_render::terrain::Point;
use s2_sim::{Command, TRANSPORT_GROUPS, World};
use s2_systems::player::Player;

use crate::geometry::Rect;
use crate::group::GROUPS;
use crate::paint::{self, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{self, Outcome};

/// Which row the window starts with chosen, as the original's `+0x6c` does.
pub const FIRST_CHOSEN: u8 = 4;

/// Where the names of the ware groups start in the names table, which a row reports as its line
/// of help (`group + 180`, with bit 16 set to pick the names table).
const GROUP_NAMES: u16 = 180;

/// The text slot each of the four movers reports, top to bottom.
const MOVER_HELP: [u16; 4] = [261, 262, 263, 264];

/// The help page the window's own button opens, which command 10948 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1009;

/// Where each of the four buttons sends the chosen row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// To the front of the list.
    Top,
    /// One place towards the front.
    Up,
    /// One place towards the back.
    Down,
    /// To the back of the list.
    Bottom,
}

impl Move {
    /// Every button, in the order they sit down the window's right-hand side.
    pub const ALL: [Self; 4] = [Self::Top, Self::Up, Self::Down, Self::Bottom];

    /// The picture the button draws.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::Top => Picture::TO_THE_FRONT,
            Self::Up => Picture::LINE_UP,
            Self::Down => Picture::LINE_DOWN,
            Self::Bottom => Picture::TO_THE_BACK,
        }
    }

    /// Where a row at `place` lands.
    #[must_use]
    pub fn applied(self, place: u8) -> u8 {
        let last = u8::try_from(TRANSPORT_GROUPS - 1).unwrap_or(u8::MAX);
        match self {
            Self::Top => 0,
            Self::Up => place.saturating_sub(1),
            Self::Down => place.saturating_add(1).min(last),
            Self::Bottom => last,
        }
    }
}

/// What the transport window shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportView {
    /// The group at each place, front first.
    pub order: [u8; TRANSPORT_GROUPS],
}

impl TransportView {
    /// `player`'s list, turned round from the rank per group the world keeps.
    #[must_use]
    pub fn read(world: &World, player: Player) -> Self {
        let mut order = [0; TRANSPORT_GROUPS];
        for (group, &rank) in world.transport_ranks(player).iter().enumerate() {
            if let Some(place) = order.get_mut(usize::from(rank)) {
                *place = u8::try_from(group).unwrap_or_default();
            }
        }
        Self { order }
    }

    /// The group at `place`.
    #[must_use]
    pub fn group(&self, place: u8) -> Option<u8> {
        self.order.get(usize::from(place)).copied()
    }
}

/// The transport priorities window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// Which row the buttons move.
    pub chosen: u8,
}

impl TransportWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 166;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 333;
    /// A row's icon.
    pub const ROW: (u32, u32) = (30, 30);
    /// Rows to a run before the x turns back.
    const RUN: u8 = 6;
    /// Rows on the first half of a run.
    const FORWARD: u8 = 4;
    /// How far apart two rows of a run sit.
    const ROW_STEP: i32 = 32;
    /// Where the first row of a run starts.
    const ROW_LEFT: i32 = 20;
    /// Where the second half of a run starts, counting back.
    const ROW_RIGHT: i32 = 84;
    /// How far each row sits below the one before.
    const ROW_DROP: i32 = 17;
    /// Where the first row sits.
    const ROW_TOP: i32 = 25;
    /// A mover button.
    const MOVER: (u32, u32) = (30, 20);
    /// How far in from the right edge a mover sits.
    const MOVER_IN: i32 = 48;
    /// How far up from the bottom edge each mover sits, in button order.
    const MOVER_UP: [i32; 4] = [98, 78, 58, 38];
    /// The button that puts the defaults back.
    const DEFAULTS: (i32, u32, u32) = (60, 48, 30);
    /// The button that shows what the rows mean.
    const HELP: (i32, u32, u32) = (18, 30, 30);
    /// How far up from the bottom edge those two sit.
    const FOOT_UP: i32 = 48;

    /// The window at `id`, with the row the original starts on chosen.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self {
            id,
            chosen: FIRST_CHOSEN,
        }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where row `place` sits in a window whose top-left corner is `at`. A run of six goes four
    /// to the right and then turns back, which is how `0x5d765` lays them out.
    #[must_use]
    pub fn row(place: u8, at: Point) -> Rect {
        let column = i32::from(place % Self::RUN);
        let forward = i32::from(Self::FORWARD);
        let x = if column < forward {
            column * Self::ROW_STEP + Self::ROW_LEFT
        } else {
            Self::ROW_RIGHT - (column - forward) * Self::ROW_STEP
        };
        Rect::new(
            at.x + x,
            at.y + i32::from(place) * Self::ROW_DROP + Self::ROW_TOP,
            Self::ROW.0,
            Self::ROW.1,
        )
    }

    /// Where the button for `how` sits in a window of `area`.
    #[must_use]
    pub fn mover(how: Move, area: Rect) -> Rect {
        let up = Self::MOVER_UP[Move::ALL.iter().position(|&one| one == how).unwrap_or(0)];
        Rect::new(
            area.right() - Self::MOVER_IN,
            area.bottom() - up,
            Self::MOVER.0,
            Self::MOVER.1,
        )
    }

    /// Where the button that puts the defaults back sits in a window of `area`.
    #[must_use]
    pub fn defaults(area: Rect) -> Rect {
        let (x, width, height) = Self::DEFAULTS;
        Rect::new(area.x + x, area.bottom() - Self::FOOT_UP, width, height)
    }

    /// Where the button that shows what the rows mean sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (x, width, height) = Self::HELP;
        Rect::new(area.x + x, area.bottom() - Self::FOOT_UP, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does. A
    /// release on a row chooses it and makes no command.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &TransportView,
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
        let mut outcome = None;
        for place in 0..u8::try_from(TRANSPORT_GROUPS).unwrap_or(u8::MAX) {
            let row = Self::row(place, at);
            let widget = WidgetId::new(self.id, u16::from(place));
            let group = view.group(place);
            if let Some(group) = group {
                ui.note_help(row, mouse, Label::Named(GROUP_NAMES + u16::from(group)));
            }
            let acted = ui.press(widget, row, mouse, true);
            // The row takes the mark before it draws, so the frame the click lands on already
            // shows it rather than leaving the row unmarked for one frame.
            if acted {
                self.chosen = place;
            }
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), row, look.edges());
            }
            // The row the buttons move is marked rather than held, and a marked control
            // lightens its own background (`0x51898`).
            if place == self.chosen {
                painter.lighten(row.shrink(paint::EDGE));
            }
            if let Some(record) = group.and_then(|group| GROUPS.get(usize::from(group))) {
                record.draw(painter, skin, row);
            }
        }
        for how in Move::ALL {
            let place = Self::mover(how, area);
            let landing = how.applied(self.chosen);
            let enabled = landing != self.chosen;
            let widget = WidgetId::new(self.id, Self::MOVER_SLOT + Self::index(how));
            ui.note_help(
                place,
                mouse,
                Label::Slot(MOVER_HELP[usize::from(Self::index(how))]),
            );
            let acted = ui.press(widget, place, mouse, enabled);
            let look = ui.button(widget).look(enabled, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            if let Some(item) = skin.picture(how.picture()) {
                painter.item_centred(item, place);
            }
            if acted
                && outcome.is_none()
                && let Some(group) = view.group(self.chosen)
            {
                // The selection goes with the group it moved (`0x5dc9d`), so pressing the same
                // button again carries that group on rather than the one that took its place.
                self.chosen = landing;
                outcome = Some(Outcome::Command(Command::MoveTransport { group, rank: landing }));
            }
        }
        for (slot, place, what, line) in [
            (
                Self::DEFAULTS_SLOT,
                Self::defaults(area),
                None,
                windows::DEFAULTS_BUTTON,
            ),
            (
                Self::HELP_SLOT,
                Self::help(area),
                Some(HELP_PAGE),
                windows::HELP_BUTTON,
            ),
        ] {
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(place, mouse, Label::Slot(line));
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), place, look.edges());
            }
            let face = if what.is_some() {
                Picture::HELP
            } else {
                Picture::DEFAULTS
            };
            if let Some(item) = skin.picture(face) {
                painter.item_centred(item, place);
            }
            if acted && outcome.is_none() {
                outcome = Some(match what {
                    Some(page) => Outcome::Help(page),
                    None => Outcome::Command(Command::ResetTransport),
                });
            }
        }
        painter.pop_clip();
        outcome
    }

    /// The first widget slot the four movers take, past the rows.
    const MOVER_SLOT: u16 = 32;
    /// The widget slot the button that puts the defaults back takes.
    const DEFAULTS_SLOT: u16 = 48;
    /// The widget slot the button that shows what the rows mean takes.
    const HELP_SLOT: u16 = 49;

    /// A mover's place in [`Move::ALL`].
    fn index(how: Move) -> u16 {
        u16::try_from(Move::ALL.iter().position(|&one| one == how).unwrap_or(0)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::paint::OpKind;
    use crate::windows::kind;

    const WINDOW: WindowId = kind::TRANSPORT;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Button, Look::Disabled, ItemId(3));
        skin.set(Control::Tick, Look::Idle, ItemId(4));
        skin.set(Control::Tick, Look::On, ItemId(5));
        skin
    }

    fn view() -> TransportView {
        let mut order = [0; TRANSPORT_GROUPS];
        for (place, slot) in order.iter_mut().enumerate() {
            *slot = u8::try_from(TRANSPORT_GROUPS - 1 - place).expect("fits");
        }
        TransportView { order }
    }

    fn open() -> (TransportWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = TransportWindow::size();
        ui.open(WINDOW, Rect::new(0, 0, width, height));
        (
            TransportWindow::new(WINDOW),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn click(
        window: &mut TransportWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), &view(), on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), &view(), on.releasing())
    }

    #[test]
    fn a_run_of_six_rows_goes_out_and_turns_back() {
        let at = Point::new(0, 0);
        assert_eq!(TransportWindow::row(0, at), Rect::new(20, 25, 30, 30));
        assert_eq!(
            TransportWindow::row(3, at),
            Rect::new(20 + 3 * 32, 25 + 3 * 17, 30, 30)
        );
        assert_eq!(
            TransportWindow::row(4, at),
            Rect::new(84, 25 + 4 * 17, 30, 30),
            "the fifth row turns back"
        );
        assert_eq!(TransportWindow::row(5, at), Rect::new(52, 25 + 5 * 17, 30, 30));
        assert_eq!(
            TransportWindow::row(6, at).x,
            TransportWindow::row(0, at).x,
            "the next run starts over"
        );
        let last = TransportWindow::row(13, at);
        assert!(
            last.bottom() <= i32::try_from(TransportWindow::HEIGHT).expect("fits"),
            "every row fits the window"
        );
        assert_eq!(
            TransportWindow::row(0, Point::new(5, 7)),
            Rect::new(25, 32, 30, 30),
            "a row moves with the window"
        );
    }

    #[test]
    fn the_buttons_sit_down_the_right_hand_side() {
        let area = Rect::new(0, 0, TransportWindow::WIDTH, TransportWindow::HEIGHT);
        assert_eq!(
            TransportWindow::mover(Move::Top, area),
            Rect::new(118, 235, 30, 20)
        );
        assert_eq!(
            TransportWindow::mover(Move::Up, area),
            Rect::new(118, 255, 30, 20)
        );
        assert_eq!(
            TransportWindow::mover(Move::Down, area),
            Rect::new(118, 275, 30, 20)
        );
        assert_eq!(
            TransportWindow::mover(Move::Bottom, area),
            Rect::new(118, 295, 30, 20)
        );
        assert_eq!(TransportWindow::defaults(area), Rect::new(60, 285, 48, 30));
        assert_eq!(TransportWindow::help(area), Rect::new(18, 285, 30, 30));
    }

    #[test]
    fn a_mover_names_the_place_the_chosen_row_lands_on() {
        assert_eq!(Move::Top.applied(7), 0);
        assert_eq!(Move::Up.applied(7), 6);
        assert_eq!(Move::Down.applied(7), 8);
        assert_eq!(Move::Bottom.applied(7), 13);
        assert_eq!(Move::Up.applied(0), 0, "the front stays put");
        assert_eq!(Move::Down.applied(13), 13, "the back stays put");
    }

    #[test]
    fn a_click_on_a_row_chooses_it_and_makes_no_command() {
        let (mut window, mut ui, mut painter) = open();
        assert_eq!(window.chosen, FIRST_CHOSEN);
        let row = TransportWindow::row(9, Point::new(0, 0));
        assert_eq!(click(&mut window, &mut ui, &mut painter, row), None);
        assert_eq!(window.chosen, 9);
        let marked = painter
            .ops()
            .iter()
            .any(|op| matches!(op.kind, OpKind::Lighten { rect } if rect == row.shrink(paint::EDGE)));
        assert!(marked, "the row the click chose is marked on that same frame");
    }

    #[test]
    fn a_mover_moves_the_group_in_the_chosen_row() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, TransportWindow::WIDTH, TransportWindow::HEIGHT);
        let group = view().group(FIRST_CHOSEN).expect("a group");
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                TransportWindow::mover(Move::Up, area)
            ),
            Some(Outcome::Command(Command::MoveTransport { group, rank: 3 }))
        );
        assert_eq!(window.chosen, 3, "the selection goes with the group it moved");
        // The view a test holds does not move with the command, so the second press names
        // whatever that fixed view puts in the row the selection has reached.
        let landed = view().group(3).expect("a group");
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                TransportWindow::mover(Move::Top, area)
            ),
            Some(Outcome::Command(Command::MoveTransport {
                group: landed,
                rank: 0
            })),
        );
        assert_eq!(window.chosen, 0);
    }

    #[test]
    fn a_mover_with_nowhere_to_go_swallows_the_click() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, TransportWindow::WIDTH, TransportWindow::HEIGHT);
        let top = TransportWindow::row(0, Point::new(0, 0));
        click(&mut window, &mut ui, &mut painter, top);
        assert_eq!(window.chosen, 0);
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                TransportWindow::mover(Move::Top, area)
            ),
            None
        );
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                TransportWindow::mover(Move::Up, area)
            ),
            None
        );
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                TransportWindow::mover(Move::Down, area)
            ),
            Some(Outcome::Command(Command::MoveTransport {
                group: view().group(0).expect("a group"),
                rank: 1
            }))
        );
    }

    #[test]
    fn the_two_buttons_at_the_bottom_do_their_own_jobs() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, TransportWindow::WIDTH, TransportWindow::HEIGHT);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, TransportWindow::help(area)),
            Some(Outcome::Help(HELP_PAGE))
        );
        assert_eq!(
            click(
                &mut window,
                &mut ui,
                &mut painter,
                TransportWindow::defaults(area)
            ),
            Some(Outcome::Command(Command::ResetTransport))
        );
    }

    #[test]
    fn the_chosen_row_lightens_its_own_background() {
        let (mut window, mut ui, mut painter) = open();
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view(),
            Pointer::at(Point::new(-1, -1)),
        );
        let marked: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                crate::paint::OpKind::Lighten { rect } => Some(rect),
                _ => None,
            })
            .collect();
        let row = TransportWindow::row(FIRST_CHOSEN, Point::new(0, 0));
        assert_eq!(marked, vec![row.shrink(paint::EDGE)]);
    }
}
