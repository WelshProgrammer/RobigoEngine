use std::ffi::{CStr, CString, c_char, c_int, c_ulong, c_void};
use std::num::NonZeroIsize;
use std::path::PathBuf;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle,
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, Win32WindowHandle,
    WindowHandle, WindowsDisplayHandle, XlibDisplayHandle, XlibWindowHandle,
};
use sdl3_sys::dialog::SDL_ShowOpenFolderDialog;
use sdl3_sys::events::SDL_PumpEvents;
use sdl3_sys::messagebox::{
    SDL_MESSAGEBOX_BUTTON_ESCAPEKEY_DEFAULT, SDL_MESSAGEBOX_BUTTON_RETURNKEY_DEFAULT,
    SDL_MESSAGEBOX_INFORMATION, SDL_MESSAGEBOX_WARNING, SDL_MessageBoxButtonData, SDL_MessageBoxData,
    SDL_ShowMessageBox, SDL_ShowSimpleMessageBox,
};
use sdl3_sys::metal::{SDL_Metal_CreateView, SDL_Metal_DestroyView};
use sdl3_sys::mouse::SDL_WarpMouseInWindow;
use sdl3_sys::pixels::SDL_PIXELFORMAT_RGBA32;
use sdl3_sys::properties::{SDL_GetNumberProperty, SDL_GetPointerProperty};
use sdl3_sys::surface::{SDL_CreateSurfaceFrom, SDL_DestroySurface};
use sdl3_sys::video::{
    SDL_CreateWindow, SDL_DestroyWindow, SDL_GetWindowPixelDensity, SDL_GetWindowProperties,
    SDL_GetWindowSize, SDL_GetWindowSizeInPixels, SDL_PROP_WINDOW_WAYLAND_DISPLAY_POINTER,
    SDL_PROP_WINDOW_WAYLAND_SURFACE_POINTER, SDL_PROP_WINDOW_WIN32_HWND_POINTER,
    SDL_PROP_WINDOW_WIN32_INSTANCE_POINTER, SDL_PROP_WINDOW_X11_DISPLAY_POINTER,
    SDL_PROP_WINDOW_X11_SCREEN_NUMBER, SDL_PROP_WINDOW_X11_WINDOW_NUMBER, SDL_SetWindowFullscreen,
    SDL_SetWindowIcon, SDL_SetWindowMinimumSize, SDL_SetWindowSize, SDL_SetWindowTitle,
    SDL_WINDOW_HIGH_PIXEL_DENSITY, SDL_WINDOW_RESIZABLE, SDL_Window,
};

use crate::{Context, Error, Result, sdl_error};

/// Native handles captured when the window is created, in the form raw-window-handle wants.
#[derive(Debug, Clone, Copy)]
enum Native {
    Xlib {
        display: Option<NonNull<c_void>>,
        screen: i32,
        window: c_ulong,
    },
    Wayland {
        display: NonNull<c_void>,
        surface: NonNull<c_void>,
    },
    Win32 {
        hwnd: NonZeroIsize,
        hinstance: Option<NonZeroIsize>,
    },
    /// The Metal view SDL makes over a macOS window, which is an `NSView`. The window owns it
    /// and destroys it when it drops.
    AppKit { view: NonNull<c_void> },
}

/// An SDL window. Implements raw-window-handle's traits so a renderer can create a surface on it.
#[derive(Debug)]
pub struct Window {
    raw: NonNull<SDL_Window>,
    native: Native,
    fullscreen: bool,
    _context: Rc<Context>,
}

impl Window {
    pub(crate) fn new(
        context: Rc<Context>,
        driver: &str,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let title = CString::new(title).map_err(|_| Error::Sdl("window title contains NUL".into()))?;
        let flags = SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIGH_PIXEL_DENSITY;
        // SAFETY: valid NUL-terminated title and flags; SDL is initialised while `context` lives.
        let raw = unsafe { SDL_CreateWindow(title.as_ptr(), clamp_i32(width), clamp_i32(height), flags) };
        let raw = NonNull::new(raw).ok_or_else(sdl_error)?;
        let Some(native) = native_handles(raw, driver) else {
            // SAFETY: this function owns `raw` and nothing else references it yet.
            unsafe { SDL_DestroyWindow(raw.as_ptr()) };
            return Err(Error::Sdl(format!(
                "no native window handle for video driver '{driver}'"
            )));
        };
        Ok(Self {
            raw,
            native,
            fullscreen: false,
            _context: context,
        })
    }

