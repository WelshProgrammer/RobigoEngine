//! Controls: what identifies one, and how a press travels through it.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::skin::Look;

/// Which window a control belongs to.
///
/// `S2.EXE` numbers its windows 0 to 46 and `0x45690` refuses to open a second window of a
/// kind already open. Robigo keeps the numbering and leaves the one-at-a-time rule to the store,
/// so a kind the original only ever showed once can gain a second instance later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u16);

/// The first slot the window frame keeps for its own controls. A window numbers its controls
/// below this, so the frame's corner buttons can never land on the same control state as one of
/// the window's own.
pub const FRAME_SLOTS: u16 = 65_280;

/// A control's identity, stable from frame to frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId {
    /// The window it sits in.
    pub window: WindowId,
    /// Its place in that window.
    pub slot: u16,
}

impl WidgetId {
    /// Control `slot` of `window`, which must be below [`FRAME_SLOTS`].
    #[must_use]
    pub const fn new(window: WindowId, slot: u16) -> Self {
        debug_assert!(slot < FRAME_SLOTS);
        Self { window, slot }
    }

    /// The frame's own control `which` of `window`.
    #[must_use]
    pub const fn framed(window: WindowId, which: u16) -> Self {
        Self {
            window,
            slot: FRAME_SLOTS.saturating_add(which),
        }
    }
}

/// How far a press on a control has got, which `S2.EXE` keeps at the widget's `+0x12`. An idle
/// control carries no press, which is the 0 the original stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Press {
    /// Held with the pointer inside the control: the original's 1.
    Inside,
    /// Held after the pointer left the control: the original's 2.
    Outside,
}

/// What the pointer is doing this frame.
///
/// At most one of `pressed` and `released` is true in a frame where the button changed, which is
/// what an event queue delivers. A frame carrying both is read as a press followed by its
/// release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pointer {
    /// Where it is, in desktop pixels.
    pub at: Point,
    /// The button went down.
    pub pressed: bool,
    /// The button came up.
    pub released: bool,
    /// The button is down, which keeps a control the pointer moves onto from showing itself
    /// under the pointer (the event's `+0xb` in `0x4d9bc`).
    pub held: bool,
}

/// Where [`Pointer::away`] puts a pointer: far enough from every window that no control can
/// contain it, and far enough inside [`i32`] that adding a window's size to it cannot overflow.
const NOWHERE: i32 = i32::MIN / 2;

impl Pointer {
    /// A pointer nowhere near this window, for the windows under the one that takes it. Their
    /// controls see it leave them, which drops a press that never ends on them.
    #[must_use]
    pub const fn away() -> Self {
        Self {
            at: Point::new(NOWHERE, NOWHERE),
            pressed: false,
            released: false,
            held: false,
        }
    }

    /// A pointer resting at `at` with no button change.
    #[must_use]
    pub const fn at(at: Point) -> Self {
        Self {
            at,
            pressed: false,
            released: false,
            held: false,
        }
    }

    /// The same pointer with the button going down.
    #[must_use]
    pub const fn pressing(self) -> Self {
        Self {
            pressed: true,
            released: false,
            held: true,
            ..self
        }
    }

    /// The same pointer with the button coming up.
    #[must_use]
    pub const fn releasing(self) -> Self {
        Self {
            pressed: false,
            released: true,
            held: false,
            ..self
        }
    }
}

/// One control's state: the press, whether it shows itself under the pointer, and whether the
/// pointer was over it last time it looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Button {
    press: Option<Press>,
    under: bool,
    inside: bool,
}

impl Button {
    /// How far a press on it has got.
    #[must_use]
    pub const fn press(self) -> Option<Press> {
        self.press
    }

    /// Takes `pointer` for a control covering `area`, and returns whether the control acts.
    ///
    /// `0x4d9bc` puts 1 in the widget's `+0x12` when the button goes down on the control, 2 when
    /// the pointer leaves while it is held, and 1 again when the pointer comes back. A release
    /// from state 1 clears the state and calls the control's action. A release from state 2
    /// clears the state and calls nothing, so sliding off a control before letting go cancels
    /// the click, which is visible behaviour.
    ///
    /// It also keeps the face state at the widget's `+0x10`. The control shows itself under the
    /// pointer when the pointer comes in with no button down, when a held press is dragged out
    /// and when a button comes up over it without having gone down there. It goes back to rest
    /// when the pointer leaves and when a press on it ends, even with the pointer still on it.
    ///
    /// `enabled` suppresses the action alone. A control that cannot act still takes the press
    /// and swallows the release, as a nonzero `+0x1d` does in the original.
    pub fn step(&mut self, pointer: Pointer, area: Rect, enabled: bool) -> bool {
        let inside = area.contains(pointer.at.x, pointer.at.y);
        if inside != self.inside {
            self.inside = inside;
            if inside {
                if self.press == Some(Press::Outside) {
                    self.press = Some(Press::Inside);
                    self.under = false;
                } else if !pointer.held {
                    self.under = true;
                }
            } else if self.press == Some(Press::Inside) {
                self.press = Some(Press::Outside);
                self.under = true;
            } else {
                self.under = false;
            }
        }
        if pointer.pressed && inside && self.press.is_none() {
            self.press = Some(Press::Inside);
            self.under = false;
        }
        if pointer.released {
            let acted = self.press == Some(Press::Inside);
            if self.press.take().is_some() {
                self.under = false;
            } else if inside {
                self.under = true;
            }
            return acted && enabled;
        }
        false
    }

