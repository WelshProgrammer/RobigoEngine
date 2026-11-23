use sdl3_sys::events::{SDL_Event, SDL_EventType};
use sdl3_sys::keycode::SDL_Keycode;
use sdl3_sys::mouse::{SDL_BUTTON_LEFT, SDL_BUTTON_MIDDLE, SDL_BUTTON_RIGHT, SDL_MouseWheelDirection};
use sdl3_sys::scancode::SDL_Scancode;

/// A key, identified by its layout-dependent keycode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(pub u32);

impl Key {
    /// Escape.
    pub const ESCAPE: Key = Key(SDL_Keycode::ESCAPE.0);
    /// Return / Enter.
    pub const RETURN: Key = Key(SDL_Keycode::RETURN.0);
    /// F2.
    pub const F2: Key = Key(SDL_Keycode::F2.0);
    /// F3.
    pub const F3: Key = Key(SDL_Keycode::F3.0);
    /// F11.
    pub const F11: Key = Key(SDL_Keycode::F11.0);
    /// F12.
    pub const F12: Key = Key(SDL_Keycode::F12.0);
    /// Left Alt.
    pub const LALT: Key = Key(SDL_Keycode::LALT.0);
    /// Right Alt.
    pub const RALT: Key = Key(SDL_Keycode::RALT.0);
    /// Left Shift.
    pub const LSHIFT: Key = Key(SDL_Keycode::LSHIFT.0);
    /// Right Shift.
    pub const RSHIFT: Key = Key(SDL_Keycode::RSHIFT.0);
    /// Left Ctrl.
    pub const LCTRL: Key = Key(SDL_Keycode::LCTRL.0);
    /// Right Ctrl.
    pub const RCTRL: Key = Key(SDL_Keycode::RCTRL.0);
    /// Tab.
    pub const TAB: Key = Key(SDL_Keycode::TAB.0);
    /// Left arrow.
    pub const LEFT: Key = Key(SDL_Keycode::LEFT.0);
    /// Right arrow.
    pub const RIGHT: Key = Key(SDL_Keycode::RIGHT.0);
    /// Up arrow.
    pub const UP: Key = Key(SDL_Keycode::UP.0);
    /// Down arrow.
    pub const DOWN: Key = Key(SDL_Keycode::DOWN.0);
    /// `+` on layouts that have it unshifted.
    pub const PLUS: Key = Key(SDL_Keycode::PLUS.0);
    /// `=`, which carries `+` on US layouts.
    pub const EQUALS: Key = Key(SDL_Keycode::EQUALS.0);
    /// `-`.
    pub const MINUS: Key = Key(SDL_Keycode::MINUS.0);
    /// Keypad `+`.
    pub const KP_PLUS: Key = Key(SDL_Keycode::KP_PLUS.0);
    /// Keypad `-`.
    pub const KP_MINUS: Key = Key(SDL_Keycode::KP_MINUS.0);
    /// The letter B.
    pub const B: Key = Key(SDL_Keycode::B.0);
    /// The letter F.
    pub const F: Key = Key(SDL_Keycode::F.0);
    /// The letter G.
    pub const G: Key = Key(SDL_Keycode::G.0);
    /// The letter P.
    pub const P: Key = Key(SDL_Keycode::P.0);
    /// The letter S.
    pub const S: Key = Key(SDL_Keycode::S.0);

    /// The digit a key on the main row types, 0 to 9, or `None` for any other key.
    #[must_use]
    pub fn digit(self) -> Option<u8> {
        let offset = self.0.checked_sub(SDL_Keycode::_0.0)?;
        u8::try_from(offset).ok().filter(|&d| d <= 9)
    }
}

/// A key by its place on the keyboard, which SDL reports as a USB usage code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scan(pub u32);

impl Scan {
    /// The set 1 scancode of each letter key, A to Z, which is the order SDL numbers them in.
    const LETTERS: [u8; 26] = [
        30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21,
        44,
    ];
    /// The set 1 scancode of the digit `1`, which the main row counts up from to `9`.
    const DIGIT_1: u8 = 2;
    /// The set 1 scancode of the digit `0`, which sits past `9` on the keyboard.
    const DIGIT_0: u8 = 11;
    /// The set 1 scancode of the space bar.
    const SPACE: u8 = 57;
    /// The set 1 scancode of the main Enter key.
    const ENTER: u8 = 28;
    /// The set 1 scancode of `F1`, which counts up to `F10`.
    const F1: u8 = 59;
    /// The set 1 scancodes of `F11` and `F12`, which sit apart from the other ten.
    const F11: [u8; 2] = [87, 88];
    /// The set 1 scancode of keypad `1`, with `2` and `3` beside it.
    const KEYPAD_1: u8 = 79;
    /// The same for keypad `4`, with `5` and `6` beside it.
    const KEYPAD_4: u8 = 75;
    /// The same for keypad `7`, with `8` and `9` beside it.
    const KEYPAD_7: u8 = 71;