    /// The size in window coordinates, the units [`crate::Platform::create_window`] takes.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        let (mut w, mut h) = (0, 0);
        // SAFETY: `raw` is a live window and the out-pointers are valid.
        unsafe { SDL_GetWindowSize(self.raw.as_ptr(), &raw mut w, &raw mut h) };
        (u32::try_from(w).unwrap_or(0), u32::try_from(h).unwrap_or(0))
    }

    /// The drawable size in physical pixels, which differs from the logical size on high-DPI displays.
    #[must_use]
    pub fn size_in_pixels(&self) -> (u32, u32) {
        let (mut w, mut h) = (0, 0);
        // SAFETY: `raw` is a live window and the out-pointers are valid.
        unsafe { SDL_GetWindowSizeInPixels(self.raw.as_ptr(), &raw mut w, &raw mut h) };
        (u32::try_from(w).unwrap_or(0), u32::try_from(h).unwrap_or(0))
    }

    /// Pixels per window coordinate: 1 on ordinary displays, 2 on a typical high-DPI one. Mouse
    /// events arrive in window coordinates; multiply by this to get pixels.
    #[must_use]
    pub fn pixel_density(&self) -> f32 {
        // SAFETY: `raw` is a live window.
        let density = unsafe { SDL_GetWindowPixelDensity(self.raw.as_ptr()) };
        if density > 0.0 { density } else { 1.0 }
    }

    /// Moves the pointer to (`x`, `y`) in window coordinates, the units mouse events arrive in.
    /// A platform that refuses to move the pointer, such as Wayland without a pointer
    /// constraint, leaves it where it is.
    pub fn warp_pointer(&self, x: f32, y: f32) {
        // SAFETY: `raw` is a live window.
        unsafe { SDL_WarpMouseInWindow(self.raw.as_ptr(), x, y) };
    }

    /// Gives the window the icon `rgba` holds, `width` pixels a row of four bytes each: red,
    /// green, blue and alpha. A desktop that takes no icon from a window keeps its own.
    ///
    /// # Errors
    ///
    /// Returns an error when `rgba` is not `width` by `height` pixels, or SDL's own.
    pub fn set_icon(&self, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
        const BYTES: u32 = 4;
        let pitch = width.saturating_mul(BYTES);
        if width == 0 || u64::from(pitch) * u64::from(height) != rgba.len() as u64 {
            return Err(Error::Sdl("the icon's pixels do not fit its size".into()));
        }
        let mut pixels = rgba.to_vec();
        // SAFETY: `pixels` holds `height` rows of `pitch` bytes and outlives the surface, which
        // is destroyed below. SDL copies the icon before SDL_SetWindowIcon returns.
        unsafe {
            let surface = SDL_CreateSurfaceFrom(
                clamp_i32(width),
                clamp_i32(height),
                SDL_PIXELFORMAT_RGBA32,
                pixels.as_mut_ptr().cast(),
                clamp_i32(pitch),
            );
            if surface.is_null() {
                return Err(sdl_error());
            }
            let set = SDL_SetWindowIcon(self.raw.as_ptr(), surface);
            SDL_DestroySurface(surface);
            if set { Ok(()) } else { Err(sdl_error()) }
        }
    }

    /// Stops the player resizing the window below `width` by `height` in window coordinates.
    pub fn set_minimum_size(&self, width: u32, height: u32) {
        // SAFETY: `raw` is a live window.
        unsafe { SDL_SetWindowMinimumSize(self.raw.as_ptr(), clamp_i32(width), clamp_i32(height)) };
    }

    /// Asks for the window to be `width` by `height` in window coordinates. A fullscreen window
    /// keeps filling the screen; the window manager may hold the size to what fits.
    pub fn set_size(&self, width: u32, height: u32) {
        // SAFETY: `raw` is a live window.
        unsafe { SDL_SetWindowSize(self.raw.as_ptr(), clamp_i32(width), clamp_i32(height)) };
    }

    /// Asks the player `message` in the system's own dialog over the window, titled `title`, with
    /// the buttons `yes` and `no` and the icon `tone` names. Return takes `yes` and Escape `no`.
    /// True for `yes`.
    ///
    /// # Errors
    ///
    /// Returns SDL's error if the dialog cannot be shown.
    pub fn ask(&self, tone: Tone, title: &str, message: &str, yes: &str, no: &str) -> Result<bool> {
        let text = |text: &str| CString::new(text.replace('\0', "")).unwrap_or_default();
        let (title, message, yes, no) = (text(title), text(message), text(yes), text(no));
        let buttons = [
            SDL_MessageBoxButtonData {
                flags: SDL_MESSAGEBOX_BUTTON_ESCAPEKEY_DEFAULT,
                buttonID: 0,
                text: no.as_ptr(),
            },
            SDL_MessageBoxButtonData {
                flags: SDL_MESSAGEBOX_BUTTON_RETURNKEY_DEFAULT,
                buttonID: 1,
                text: yes.as_ptr(),
            },
        ];
        let data = SDL_MessageBoxData {
            flags: match tone {
                Tone::Information => SDL_MESSAGEBOX_INFORMATION,
                Tone::Warning => SDL_MESSAGEBOX_WARNING,
            },
            window: self.raw.as_ptr(),
            title: title.as_ptr(),
            message: message.as_ptr(),
            numbuttons: 2,
            buttons: buttons.as_ptr(),
            colorScheme: ptr::null(),
        };
        let mut pressed = 0;
        // SAFETY: `data` and the strings and buttons it points at outlive the call, and `raw` is a
        // live window.
        if !unsafe { SDL_ShowMessageBox(&raw const data, &raw mut pressed) } {
            return Err(sdl_error());
        }
        Ok(pressed == 1)
    }

    /// Tells the player `message` in the system's own dialog over the window, titled `title`,
    /// with one button that closes it.
    ///
    /// # Errors
    ///
    /// Returns SDL's error if the dialog cannot be shown.
    pub fn tell(&self, tone: Tone, title: &str, message: &str) -> Result<()> {
        let text = |text: &str| CString::new(text.replace('\0', "")).unwrap_or_default();
        let (title, message) = (text(title), text(message));
        let flags = match tone {
            Tone::Information => SDL_MESSAGEBOX_INFORMATION,
            Tone::Warning => SDL_MESSAGEBOX_WARNING,
        };
        // SAFETY: the strings outlive the call, and `raw` is a live window.
        if !unsafe { SDL_ShowSimpleMessageBox(flags, title.as_ptr(), message.as_ptr(), self.raw.as_ptr()) } {
            return Err(sdl_error());
        }
        Ok(())
    }

    /// Has the player pick a folder in the system's own folder dialog over the window, and waits
    /// for the answer. `None` when the player closes the dialog without picking.
    ///
    /// # Errors
    ///
    /// Returns SDL's error if the system has no folder dialog to show.
    pub fn pick_folder(&self) -> Result<Option<PathBuf>> {
        let answer: Mutex<Option<Picked>> = Mutex::new(None);
        // SAFETY: `raw` is a live window, and `answer` outlives the dialog: SDL calls `picked`
        // once, and the loop below waits for that call before `answer` is dropped.
        unsafe {
            SDL_ShowOpenFolderDialog(
                Some(picked),
                (&raw const answer).cast_mut().cast(),
                self.raw.as_ptr(),
                ptr::null(),
                false,
            );
        }
        loop {
            // SAFETY: the window's thread is the one that started SDL.
            unsafe { SDL_PumpEvents() };
            let taken = answer.lock().ok().and_then(|mut answer| answer.take());
            match taken {
                Some(Picked::Folder(folder)) => return Ok(Some(folder)),
                Some(Picked::Nothing) => return Ok(None),
                Some(Picked::Failed(error)) => return Err(error),
                None => std::thread::sleep(PICKER_POLL),
            }
        }
    }

    /// Names the window `title`. A title holding a NUL byte is cut at it.
    pub fn set_title(&self, title: &str) {
        let kept = title.split('\0').next().unwrap_or_default();
        let Ok(title) = CString::new(kept) else {
            return;
        };
        // SAFETY: `raw` is a live window and `title` is a valid NUL-terminated string.
        unsafe { SDL_SetWindowTitle(self.raw.as_ptr(), title.as_ptr()) };
    }

    /// True while the window is in borderless fullscreen.
    #[must_use]
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Switches between windowed and borderless desktop fullscreen.
    ///
    /// # Errors
    ///
    /// Returns SDL's error if the window manager refuses the change.
    pub fn set_fullscreen(&mut self, fullscreen: bool) -> Result<()> {
        // SAFETY: `raw` is a live window.
        if !unsafe { SDL_SetWindowFullscreen(self.raw.as_ptr(), fullscreen) } {
            return Err(sdl_error());
        }
        self.fullscreen = fullscreen;
        Ok(())
    }
}

