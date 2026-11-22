//! The state that outlives a frame.

use std::collections::BTreeMap;

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::text::Label;
use crate::widget::{Button, Pointer, WidgetId, WindowId};

/// The height `0x8ecd4` resizes a window to when it rolls up: 32 pixels.
pub const ROLLED_HEIGHT: u32 = 32;

/// An open window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Which window.
    pub id: WindowId,
    /// Where it sits and how big it is.
    pub area: Rect,
    /// The height to go back to, set only while the window is rolled up. `S2.EXE` keeps it at
    /// the window's `+0x60` with the rolled state at `+0x62`.
    pub restore: Option<u32>,
}

impl Window {
    /// Whether it is rolled up.
    #[must_use]
    pub const fn rolled(&self) -> bool {
        self.restore.is_some()
    }
}

/// Where a drag started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    /// Which control is being dragged.
    pub widget: WidgetId,
    /// Where the pointer was when it started.
    pub from: Point,
}

/// State an immediate-mode pass has nowhere else to put: which windows are open, where they sit,
/// their order front to back, and the per-control state that survives between frames.
#[derive(Debug, Clone, Default)]
pub struct Ui {
    /// Open windows, backmost first, so the frontmost is last.
    windows: Vec<Window>,
    buttons: BTreeMap<WidgetId, Button>,
    scrolls: BTreeMap<WidgetId, u32>,
    carets: BTreeMap<WidgetId, u32>,
    drag: Option<Drag>,
    help: Option<Label>,
    paused: bool,
}

impl Ui {
    /// Nothing open.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens `id` at `area` in front of everything, and says whether it opened.
    ///
    /// A window of the same id already open is left exactly as it was, which is what `0x45690`
    /// does when the per-kind record says the window is up.
    pub fn open(&mut self, id: WindowId, area: Rect) -> bool {
        if self.is_open(id) {
            return false;
        }
        self.windows.push(Window {
            id,
            area,
            restore: None,
        });
        true
    }

    /// Opens `id` at `area` behind every window already open, and says whether it opened.
    pub fn open_behind(&mut self, id: WindowId, area: Rect) -> bool {
        if self.is_open(id) {
            return false;
        }
        self.windows.insert(
            0,
            Window {
                id,
                area,
                restore: None,
            },
        );
        true
    }

    /// Closes `id` along with every control state belonging to it, and says whether it was open.
    pub fn close(&mut self, id: WindowId) -> bool {
        let before = self.windows.len();
        self.windows.retain(|window| window.id != id);
        if self.windows.len() == before {
            return false;
        }
        self.buttons.retain(|widget, _| widget.window != id);
        self.scrolls.retain(|widget, _| widget.window != id);
        self.carets.retain(|widget, _| widget.window != id);
        if self.drag.is_some_and(|drag| drag.widget.window == id) {
            self.drag = None;
        }
        true
    }

    /// Follows the game's pause flag.
    ///
    /// The one place the pause is kept, so no second overlay or second flag can disagree with it
    /// the way the original's do. Pausing closes nothing, as the original's overlay closes nothing:
    /// a window already open stays open, and one that sends a command cannot open while the flag is
    /// on ([`crate::windows::opens_while_paused`]).
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Whether the game is paused.
    #[must_use]
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Whether a control that sends a command may act, which a paused game refuses.
    #[must_use]
    pub fn may_command(&self) -> bool {
        !self.paused
    }

    /// Whether `id` is open.
    #[must_use]
    pub fn is_open(&self, id: WindowId) -> bool {
        self.windows.iter().any(|window| window.id == id)
    }

