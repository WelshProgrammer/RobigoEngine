//! The sound card: one stream of 16-bit stereo at whatever rate the caller makes it, which SDL
//! converts to the device's own format and rate.

use std::ffi::c_int;
use std::ptr::{self, NonNull};
use std::rc::Rc;

use sdl3_sys::audio::{
    SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, SDL_AUDIO_S16, SDL_AudioSpec, SDL_AudioStream, SDL_DestroyAudioStream,
    SDL_GetAudioStreamQueued, SDL_OpenAudioDeviceStream, SDL_PutAudioStreamData, SDL_ResumeAudioStreamDevice,
};
use sdl3_sys::init::{SDL_INIT_AUDIO, SDL_InitSubSystem};

use crate::{Context, Error, Result, sdl_error};

/// Channels the stream carries, left then right.
const CHANNELS: usize = 2;

/// Bytes in one frame of the stream.
const FRAME_BYTES: usize = CHANNELS * size_of::<i16>();

/// The default playback device, fed 16-bit stereo at a fixed rate.
#[derive(Debug)]
pub struct AudioOut {
    stream: NonNull<SDL_AudioStream>,
    _context: Rc<Context>,
}

impl AudioOut {
    pub(crate) fn open(context: Rc<Context>, rate: u32) -> Result<Self> {
        // SAFETY: plain FFI call with a valid flag; SDL counts subsystem starts.
        if !unsafe { SDL_InitSubSystem(SDL_INIT_AUDIO) } {
            return Err(sdl_error());
        }
        let spec = SDL_AudioSpec {
            format: SDL_AUDIO_S16,
            channels: c_int::try_from(CHANNELS).unwrap_or(2),
            freq: c_int::try_from(rate).map_err(|_| Error::Sdl(format!("rate {rate} too high")))?,
        };
        // SAFETY: `spec` is a valid spec that SDL copies; no callback, so SDL pulls only what is
        // pushed.
        let raw = unsafe {
            SDL_OpenAudioDeviceStream(
                SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,
                &raw const spec,
                None,
                ptr::null_mut(),
            )
        };
        let stream = NonNull::new(raw).ok_or_else(sdl_error)?;
        let out = Self {
            stream,
            _context: context,
        };
        // SAFETY: `stream` is the live stream just opened; its device starts paused.
        if !unsafe { SDL_ResumeAudioStreamDevice(out.stream.as_ptr()) } {
            return Err(sdl_error());
        }
        Ok(out)
    }

    /// How many frames are waiting to be played.
    #[must_use]
    pub fn queued(&self) -> usize {
        // SAFETY: `stream` is live for as long as `self`.
        let bytes = unsafe { SDL_GetAudioStreamQueued(self.stream.as_ptr()) };
        usize::try_from(bytes).unwrap_or_default() / FRAME_BYTES
    }

    /// Queues `samples`, left and right interleaved, behind what is already waiting.
    ///
    /// # Errors
    ///
    /// SDL's error if it refuses the data.
    pub fn push(&mut self, samples: &[i16]) -> Result<()> {
        let len = c_int::try_from(size_of_val(samples))
            .map_err(|_| Error::Sdl("too many samples at once".into()))?;
        // SAFETY: `samples` is valid for `len` bytes and SDL copies them before returning.
        if unsafe { SDL_PutAudioStreamData(self.stream.as_ptr(), samples.as_ptr().cast(), len) } {
            Ok(())
        } else {
            Err(sdl_error())
        }
    }
}

impl Drop for AudioOut {
    fn drop(&mut self) {
        // SAFETY: the stream is live and nothing else holds it; this closes its device too.
        unsafe { SDL_DestroyAudioStream(self.stream.as_ptr()) };
    }
}
