//! The intro film, which plays before the title as S2's launcher plays it before starting
//! `S2.EXE`. The CD's `VIDEO/INTRO2.SMK` plays when the data has it, the same film compressed
//! less, and `VIDEO/INTRO.SMK` otherwise. The pictures are drawn at 4:3, as a monitor of the time
//! stretched the 320 by 200 film, fitted to the window. Any key or button ends it.
//!
//! A worker thread decodes the pictures a few frames ahead of the clock. The sound is read in
//! one pass before the film starts and goes to the music line.

use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use s2_assets::{Palette, dac_palette};
use s2_audio::{CHANNELS, Frames};
use s2_formats::smacker::{Pcm, Video};
use s2_render::{Frame, Placement, Renderer};

/// Where the film lives in the data, the one to play first.
pub(crate) const FILMS: [&str; 2] = ["VIDEO/INTRO2.SMK", "VIDEO/INTRO.SMK"];

/// The shape a monitor showed the film in.
const SHOWN_AS: (u32, u32) = (4, 3);

/// Pictures decoded ahead of the one on screen.
const AHEAD: usize = 8;

/// Frames of sound handed to the line at a time.
const READ: usize = 4096;

/// The centre of an unsigned 8-bit sample.
const SILENCE: i16 = 128;

/// One decoded picture.
struct Picture {
    index: u32,
    pixels: Vec<u8>,
    /// The film's palette, when this frame changed it.
    palette: Option<Palette>,
}

/// The film playing.
pub(crate) struct Intro {
    pictures: Receiver<Picture>,
    frame: Frame,
    /// The palette the renderer is to take before the next draw.
    palette: Option<Palette>,
    /// The number of the picture in `frame`, if any.
    showing: Option<u32>,
    count: u32,
    frame_duration: Duration,
    started: Option<Instant>,
    sound: Option<Box<dyn Frames>>,
}

impl Intro {
    /// The film in `bytes`, ready to play.
    ///
    /// # Errors
    ///
    /// The decoder's error when the film cannot be read.
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self, s2_formats::smacker::Error> {
        let video = Video::parse(&bytes)?;
        let header = video.header().clone();
        let sound = film_sound(&video);
        let (send, pictures) = sync_channel(AHEAD);
        let spawned = std::thread::Builder::new()
            .name("intro".into())
            .spawn(move || decode(&bytes, &send));
        if let Err(error) = spawned {
            log::warn!("no thread to decode the intro: {error}");
        }
        Ok(Self {
            pictures,
            frame: Frame::new(u32::from(header.width), u32::from(header.height), 0),
            palette: None,
            showing: None,
            count: header.frame_count,
            frame_duration: header.frame_duration,
            started: None,
            sound,
        })
    }

    /// The film's sound for the music line, once, when the film starts.
    pub(crate) fn take_sound(&mut self) -> Option<Box<dyn Frames>> {
        self.sound.take()
    }

    /// Draws the picture due now, starting the clock on the first call. Gives false once the
    /// film has ended.
    ///
    /// # Errors
    ///
    /// The renderer's error when the frame cannot be presented.
    pub(crate) fn show(&mut self, renderer: &mut Renderer) -> s2_render::Result<bool> {
        let started = *self.started.get_or_insert_with(Instant::now);
        let due = due(started.elapsed(), self.frame_duration);
        if due >= self.count || !self.catch_up(due) {
            return Ok(false);
        }
        if let Some(palette) = self.palette.take() {
            renderer.set_palette(&palette);
        }
        let output = renderer.output_size().unwrap_or(SHOWN_AS);
        renderer.render_placed(&self.frame, Placement::fitted(output, SHOWN_AS))?;
        Ok(true)
    }