    /// The set 1 scancode the same key sends on a PC/AT keyboard, which is what `S2.EXE` reads.
    ///
    /// Covers the letters, the main row's digits, the function keys, the space bar, the main
    /// Enter key and the keypad's digits. Any other key gives `None`.
    #[must_use]
    pub fn set1(self) -> Option<u8> {
        let place = |first: SDL_Scancode| u8::try_from(self.0.checked_sub(first.0.cast_unsigned())?).ok();
        if let Some(letter) = place(SDL_Scancode::A).filter(|&p| usize::from(p) < Self::LETTERS.len()) {
            return Some(Self::LETTERS[usize::from(letter)]);
        }
        if let Some(digit) = place(SDL_Scancode::_1).filter(|&p| p < 9) {
            return Some(Self::DIGIT_1 + digit);
        }
        if self.0 == SDL_Scancode::_0.0.cast_unsigned() {
            return Some(Self::DIGIT_0);
        }
        if self.0 == SDL_Scancode::SPACE.0.cast_unsigned() {
            return Some(Self::SPACE);
        }
        if self.0 == SDL_Scancode::RETURN.0.cast_unsigned() {
            return Some(Self::ENTER);
        }
        if let Some(key) = place(SDL_Scancode::F1).filter(|&p| p < 12) {
            return Some(match key {
                10 | 11 => Self::F11[usize::from(key) - 10],
                other => Self::F1 + other,
            });
        }
        let keypad = place(SDL_Scancode::KP_1).filter(|&p| p < 9)?;
        let row = [Self::KEYPAD_1, Self::KEYPAD_4, Self::KEYPAD_7];
        Some(row[usize::from(keypad / 3)] + keypad % 3)
    }
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// The primary button.
    Left,
    /// The wheel button.
    Middle,
    /// The secondary button.
    Right,
    /// Any other button, by SDL's number.
    Other(u8),
}

impl MouseButton {
    fn from_sdl(button: u8) -> Self {
        match i32::from(button) {
            SDL_BUTTON_LEFT => Self::Left,
            SDL_BUTTON_MIDDLE => Self::Middle,
            SDL_BUTTON_RIGHT => Self::Right,
            _ => Self::Other(button),
        }
    }
}

/// Input and window events the game cares about. Anything else arrives as [`Event::Other`].
///
/// Mouse positions and movements are in window coordinates, which differ from pixels on
/// high-DPI displays; see [`Window::pixel_density`](crate::Window::pixel_density).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// The user asked to quit (window closed, Alt+F4, system shutdown).
    Quit,
    /// The window's drawable size changed, in physical pixels.
    PixelSizeChanged {
        /// New width in pixels.
        width: u32,
        /// New height in pixels.
        height: u32,
    },
    /// A key went down.
    KeyDown {
        /// The key pressed.
        key: Key,
        /// Where that key sits on the keyboard.
        scan: Scan,
        /// True when this is an auto-repeat.
        repeat: bool,
    },
    /// A key was released.
    KeyUp {
        /// The key released.
        key: Key,
        /// Where that key sits on the keyboard.
        scan: Scan,
    },
    /// The mouse moved.
    MouseMotion {
        /// New x.
        x: f32,
        /// New y.
        y: f32,
        /// Movement across since the last motion event.
        dx: f32,
        /// Movement down since the last motion event.
        dy: f32,
    },
    /// A mouse button went down.
    MouseDown {
        /// The button.
        button: MouseButton,
        /// Cursor x.
        x: f32,
        /// Cursor y.
        y: f32,
    },
    /// A mouse button was released.
    MouseUp {
        /// The button.
        button: MouseButton,
        /// Cursor x.
        x: f32,
        /// Cursor y.
        y: f32,
    },
    /// The wheel turned.
    MouseWheel {
        /// Notches turned away from the user; negative towards. Natural scrolling is undone.
        amount: f32,
        /// Cursor x.
        x: f32,
        /// Cursor y.
        y: f32,
    },
    /// An event this crate does not translate yet.
    Other,
}

