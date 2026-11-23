//! Thin safe layer over SDL3 for windows, input and the sound card.
//!
//! Everything SDL-specific stays in this crate so the rest of the game can swap to another backend by
//! replacing it. Robigo calls SDL from one thread only, so [`Platform`] and [`Window`] are `!Send`.

mod audio;
mod cursor;
mod event;
mod window;

use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::rc::Rc;

use sdl3_sys::events::{SDL_Event, SDL_PollEvent};
use sdl3_sys::init::{SDL_INIT_EVENTS, SDL_INIT_VIDEO, SDL_Init, SDL_Quit, SDL_SetAppMetadata};
use sdl3_sys::mouse::{SDL_HideCursor, SDL_SetCursor, SDL_ShowCursor};
use sdl3_sys::video::SDL_GetCurrentVideoDriver;

pub use audio::AudioOut;
pub use cursor::Cursor;
pub use event::{Event, Key, MouseButton, Scan};
pub use window::{Tone, Window};

/// Errors raised by the platform layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SDL reported a failure; the message is SDL's own.
    #[error("SDL: {0}")]
    Sdl(String),
}

/// Result type for platform calls.
pub type Result<T> = std::result::Result<T, Error>;

/// The folder this user's preferences for `app` belong in, created if it was missing:
/// `~/.local/share/<app>/` on Linux, `%APPDATA%\<app>\` on Windows and
/// `~/Library/Application Support/<app>/` on macOS. `None` when the platform cannot name or
/// create one.
#[must_use]
pub fn preferences_dir(app: &str) -> Option<PathBuf> {
    let app = CString::new(app).ok()?;
    // SAFETY: SDL allows an empty organisation, and `app` is a valid NUL-terminated string. The
    // answer is a string SDL allocates, read once and then freed here.
    unsafe {
        let raw = sdl3_sys::filesystem::SDL_GetPrefPath(c"".as_ptr(), app.as_ptr());
        if raw.is_null() {
            return None;
        }
        let path = CStr::from_ptr(raw).to_string_lossy().into_owned();
        sdl3_sys::stdinc::SDL_free(raw.cast());
        Some(PathBuf::from(path))
    }
}

fn sdl_error() -> Error {
    // SAFETY: SDL_GetError always returns a valid, NUL-terminated string owned by SDL.
    let message = unsafe { CStr::from_ptr(sdl3_sys::error::SDL_GetError()) };
    Error::Sdl(message.to_string_lossy().into_owned())
}

/// Shuts SDL down once the platform and every window created from it have been dropped.
#[derive(Debug)]
struct Context {
    _not_send: PhantomData<*const ()>,
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the last owner of the context is going away, so no SDL objects remain in use.
        unsafe { SDL_Quit() };
    }
}

/// The initialised SDL video and event subsystems, and audio once a device is opened.
#[derive(Debug)]
pub struct Platform {
    context: Rc<Context>,
}

impl Platform {
    /// Initialises SDL's video and event subsystems. Call once, from the main thread.
    ///
    /// `app` names the program to the desktop: it is the Wayland app id and the X11 window
    /// class, which a desktop matches to `<app>.desktop` for the window's icon, and the name a
    /// sound mixer shows.
    ///
    /// # Errors
    ///
    /// Returns SDL's error if `app` holds a NUL or no video driver can start (no display,
    /// missing libraries).
    pub fn init(app: &str) -> Result<Self> {
        let app = CString::new(app).map_err(|error| Error::Sdl(error.to_string()))?;
        // SAFETY: both strings are valid and NUL-terminated, SDL copies them, and it allows no
        // version.
        if !unsafe { SDL_SetAppMetadata(app.as_ptr(), std::ptr::null(), app.as_ptr()) } {
            return Err(sdl_error());
        }
        // SAFETY: plain FFI call with valid flags.
        if !unsafe { SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS) } {
            return Err(sdl_error());
        }
        Ok(Self {
            context: Rc::new(Context {
                _not_send: PhantomData,
            }),
        })
    }

    /// The name of SDL's active video driver, such as `x11`, `wayland` or `windows`.
    #[must_use]
    pub fn video_driver(&self) -> String {
        // SAFETY: returns NULL or a static NUL-terminated string.
        let name = unsafe { SDL_GetCurrentVideoDriver() };
        if name.is_null() {
            return String::new();
        }
        // SAFETY: checked for NULL above.
        unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
    }

    /// Opens a resizable, high-DPI window of the given logical size.
    ///
    /// # Errors
    ///
    /// Returns an error if SDL can't create the window or expose a native handle for it.
    pub fn create_window(&self, title: &str, width: u32, height: u32) -> Result<Window> {
        Window::new(self.context.clone(), &self.video_driver(), title, width, height)
    }

    /// Opens the default playback device for 16-bit stereo at `rate` frames a second. SDL
    /// converts to the device's own format and rate.
    ///
    /// # Errors
    ///
    /// SDL's error when there is no audio driver or device, which a caller can treat as silence.
    pub fn open_audio(&self, rate: u32) -> Result<AudioOut> {
        AudioOut::open(self.context.clone(), rate)
    }

    /// A colour cursor from `pixels`, rows of red, green, blue and alpha bytes `size` pixels
    /// across and down, with the click point `hotspot` pixels in from its top-left corner.
    ///
    /// # Errors
    ///
    /// When the picture is empty, `pixels` is not `size` big, the hotspot lies outside the
    /// picture or SDL refuses the cursor.
    pub fn create_cursor(&self, pixels: &[u8], size: (u32, u32), hotspot: (u32, u32)) -> Result<Cursor> {
        Cursor::new(self.context.clone(), pixels, size, hotspot)
    }

    /// Makes `cursor` the one SDL shows.
    pub fn set_cursor(&self, cursor: &Cursor) {
        // SAFETY: `cursor` is a live cursor; SDL only reads it while it stays set.
        unsafe { SDL_SetCursor(cursor.raw()) };
    }

    /// Shows SDL's cursor, or hides it while the game draws its own.
    pub fn show_cursor(&self, show: bool) {
        // SAFETY: plain FFI calls with no arguments.
        unsafe {
            if show {
                SDL_ShowCursor();
            } else {
                SDL_HideCursor();
            }
        }
    }

    /// Returns the next pending event, or `None` when the queue is empty.
    pub fn poll_event(&mut self) -> Option<Event> {
        let mut raw = SDL_Event::default();
        // SAFETY: `raw` is a valid, writable SDL_Event.
        if unsafe { SDL_PollEvent(&raw mut raw) } {
            Some(Event::from_sdl(&raw))
        } else {
            None
        }
    }
}