    /// Takes decoded pictures up to the one numbered `due`, keeping each palette change on
    /// the way. False once the worker has stopped with nothing left to give.
    fn catch_up(&mut self, due: u32) -> bool {
        while self.showing.is_none_or(|showing| showing < due) {
            match self.pictures.try_recv() {
                Ok(picture) => {
                    if let Some(palette) = picture.palette {
                        self.palette = Some(dac_palette(&palette));
                    }
                    self.frame.fill_from(&picture.pixels);
                    self.showing = Some(picture.index);
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => return self.showing.is_some(),
            }
        }
        true
    }
}

/// The number of the picture on screen `elapsed` into a film of `frame_duration` a picture.
fn due(elapsed: Duration, frame_duration: Duration) -> u32 {
    let each = frame_duration.as_micros().max(1);
    u32::try_from(elapsed.as_micros() / each).unwrap_or(u32::MAX)
}

/// Decodes the film in `bytes` in order onto `send` until it ends or the film is dropped. A
/// frame that does not decode is left out and the next carries on.
fn decode(bytes: &[u8], send: &std::sync::mpsc::SyncSender<Picture>) {
    let Ok(video) = Video::parse(bytes) else {
        return;
    };
    let mut decoder = video.decoder();
    loop {
        let picture = match decoder.decode_next() {
            Ok(Some(frame)) => Picture {
                index: frame.index,
                pixels: frame.image.pixels().to_vec(),
                palette: frame.palette_changed.then(|| frame.palette.clone()),
            },
            Ok(None) => return,
            Err(error) => {
                log::warn!("intro frame {}: {error}", decoder.position());
                continue;
            }
        };
        if send.send(picture).is_err() {
            return;
        }
    }
}

/// The first sound track of `video` as 16-bit stereo, or `None` when it has none.
fn film_sound(video: &Video<'_>) -> Option<Box<dyn Frames>> {
    let track = video.header().tracks[0]?;
    let stereo = track.channels == 2;
    let mut frames: Vec<[i16; CHANNELS]> = Vec::new();
    let mut decoder = video.decoder().sound_only();
    loop {
        match decoder.decode_next() {
            Ok(Some(frame)) => match frame.audio(0) {
                Some(Pcm::U8(samples)) => {
                    widen(
                        &mut frames,
                        samples.iter().map(|&s| (i16::from(s) - SILENCE) << 8),
                        stereo,
                    );
                }
                Some(Pcm::I16(samples)) => widen(&mut frames, samples.iter().copied(), stereo),
                None => {}
            },
            Ok(None) => break,
            Err(error) => log::warn!("intro sound at frame {}: {error}", decoder.position()),
        }
    }
    Some(Box::new(FilmSound {
        rate: track.rate,
        frames,
        at: 0,
    }))
}

/// Adds `samples` to `frames` as stereo pairs, doubling each mono sample.
fn widen(frames: &mut Vec<[i16; CHANNELS]>, samples: impl Iterator<Item = i16>, stereo: bool) {
    if stereo {
        let samples: Vec<i16> = samples.collect();
        frames.extend(samples.as_chunks::<CHANNELS>().0.iter().copied());
    } else {
        frames.extend(samples.map(|sample| [sample; CHANNELS]));
    }
}

/// The film's whole sound, played from the start.
struct FilmSound {
    rate: u32,
    frames: Vec<[i16; CHANNELS]>,
    at: usize,
}

impl Frames for FilmSound {
    fn rate(&self) -> u32 {
        self.rate
    }

    fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
        let end = (self.at + READ).min(self.frames.len());
        frames.extend_from_slice(&self.frames[self.at..end]);
        self.at = end;
        self.at < self.frames.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_due_follows_the_clock() {
        let each = Duration::from_millis(66);
        assert_eq!(due(Duration::ZERO, each), 0);
        assert_eq!(due(Duration::from_millis(65), each), 0);
        assert_eq!(due(Duration::from_millis(66), each), 1);
        assert_eq!(due(Duration::from_millis(146_784), each), 2224);
    }

    #[test]
    fn mono_sound_doubles_and_eight_bits_centre_on_zero() {
        let mut frames = Vec::new();
        widen(
            &mut frames,
            [0u8, 128, 255].iter().map(|&s| (i16::from(s) - SILENCE) << 8),
            false,
        );
        assert_eq!(frames, vec![[-32768, -32768], [0, 0], [32512, 32512]]);
        let mut both = Vec::new();
        widen(&mut both, [1i16, 2, 3, 4].into_iter(), true);
        assert_eq!(both, vec![[1, 2], [3, 4]]);
    }

    #[test]
    fn the_sound_runs_out_after_its_last_frame() {
        let mut sound = FilmSound {
            rate: 22_050,
            frames: vec![[1, 1]; READ + 10],
            at: 0,
        };
        let mut read = Vec::new();
        assert!(sound.read(&mut read));
        assert!(!sound.read(&mut read));
        assert_eq!(read.len(), READ + 10);
    }
}