    /// The open window `id`.
    #[must_use]
    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|window| window.id == id)
    }

    /// Open windows in drawing order, backmost first.
    pub fn back_to_front(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter()
    }

    /// Open windows in hit-testing order, frontmost first.
    pub fn front_to_back(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter().rev()
    }

    /// Moves `id` in front of every other window.
    pub fn bring_to_front(&mut self, id: WindowId) {
        if let Some(place) = self.windows.iter().position(|window| window.id == id) {
            let window = self.windows.remove(place);
            self.windows.push(window);
        }
    }

    /// Moves `id`'s top-left corner to `at`, keeping its size.
    pub fn move_to(&mut self, id: WindowId, at: Point) {
        if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
            window.area.x = at.x;
            window.area.y = at.y;
        }
    }

    /// Resizes `id`, which is what a window that grows a row does. A rolled window keeps the
    /// height it will go back to.
    pub fn resize(&mut self, id: WindowId, width: u32, height: u32) {
        if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
            window.area.width = width;
            match &mut window.restore {
                Some(restore) => *restore = height,
                None => window.area.height = height,
            }
        }
    }

    /// Rolls `id` up to [`ROLLED_HEIGHT`], keeping its height to go back to, or back down to the
    /// height it kept. Says whether it ended up rolled.
    pub fn roll(&mut self, id: WindowId) -> bool {
        let Some(window) = self.windows.iter_mut().find(|window| window.id == id) else {
            return false;
        };
        if let Some(height) = window.restore.take() {
            window.area = window.area.with_height(height);
            return false;
        }
        window.restore = Some(window.area.height);
        window.area = window.area.with_height(ROLLED_HEIGHT);
        true
    }

    /// The frontmost open window covering `at`.
    #[must_use]
    pub fn hit(&self, at: Point) -> Option<WindowId> {
        self.front_to_back()
            .find(|window| window.area.contains(at.x, at.y))
            .map(|window| window.id)
    }

    /// The press state of `widget`.
    #[must_use]
    pub fn button(&self, widget: WidgetId) -> Button {
        self.buttons.get(&widget).copied().unwrap_or_default()
    }

    /// Takes `pointer` for the control `widget` covering `area`, and says whether it acts. See
    /// [`Button::step`] for the states it walks through.
    pub fn press(&mut self, widget: WidgetId, area: Rect, pointer: Pointer, enabled: bool) -> bool {
        let button = self.buttons.entry(widget).or_default();
        let acted = button.step(pointer, area, enabled);
        if !button.worth_keeping() {
            self.buttons.remove(&widget);
        }
        acted
    }

    /// Forgets the help the pointer was over, which the windows of the frame about to be drawn
    /// then fill in again.
    pub fn forget_help(&mut self) {
        self.help = None;
    }

    /// Notes that the control covering `area` carries the one-line help `help` names.
    ///
    /// The windows draw back to front and a control reports as it draws, so the last report of a
    /// frame is the control in front of the others.
    pub fn note_help(&mut self, area: Rect, pointer: Pointer, help: Label) {
        if area.contains(pointer.at.x, pointer.at.y) {
            self.help = Some(help);
        }
    }

    /// The one-line help the pointer is over.
    #[must_use]
    pub const fn help(&self) -> Option<Label> {
        self.help
    }

    /// Whether any control is holding a press, which says that a press on a window landed on a
    /// control rather than on its background.
    #[must_use]
    pub fn holding(&self) -> bool {
        self.holder().is_some()
    }

    /// The control holding a press, if one is.
    #[must_use]
    pub fn holder(&self) -> Option<WidgetId> {
        self.buttons
            .iter()
            .find(|(_, button)| button.press().is_some())
            .map(|(&widget, _)| widget)
    }

    /// How far `widget` is scrolled.
    #[must_use]
    pub fn scroll(&self, widget: WidgetId) -> u32 {
        self.scrolls.get(&widget).copied().unwrap_or_default()
    }

    /// Scrolls `widget` to `offset`.
    pub fn set_scroll(&mut self, widget: WidgetId, offset: u32) {
        if offset == 0 {
            self.scrolls.remove(&widget);
        } else {
            self.scrolls.insert(widget, offset);
        }
    }

    /// Where `widget`'s caret sits.
    #[must_use]
    pub fn caret(&self, widget: WidgetId) -> u32 {
        self.carets.get(&widget).copied().unwrap_or_default()
    }

    /// Puts `widget`'s caret at `place`.
    pub fn set_caret(&mut self, widget: WidgetId, place: u32) {
        self.carets.insert(widget, place);
    }

    /// The drag under way.
    #[must_use]
    pub const fn drag(&self) -> Option<Drag> {
        self.drag
    }

    /// Starts a drag of `widget` from `from`.
    pub fn start_drag(&mut self, widget: WidgetId, from: Point) {
        self.drag = Some(Drag { widget, from });
    }

    /// Ends the drag under way and returns what it was.
    pub fn end_drag(&mut self) -> Option<Drag> {
        self.drag.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: WindowId = WindowId(1);
    const SECOND: WindowId = WindowId(2);

    fn two() -> Ui {
        let mut ui = Ui::new();
        ui.open(FIRST, Rect::new(0, 0, 40, 30));
        ui.open(SECOND, Rect::new(20, 10, 40, 30));
        ui
    }

    fn order(ui: &Ui) -> Vec<WindowId> {
        ui.back_to_front().map(|window| window.id).collect()
    }

    #[test]
    fn a_second_window_of_the_same_kind_changes_nothing() {
        let mut ui = two();
        assert!(!ui.open(FIRST, Rect::new(99, 99, 1, 1)));
        assert_eq!(ui.window(FIRST).map(|w| w.area), Some(Rect::new(0, 0, 40, 30)));
        assert_eq!(order(&ui), [FIRST, SECOND]);
    }

    #[test]
    fn a_window_opened_behind_goes_under_the_ones_already_open() {
        let mut ui = two();
        let third = WindowId(9);
        assert!(ui.open_behind(third, Rect::new(0, 0, 10, 10)));
        assert_eq!(order(&ui), [third, FIRST, SECOND]);
        assert!(!ui.open_behind(third, Rect::new(0, 0, 10, 10)));
    }

    #[test]
    fn the_frontmost_window_takes_a_hit_in_the_overlap() {
        let mut ui = two();
        assert_eq!(ui.hit(Point::new(25, 15)), Some(SECOND));
        assert_eq!(ui.hit(Point::new(5, 5)), Some(FIRST));
        assert_eq!(ui.hit(Point::new(100, 100)), None);
        ui.bring_to_front(FIRST);
        assert_eq!(order(&ui), [SECOND, FIRST]);
        assert_eq!(ui.hit(Point::new(25, 15)), Some(FIRST));
    }

    #[test]
    fn a_roll_keeps_the_height_and_gives_it_back() {
        let mut ui = two();
        assert!(ui.roll(FIRST));
        let rolled = ui.window(FIRST).copied().expect("open");
        assert_eq!(rolled.area, Rect::new(0, 0, 40, ROLLED_HEIGHT));
        assert_eq!(rolled.restore, Some(30));
        assert!(rolled.rolled());
        assert!(!ui.roll(FIRST));
        let back = ui.window(FIRST).copied().expect("open");
        assert_eq!(back.area, Rect::new(0, 0, 40, 30));
        assert_eq!(back.restore, None);
        assert!(!ui.roll(WindowId(99)), "a window that is not open does not roll");
    }

    #[test]
    fn moving_a_window_keeps_its_size() {
        let mut ui = two();
        ui.move_to(FIRST, Point::new(-5, 7));
        assert_eq!(ui.window(FIRST).map(|w| w.area), Some(Rect::new(-5, 7, 40, 30)));
    }

    #[test]
    fn closing_a_window_drops_its_control_state() {
        let mut ui = two();
        let widget = WidgetId::new(FIRST, 3);
        ui.set_scroll(widget, 4);
        ui.set_caret(widget, 2);
        ui.start_drag(widget, Point::new(1, 1));
        ui.press(
            widget,
            Rect::new(0, 0, 10, 10),
            Pointer::at(Point::new(5, 5)).pressing(),
            true,
        );
        assert!(ui.button(widget).press().is_some());
        assert!(ui.close(FIRST));
        assert_eq!(ui.scroll(widget), 0);
        assert_eq!(ui.caret(widget), 0);
        assert_eq!(ui.drag(), None);
        assert!(ui.button(widget).press().is_none());
        assert_eq!(order(&ui), [SECOND]);
        assert!(!ui.close(FIRST), "closing it twice reports nothing to close");
    }

    #[test]
    fn a_press_runs_through_the_store() {
        let mut ui = two();
        let widget = WidgetId::new(SECOND, 0);
        let area = Rect::new(20, 10, 10, 10);
        let on = Pointer::at(Point::new(25, 15));
        assert!(!ui.press(widget, area, on.pressing(), true));
        assert!(ui.press(widget, area, on.releasing(), true));
        assert!(
            ui.button(widget).press().is_none(),
            "an idle control keeps no state"
        );
    }

    #[test]
    fn pausing_leaves_every_open_window_open() {
        use crate::windows::kind;
        let mut ui = Ui::new();
        for id in [kind::MENU, kind::TOOLS, kind::MAP, kind::FIRST_BUILDING] {
            ui.open(id, Rect::new(0, 0, 100, 100));
        }
        ui.set_paused(true);
        ui.set_paused(true);
        assert!(ui.paused());
        assert!(!ui.may_command());
        for id in [kind::MENU, kind::TOOLS, kind::MAP, kind::FIRST_BUILDING] {
            assert!(ui.is_open(id), "window {id:?} stays open");
        }
    }

    #[test]
    fn one_call_takes_the_pause_off_again() {
        let mut ui = Ui::new();
        ui.set_paused(true);
        ui.set_paused(false);
        assert!(!ui.paused(), "nothing is left paused");
        assert!(ui.may_command());
    }

    #[test]
    fn the_control_in_front_owns_the_line_of_help() {
        let mut ui = two();
        let on = Pointer::at(Point::new(5, 5));
        assert_eq!(ui.help(), None);
        ui.note_help(Rect::new(0, 0, 10, 10), on, Label::Slot(208));
        ui.note_help(Rect::new(40, 40, 10, 10), on, Label::Slot(175));
        assert_eq!(
            ui.help(),
            Some(Label::Slot(208)),
            "a control the pointer is not over reports nothing"
        );
        ui.note_help(Rect::new(0, 0, 20, 20), on, Label::Slot(179));
        assert_eq!(
            ui.help(),
            Some(Label::Slot(179)),
            "the last report of a frame is the one in front"
        );
        ui.forget_help();
        assert_eq!(ui.help(), None);
    }

    #[test]
    fn a_drag_is_handed_back_when_it_ends() {
        let mut ui = two();
        let widget = WidgetId::new(SECOND, 1);
        ui.start_drag(widget, Point::new(3, 4));
        assert_eq!(ui.drag().map(|drag| drag.from), Some(Point::new(3, 4)));
        assert_eq!(ui.end_drag().map(|drag| drag.widget), Some(widget));
        assert_eq!(ui.drag(), None);
    }
}
