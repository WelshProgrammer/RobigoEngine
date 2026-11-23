//! What a key does while a game is on.
//!
//! The original reads set 1 scancodes and dispatches through a jump table. The table is kept here
//! as a match on the same scancodes so the bindings can be read, changed and listed later,
//! starting from what S2 does. Two keys are left out: `n`, which only redraws the map's
//! picture, and `F6`, whose routine has not been read out of `S2.EXE`.

use crate::widget::WindowId;
use crate::windows::kind;

/// A key as the original reads it: a set 1 scancode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Scancode(pub u8);

impl Scancode {
    /// The bit a set 1 scancode carries when the key is let go.
    const RELEASE: u8 = 0x80;
    /// The first of the six digits that set the sim speed.
    const FIRST_SPEED: Self = Self(2);
    /// The scancodes the window keys take.
    const Q: Self = Self(16);
    const Y: Self = Self(21);
    const I: Self = Self(23);
    const S: Self = Self(31);
    const H: Self = Self(35);
    const L: Self = Self(38);
    const P: Self = Self(25);
    const Z: Self = Self(44);
    const C: Self = Self(46);
    const V: Self = Self(47);
    const B: Self = Self(48);
    const M: Self = Self(50);
    const SPACE: Self = Self(57);
    const F1: Self = Self(59);
    const F2: Self = Self(60);
    const F7: Self = Self(66);
    const F8: Self = Self(67);
    const F9: Self = Self(68);
    const F10: Self = Self(69);
    const F11: Self = Self(87);
    const F12: Self = Self(88);
    /// The keypad keys that repeat the keys for the other view. Keypad 4 and 5 repeat `n` and
    /// `l`, which are not bound here.
    const PAD_6: Self = Self(77);
    const PAD_1: Self = Self(79);
    const PAD_2: Self = Self(80);
    const PAD_3: Self = Self(81);
    /// The four keys that scroll.
    const SCROLL_UP: Self = Self(102);
    const SCROLL_LEFT: Self = Self(104);
    const SCROLL_RIGHT: Self = Self(105);
    const SCROLL_DOWN: Self = Self(107);
}

/// How many sim steps a frame runs, in the order the six keys set them (`0x4403c` onwards).
pub const SPEEDS: [u16; 6] = [1, 2, 5, 10, 25, 100];

/// How far the scroll keys move the view, one way and the other.
const SCROLL: (i32, i32) = (-160, 96);

/// The draw flags the keys turn on and off (`0x1f700`).
pub mod flag {
    /// What the space bar turns on and off.
    pub const SPACE: u16 = 1 << 0;
    /// What `F7` turns on and off, which needs the cheat word.
    pub const F7: u16 = 1 << 7;
    /// What `c` turns on and off.
    pub const C: u16 = 1 << 10;
    /// What `s` turns on and off.
    pub const S: u16 = 1 << 11;
}

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Run this many sim steps a frame.
    Speed(u16),
    /// Step between one and two sim steps a frame.
    StepSpeed,
    /// Open a window of this kind, or close it if it is open already.
    Ask(WindowId),
    /// Open a window of this kind.
    Open(WindowId),
    /// Close one kind and open another, which is how the two reading screens replace each other.
    Swap {
        /// The kind that closes.
        close: WindowId,
        /// The kind that opens.
        open: WindowId,
    },
    /// Turn one of the map's draw flags on or off.
    Flag(u16),
    /// Step the map's two-state word.
    Mode,
    /// Move the view by this much.
    Scroll {
        /// Across.
        across: i32,
        /// Down.
        down: i32,
    },
    /// Move the view to the player's own headquarters.
    Headquarters,
    /// Move the view to the node it remembers.
    Remembered,
    /// Turn the pause on or off.
    Pause,
}

/// Which view a key acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// The view the key arrived through.
    #[default]
    Own,
    /// The second view, which the keypad keys act on.
    Other,
}

/// A key and the view it acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bound {
    /// What it does.
    pub action: Action,
    /// Which view it does it to.
    pub view: View,
}