impl Event {
    pub(crate) fn from_sdl(raw: &SDL_Event) -> Self {
        // SAFETY: every SDL_Event variant starts with the 32-bit type tag.
        let kind = SDL_EventType(unsafe { raw.r#type });
        match kind {
            SDL_EventType::QUIT => Event::Quit,
            SDL_EventType::WINDOW_PIXEL_SIZE_CHANGED => {
                // SAFETY: the tag says this is a window event.
                let window = unsafe { raw.window };
                Event::PixelSizeChanged {
                    width: u32::try_from(window.data1).unwrap_or(0),
                    height: u32::try_from(window.data2).unwrap_or(0),
                }
            }
            SDL_EventType::KEY_DOWN => {
                // SAFETY: the tag says this is a keyboard event.
                let key = unsafe { raw.key };
                Event::KeyDown {
                    key: Key(key.key.0),
                    scan: Scan(key.scancode.0.cast_unsigned()),
                    repeat: key.repeat,
                }
            }
            SDL_EventType::KEY_UP => {
                // SAFETY: the tag says this is a keyboard event.
                let key = unsafe { raw.key };
                Event::KeyUp {
                    key: Key(key.key.0),
                    scan: Scan(key.scancode.0.cast_unsigned()),
                }
            }
            SDL_EventType::MOUSE_MOTION => {
                // SAFETY: the tag says this is a mouse motion event.
                let motion = unsafe { raw.motion };
                Event::MouseMotion {
                    x: motion.x,
                    y: motion.y,
                    dx: motion.xrel,
                    dy: motion.yrel,
                }
            }
            SDL_EventType::MOUSE_BUTTON_DOWN | SDL_EventType::MOUSE_BUTTON_UP => {
                // SAFETY: the tag says this is a mouse button event.
                let button = unsafe { raw.button };
                let (pressed, x, y) = (button.down, button.x, button.y);
                let button = MouseButton::from_sdl(button.button);
                if pressed {
                    Event::MouseDown { button, x, y }
                } else {
                    Event::MouseUp { button, x, y }
                }
            }
            SDL_EventType::MOUSE_WHEEL => {
                // SAFETY: the tag says this is a mouse wheel event.
                let wheel = unsafe { raw.wheel };
                let sign = if wheel.direction == SDL_MouseWheelDirection::FLIPPED {
                    -1.0
                } else {
                    1.0
                };
                Event::MouseWheel {
                    amount: wheel.y * sign,
                    x: wheel.mouse_x,
                    y: wheel.mouse_y,
                }
            }
            _ => Event::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_gives_the_set_1_scancode_of_its_place() {
        let scan = |code: SDL_Scancode| Scan(code.0.cast_unsigned()).set1();
        // The cheat word's keys, which S2.EXE reads as set 1 scancodes.
        assert_eq!(scan(SDL_Scancode::W), Some(17));
        assert_eq!(scan(SDL_Scancode::S), Some(31));
        assert_eq!(scan(SDL_Scancode::A), Some(30));
        assert_eq!(scan(SDL_Scancode::M), Some(50));
        assert_eq!(scan(SDL_Scancode::Z), Some(44));
        assert_eq!(scan(SDL_Scancode::_1), Some(2));
        assert_eq!(scan(SDL_Scancode::_9), Some(10));
        assert_eq!(scan(SDL_Scancode::_0), Some(11));
        assert_eq!(scan(SDL_Scancode::SPACE), Some(57));
        assert_eq!(scan(SDL_Scancode::F1), Some(59));
        assert_eq!(scan(SDL_Scancode::F10), Some(68));
        assert_eq!(scan(SDL_Scancode::F11), Some(87));
        assert_eq!(scan(SDL_Scancode::F12), Some(88));
        assert_eq!(scan(SDL_Scancode::KP_1), Some(79));
        assert_eq!(scan(SDL_Scancode::KP_3), Some(81));
        assert_eq!(scan(SDL_Scancode::KP_4), Some(75));
        assert_eq!(scan(SDL_Scancode::KP_6), Some(77));
        assert_eq!(scan(SDL_Scancode::KP_9), Some(73));
        assert_eq!(scan(SDL_Scancode::RETURN), Some(28));
        assert_eq!(scan(SDL_Scancode::ESCAPE), None);
        assert_eq!(scan(SDL_Scancode::LEFT), None);
        assert_eq!(Scan(0).set1(), None);
    }

    #[test]
    fn every_letter_takes_a_scancode_of_its_own() {
        let mut seen = Scan::LETTERS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), Scan::LETTERS.len());
    }
}