    /// Whether the store has to keep this state: a control at rest away from the pointer is the
    /// same as one never seen.
    #[must_use]
    pub const fn worth_keeping(self) -> bool {
        self.press.is_some() || self.under || self.inside
    }

    /// The look to draw in: disabled first, then held, then an on state, then under the
    /// pointer, then idle. A press dragged off the control shows it under the pointer, as
    /// `0x4d9bc` sets state 2 when the pointer leaves a held control.
    #[must_use]
    pub fn look(self, enabled: bool, on: bool) -> Look {
        if !enabled {
            Look::Disabled
        } else if self.press == Some(Press::Inside) {
            Look::Pressed
        } else if on {
            Look::On
        } else if self.under {
            Look::Hover
        } else {
            Look::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect::new(10, 10, 20, 10);

    fn inside() -> Pointer {
        Pointer::at(Point::new(15, 15))
    }

    fn outside() -> Pointer {
        Pointer::at(Point::new(50, 50))
    }

    #[test]
    fn a_press_and_release_inside_acts() {
        let mut button = Button::default();
        assert!(!button.step(inside().pressing(), AREA, true));
        assert_eq!(button.press(), Some(Press::Inside));
        assert!(button.step(inside().releasing(), AREA, true));
        assert_eq!(button.press(), None);
    }

    #[test]
    fn sliding_off_before_letting_go_cancels_the_click() {
        let mut button = Button::default();
        button.step(inside().pressing(), AREA, true);
        assert!(!button.step(outside(), AREA, true));
        assert_eq!(button.press(), Some(Press::Outside));
        assert!(
            !button.step(outside().releasing(), AREA, true),
            "a release from outside acts on nothing"
        );
        assert_eq!(button.press(), None);
    }

    #[test]
    fn sliding_back_on_restores_the_press() {
        let mut button = Button::default();
        button.step(inside().pressing(), AREA, true);
        button.step(outside(), AREA, true);
        assert!(!button.step(inside(), AREA, true));
        assert_eq!(button.press(), Some(Press::Inside));
        assert!(button.step(inside().releasing(), AREA, true));
    }

    #[test]
    fn a_press_that_starts_outside_never_takes_the_control() {
        let mut button = Button::default();
        assert!(!button.step(outside().pressing(), AREA, true));
        assert_eq!(button.press(), None);
        assert!(
            !button.step(inside(), AREA, true),
            "moving over it while held does nothing"
        );
        assert_eq!(button.press(), None);
        assert!(!button.step(inside().releasing(), AREA, true));
    }

    #[test]
    fn a_disabled_control_swallows_the_click() {
        let mut button = Button::default();
        button.step(inside().pressing(), AREA, false);
        assert_eq!(button.press(), Some(Press::Inside), "it still takes the press");
        assert!(!button.step(inside().releasing(), AREA, false));
        assert_eq!(button.press(), None);
    }

    #[test]
    fn a_press_and_release_in_one_frame_acts() {
        let mut button = Button::default();
        let both = Pointer {
            at: Point::new(15, 15),
            pressed: true,
            released: true,
            held: false,
        };
        assert!(button.step(both, AREA, true));
    }

    #[test]
    fn the_look_puts_disabled_before_held_and_held_before_on() {
        let mut button = Button::default();
        assert_eq!(button.look(true, false), Look::Idle);
        assert_eq!(button.look(true, true), Look::On);
        assert_eq!(button.look(false, true), Look::Disabled);
        button.step(inside().pressing(), AREA, true);
        assert_eq!(button.look(true, true), Look::Pressed);
        assert_eq!(button.look(false, true), Look::Disabled);
        button.step(outside(), AREA, true);
        assert_eq!(
            button.look(true, false),
            Look::Hover,
            "a press dragged off shows the face under the pointer"
        );
    }

    #[test]
    fn the_pointer_coming_in_shows_the_face_under_it_and_leaving_drops_it() {
        let mut button = Button::default();
        button.step(outside(), AREA, true);
        assert_eq!(button.look(true, false), Look::Idle);
        button.step(inside(), AREA, true);
        assert_eq!(button.look(true, false), Look::Hover);
        button.step(outside(), AREA, true);
        assert_eq!(button.look(true, false), Look::Idle);
        assert!(!button.worth_keeping(), "at rest away from the pointer");
    }

    #[test]
    fn coming_in_with_the_button_down_changes_nothing() {
        let mut button = Button::default();
        let held = Pointer {
            held: true,
            ..inside()
        };
        button.step(held, AREA, true);
        assert_eq!(button.look(true, false), Look::Idle);
        button.step(inside().releasing(), AREA, true);
        assert_eq!(
            button.look(true, false),
            Look::Hover,
            "a button coming up over it shows the face under the pointer"
        );
    }

    #[test]
    fn a_click_leaves_the_control_at_rest_under_the_pointer() {
        let mut button = Button::default();
        button.step(inside(), AREA, true);
        button.step(inside().pressing(), AREA, true);
        assert_eq!(button.look(true, false), Look::Pressed);
        assert_eq!(Look::Pressed.edges(), crate::paint::Edges::Shaded);
        assert!(button.step(inside().releasing(), AREA, true));
        assert_eq!(button.look(true, false), Look::Idle);
        button.step(inside(), AREA, true);
        assert_eq!(
            button.look(true, false),
            Look::Idle,
            "until the pointer leaves and comes back"
        );
    }
}