impl Bound {
    /// The action on the view the key arrived through.
    const fn own(action: Action) -> Self {
        Self {
            action,
            view: View::Own,
        }
    }

    /// The action on the second view.
    const fn other(action: Action) -> Self {
        Self {
            action,
            view: View::Other,
        }
    }
}

/// What `key` does with `alt` held and the cheat word `cheat` either entered or not. A key the
/// original leaves to the view's own handler gives `None`.
#[must_use]
pub fn bound(key: Scancode, alt: bool, cheat: bool) -> Option<Bound> {
    if let Some(place) = key.0.checked_sub(Scancode::FIRST_SPEED.0).map(usize::from)
        && let Some(&speed) = SPEEDS.get(place)
    {
        return (alt && cheat).then_some(Bound::own(Action::Speed(speed)));
    }
    let action = match key {
        Scancode::Q => return (alt).then_some(Bound::own(Action::Open(kind::QUIT))),
        Scancode::Y | Scancode::Z => Action::Mode,
        Scancode::I => Action::Ask(kind::HELP_LINE),
        Scancode::S => Action::Flag(flag::S),
        Scancode::H => Action::Headquarters,
        Scancode::L => Action::Ask(kind::MAP),
        Scancode::P => Action::Pause,
        Scancode::C => Action::Flag(flag::C),
        Scancode::V => Action::StepSpeed,
        Scancode::B => Action::Remembered,
        Scancode::M => Action::Ask(kind::SELECT),
        Scancode::SPACE => Action::Flag(flag::SPACE),
        Scancode::F1 => Action::Swap {
            close: kind::SAVE_GAME,
            open: kind::LOAD_GAME,
        },
        Scancode::F2 => Action::Swap {
            close: kind::LOAD_GAME,
            open: kind::SAVE_GAME,
        },
        Scancode::F7 => return cheat.then_some(Bound::own(Action::Flag(flag::F7))),
        Scancode::F8 => Action::Ask(kind::KEYS),
        Scancode::F9 => Action::Ask(kind::README),
        Scancode::F10 => Action::Ask(kind::CD_PLAYER),
        Scancode::F11 => Action::Ask(kind::MIDI_PLAYER),
        Scancode::F12 => Action::Ask(kind::MENU),
        Scancode::SCROLL_UP => Action::Scroll {
            across: 0,
            down: SCROLL.0,
        },
        Scancode::SCROLL_DOWN => Action::Scroll {
            across: 0,
            down: SCROLL.1,
        },
        Scancode::SCROLL_LEFT => Action::Scroll {
            across: SCROLL.0,
            down: 0,
        },
        Scancode::SCROLL_RIGHT => Action::Scroll {
            across: SCROLL.1,
            down: 0,
        },
        Scancode::PAD_1 => return Some(Bound::other(Action::Flag(flag::SPACE))),
        Scancode::PAD_2 => return Some(Bound::other(Action::Flag(flag::C))),
        Scancode::PAD_3 => return Some(Bound::other(Action::Flag(flag::S))),
        Scancode::PAD_6 => return Some(Bound::other(Action::Mode)),
        _ => return None,
    };
    Some(Bound::own(action))
}

/// How many keys the original remembers for the cheat word.
const REMEMBERED: usize = 10;

/// What the newest key is overwritten with once the word matches, so one run cannot match twice.
const SPENT: u8 = Scancode::B.0;

/// The keys the cheat word is spelled with, newest first (`0x1fbdac`). As set 1 scancodes they
/// spell the word this edition takes; other editions take another.
pub const CHEAT_WORD: [u8; 6] = [19, 18, 20, 49, 23, 17];

/// The last keys pressed, which the cheat word is matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cheat {
    recent: [u8; REMEMBERED],
    on: bool,
}

