use std::ffi::c_int;
use std::ptr::NonNull;
use std::rc::Rc;

use sdl3_sys::mouse::{SDL_CreateColorCursor, SDL_Cursor, SDL_DestroyCursor};
use sdl3_sys::pixels::SDL_PIXELFORMAT_RGBA32;
use sdl3_sys::surface::{SDL_CreateSurfaceFrom, SDL_DestroySurface};

use crate::{Context, Error, Result, sdl_error};

/// Bytes a pixel of a cursor's picture takes: red, green, blue and alpha.
const RGBA: u32 = 4;

/// A colour mouse cursor. SDL draws it itself, so it follows the mouse without waiting for a
/// frame.
#[derive(Debug)]
pub struct Cursor {
    raw: NonNull<SDL_Cursor>,
    _context: Rc<Context>,
}

impl Cursor {
    pub(crate) fn new(
        context: Rc<Context>,
        pixels: &[u8],
        size: (u32, u32),
        hotspot: (u32, u32),
    ) -> Result<Self> {
        let (width, height) = size;
        let fits = |value: u32| c_int::try_from(value).ok();
        let (Some(w), Some(h), Some(pitch), Some(x), Some(y)) = (
            fits(width),
            fits(height),
            fits(width.saturating_mul(RGBA)),
            fits(hotspot.0),
            fits(hotspot.1),
        ) else {
            return Err(Error::Sdl("cursor too large".into()));
        };
        let expected = width
            .checked_mul(height)
            .and_then(|area| area.checked_mul(RGBA))
            .and_then(|bytes| usize::try_from(bytes).ok());
        if width == 0
            || height == 0
            || expected != Some(pixels.len())
            || hotspot.0 >= width
            || hotspot.1 >= height
        {
            return Err(Error::Sdl("cursor picture and hotspot do not agree".into()));
        }
        let mut copy = pixels.to_vec();
        // SAFETY: `copy` holds `height` rows of `pitch` bytes and outlives the surface, which is
        // destroyed before this function returns.
        let surface =
            unsafe { SDL_CreateSurfaceFrom(w, h, SDL_PIXELFORMAT_RGBA32, copy.as_mut_ptr().cast(), pitch) };
        if surface.is_null() {
            return Err(sdl_error());
        }
        // SAFETY: `surface` is valid; SDL copies its pixels into the cursor.
        let raw = unsafe { SDL_CreateColorCursor(surface, x, y) };
        // SAFETY: `surface` came from SDL_CreateSurfaceFrom and nothing else holds it.
        unsafe { SDL_DestroySurface(surface) };
        let raw = NonNull::new(raw).ok_or_else(sdl_error)?;
        Ok(Self {
            raw,
            _context: context,
        })
    }

    pub(crate) const fn raw(&self) -> *mut SDL_Cursor {
        self.raw.as_ptr()
    }
}

impl Drop for Cursor {
    fn drop(&mut self) {
        // SAFETY: the cursor came from SDL_CreateColorCursor and is destroyed once. SDL falls
        // back to its default cursor if this one is still set.
        unsafe { SDL_DestroyCursor(self.raw.as_ptr()) };
    }
}