/// How long [`Window::pick_folder`] sleeps between looks for the answer.
const PICKER_POLL: Duration = Duration::from_millis(10);

/// What the folder dialog came back with.
enum Picked {
    Folder(PathBuf),
    Nothing,
    Failed(Error),
}

/// SDL's callback for the folder dialog: `userdata` is the `Mutex<Option<Picked>>` that
/// [`Window::pick_folder`] waits on, and `list` is null for an error or a null-terminated list of
/// the paths picked, empty when the player picked none.
unsafe extern "C" fn picked(userdata: *mut c_void, list: *const *const c_char, _filter: c_int) {
    // SAFETY: SDL passes a null list or a null-terminated one whose strings are valid for the
    // call.
    let answer = unsafe {
        if list.is_null() {
            Picked::Failed(sdl_error())
        } else if (*list).is_null() {
            Picked::Nothing
        } else {
            Picked::Folder(PathBuf::from(
                CStr::from_ptr(*list).to_string_lossy().into_owned(),
            ))
        }
    };
    // SAFETY: `userdata` is the mutex `pick_folder` passed, which it keeps until this has run.
    let slot = unsafe { &*userdata.cast::<Mutex<Option<Picked>>>() };
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(answer);
    }
}

/// The icon a dialog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A question or a piece of news.
    Information,
    /// Something went wrong.
    Warning,
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: this window owns the Metal view and the SDL window, and its reference keeps the
        // context alive past these calls.
        unsafe {
            if let Native::AppKit { view } = self.native {
                SDL_Metal_DestroyView(view.as_ptr());
            }
            SDL_DestroyWindow(self.raw.as_ptr());
        }
    }
}