impl Cheat {
    /// Nothing typed and the cheat off.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            recent: [0; REMEMBERED],
            on: false,
        }
    }

    /// Whether the cheat word has been typed an odd number of times.
    #[must_use]
    pub const fn on(&self) -> bool {
        self.on
    }

    /// Takes `key` and gives back whether the cheat word just matched, which is where the original
    /// plays effect 58. Release codes do not count.
    pub fn press(&mut self, key: Scancode) -> bool {
        let Scancode(code) = key;
        if code & Scancode::RELEASE != 0 {
            return false;
        }
        self.recent.rotate_right(1);
        self.recent[0] = code;
        if self.recent[..CHEAT_WORD.len()] != CHEAT_WORD {
            return false;
        }
        self.on = !self.on;
        self.recent[0] = SPENT;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(word: &[u8], cheat: &mut Cheat) -> bool {
        word.iter()
            .map(|&code| cheat.press(Scancode(code)))
            .last()
            .unwrap_or_default()
    }

    #[test]
    fn the_speed_keys_need_alt_and_the_cheat_word() {
        assert_eq!(bound(Scancode(2), true, true), Some(Bound::own(Action::Speed(1))));
        assert_eq!(
            bound(Scancode(7), true, true),
            Some(Bound::own(Action::Speed(100)))
        );
        assert_eq!(bound(Scancode(4), true, true), Some(Bound::own(Action::Speed(5))));
        assert_eq!(bound(Scancode(4), false, true), None, "alt is needed");
        assert_eq!(bound(Scancode(4), true, false), None, "the cheat word is needed");
        assert_eq!(
            bound(Scancode::V, false, false),
            Some(Bound::own(Action::StepSpeed)),
            "v needs neither"
        );
    }

    #[test]
    fn the_window_keys_name_their_kinds() {
        assert_eq!(
            bound(Scancode::F12, false, false),
            Some(Bound::own(Action::Ask(kind::MENU)))
        );
        assert_eq!(
            bound(Scancode::F10, false, false),
            Some(Bound::own(Action::Ask(kind::CD_PLAYER)))
        );
        assert_eq!(
            bound(Scancode::F1, false, false),
            Some(Bound::own(Action::Swap {
                close: kind::SAVE_GAME,
                open: kind::LOAD_GAME,
            }))
        );
        assert_eq!(bound(Scancode::Q, false, false), None, "q wants alt");
        assert_eq!(
            bound(Scancode::Q, true, false),
            Some(Bound::own(Action::Open(kind::QUIT)))
        );
    }

    #[test]
    fn the_keypad_repeats_six_keys_for_the_other_view() {
        assert_eq!(
            bound(Scancode::PAD_1, false, false),
            Some(Bound::other(Action::Flag(flag::SPACE)))
        );
        assert_eq!(
            bound(Scancode::PAD_6, false, false),
            Some(Bound::other(Action::Mode))
        );
        assert_eq!(
            bound(Scancode::SPACE, false, false),
            Some(Bound::own(Action::Flag(flag::SPACE)))
        );
    }

    #[test]
    fn the_scroll_keys_move_further_one_way_than_the_other() {
        assert_eq!(
            bound(Scancode::SCROLL_UP, false, false),
            Some(Bound::own(Action::Scroll {
                across: 0,
                down: -160
            }))
        );
        assert_eq!(
            bound(Scancode::SCROLL_RIGHT, false, false),
            Some(Bound::own(Action::Scroll { across: 96, down: 0 }))
        );
    }

    #[test]
    fn the_cheat_word_flips_the_cheat_and_cannot_match_twice_over() {
        let mut cheat = Cheat::new();
        assert!(!cheat.on());
        let word: Vec<u8> = CHEAT_WORD.iter().rev().copied().collect();
        assert!(press(&word, &mut cheat), "the word matched");
        assert!(cheat.on());
        assert!(!cheat.press(Scancode(CHEAT_WORD[0])), "the newest key is spent");
        assert!(cheat.on());
        assert!(press(&word, &mut cheat));
        assert!(!cheat.on(), "a second run turns it off again");
    }

    #[test]
    fn keys_the_view_handles_itself_are_not_bound() {
        for code in [8, 12, 28, 61, 63, 108] {
            assert_eq!(bound(Scancode(code), false, true), None, "{code}");
        }
    }
}