fn clamp_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn native_handles(window: NonNull<SDL_Window>, driver: &str) -> Option<Native> {
    // SAFETY: `window` is live; property lookups return NULL or 0 for missing keys.
    unsafe {
        let props = SDL_GetWindowProperties(window.as_ptr());
        match driver {
            "x11" => {
                let window = SDL_GetNumberProperty(props, SDL_PROP_WINDOW_X11_WINDOW_NUMBER, 0);
                let window = c_ulong::try_from(window).ok().filter(|&w| w != 0)?;
                Some(Native::Xlib {
                    display: NonNull::new(SDL_GetPointerProperty(
                        props,
                        SDL_PROP_WINDOW_X11_DISPLAY_POINTER,
                        ptr::null_mut(),
                    )),
                    screen: i32::try_from(SDL_GetNumberProperty(props, SDL_PROP_WINDOW_X11_SCREEN_NUMBER, 0))
                        .unwrap_or(0),
                    window,
                })
            }
            "wayland" => Some(Native::Wayland {
                display: NonNull::new(SDL_GetPointerProperty(
                    props,
                    SDL_PROP_WINDOW_WAYLAND_DISPLAY_POINTER,
                    ptr::null_mut(),
                ))?,
                surface: NonNull::new(SDL_GetPointerProperty(
                    props,
                    SDL_PROP_WINDOW_WAYLAND_SURFACE_POINTER,
                    ptr::null_mut(),
                ))?,
            }),
            "windows" => Some(Native::Win32 {
                hwnd: NonZeroIsize::new(SDL_GetPointerProperty(
                    props,
                    SDL_PROP_WINDOW_WIN32_HWND_POINTER,
                    ptr::null_mut(),
                ) as isize)?,
                hinstance: NonZeroIsize::new(SDL_GetPointerProperty(
                    props,
                    SDL_PROP_WINDOW_WIN32_INSTANCE_POINTER,
                    ptr::null_mut(),
                ) as isize),
            }),
            "cocoa" => Some(Native::AppKit {
                view: NonNull::new(SDL_Metal_CreateView(window.as_ptr()))?,
            }),
            _ => None,
        }
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let raw = match self.native {
            Native::Xlib { window, .. } => RawWindowHandle::Xlib(XlibWindowHandle::new(window)),
            Native::Wayland { surface, .. } => RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
            Native::Win32 { hwnd, hinstance } => {
                let mut handle = Win32WindowHandle::new(hwnd);
                handle.hinstance = hinstance;
                RawWindowHandle::Win32(handle)
            }
            Native::AppKit { view } => RawWindowHandle::AppKit(AppKitWindowHandle::new(view)),
        };
        // SAFETY: the handle stays valid for as long as `self` (and so the SDL window) is alive.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        let raw = match self.native {
            Native::Xlib { display, screen, .. } => {
                RawDisplayHandle::Xlib(XlibDisplayHandle::new(display, screen))
            }
            Native::Wayland { display, .. } => RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
            Native::Win32 { .. } => RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            Native::AppKit { .. } => RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
        };
        // SAFETY: the display connection outlives every window SDL creates on it.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}
