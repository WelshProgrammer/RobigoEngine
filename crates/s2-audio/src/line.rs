//! A long recording read as it plays, such as a CD track, and mixed beside the effects at the
//! output rate. A recording at [`OUTPUT_RATE`] goes out sample for sample; one at any other rate
//! goes through the same cubic as the smooth effects.

use crate::stream::{CHANNELS, FIXED, OUTPUT_FRAMES, OUTPUT_RATE, cubic};

/// Where a [`Line`]'s frames come from.
pub trait Frames: Send {
    /// Frames a second.
    fn rate(&self) -> u32;

    /// Adds the next frames to `frames`; `false` once there are no more.
    fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool;
}

/// How many played frames the line keeps before it lets them go.
const KEEP: usize = 4096;

/// The gain at which a recording plays as recorded.
pub const FULL: u8 = 127;

/// The recording playing, if any.
pub struct Line {
    source: Option<Box<dyn Frames>>,
    frames: Vec<[i16; CHANNELS]>,
    /// Where the next output frame comes from in `frames`, 32.32 fixed point.
    position: u64,
    /// How far each output frame moves `position`.
    step: u64,
    /// Whether the source has given its last frame.
    drained: bool,
    /// How loud it plays, [`FULL`] as recorded.
    gain: u8,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            source: None,
            frames: Vec::new(),
            position: 0,
            step: 0,
            drained: false,
            gain: FULL,
        }
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Line")
            .field("playing", &self.playing())
            .field("step", &self.step)
            .finish_non_exhaustive()
    }
}

impl Line {
    /// A line with nothing playing, at full gain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets how loud the recording plays, from 0 silent to [`FULL`] as recorded, a straight
    /// scale of every sample.
    pub fn set_gain(&mut self, gain: u8) {
        self.gain = gain.min(FULL);
    }

    /// Plays `source` from its start, in place of whatever played.
    pub fn play(&mut self, source: Box<dyn Frames>) {
        self.step = (u64::from(source.rate()) << FIXED) / u64::from(OUTPUT_RATE);
        self.source = Some(source);
        self.frames.clear();
        self.position = 0;
        self.drained = false;
    }

    /// Stops what plays.
    pub fn stop(&mut self) {
        self.source = None;
        self.frames.clear();
    }

    /// Whether a recording is still playing.
    #[must_use]
    pub fn playing(&self) -> bool {
        self.source.is_some()
    }

    /// Writes the next [`OUTPUT_FRAMES`] frames into `out`, left and right interleaved, with
    /// silence once the recording ends, which stops it.
    pub fn fill(&mut self, out: &mut [i16]) {
        out.fill(0);
        if self.source.is_none() {
            return;
        }
        let exact = self.step == 1 << FIXED;
        for frame in out.as_chunks_mut::<CHANNELS>().0.iter_mut().take(OUTPUT_FRAMES) {
            let at = usize::try_from(self.position >> FIXED).unwrap_or(usize::MAX);
            let ahead = if exact { 1 } else { 3 };
            if !self.ready(at.saturating_add(ahead)) && at >= self.frames.len() {
                self.stop();
                return;
            }
            if exact {
                *frame = self.frames[at];
            } else if let Some(value) = cubic(&self.frames, self.position) {
                *frame = value.map(|sample| {
                    #[expect(clippy::cast_possible_truncation, reason = "held to the 16-bit range first")]
                    let held = sample.round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
                    held
                });
            }
            self.position += self.step;
        }
        self.let_go();
        if self.gain < FULL {
            for sample in out.iter_mut() {
                let scaled = i32::from(*sample) * i32::from(self.gain) / i32::from(FULL);
                *sample = i16::try_from(scaled).unwrap_or_default();
            }
        }
    }

    /// Reads until `frames` holds `len` frames or the source runs out; whether it holds them.
    fn ready(&mut self, len: usize) -> bool {
        while self.frames.len() < len && !self.drained {
            let Some(source) = self.source.as_mut() else {
                return false;
            };
            self.drained = !source.read(&mut self.frames);
        }
        self.frames.len() >= len
    }

    /// Lets go of the frames played long enough ago that the cubic no longer reaches them.
    fn let_go(&mut self) {
        let at = usize::try_from(self.position >> FIXED).unwrap_or(usize::MAX);
        if at > KEEP {
            let gone = at - 1;
            self.frames.drain(..gone.min(self.frames.len()));
            self.position -= u64::try_from(gone).unwrap_or(u64::MAX) << FIXED;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::OUTPUT_BUFFER;

    /// A ramp of `len` frames at `rate`, handed over 1,000 frames a read.
    struct Ramp {
        rate: u32,
        next: i16,
        len: i16,
    }

    impl Frames for Ramp {
        fn rate(&self) -> u32 {
            self.rate
        }

        fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
            let end = self.len.min(self.next.saturating_add(1000));
            frames.extend((self.next..end).map(|value| [value, -value]));
            self.next = end;
            self.next < self.len
        }
    }

    #[test]
    fn a_recording_at_the_output_rate_goes_out_sample_for_sample() {
        let mut line = Line::new();
        line.play(Box::new(Ramp {
            rate: OUTPUT_RATE,
            next: 0,
            len: 6000,
        }));
        let mut played = Vec::new();
        let mut out = [0; OUTPUT_BUFFER];
        while line.playing() {
            line.fill(&mut out);
            played.extend_from_slice(&out);
        }
        let frames: Vec<_> = played
            .as_chunks::<CHANNELS>()
            .0
            .iter()
            .take(6000)
            .copied()
            .collect();
        let ramp: Vec<_> = (0..6000).map(|value| [value, -value]).collect();
        assert_eq!(frames, ramp);
        assert!(
            played[6000 * CHANNELS..].iter().all(|&sample| sample == 0),
            "silence after"
        );
        assert_eq!(
            played.len(),
            6000_usize.div_ceil(OUTPUT_FRAMES) * OUTPUT_BUFFER,
            "stops in the call that ran out"
        );
    }

    #[test]
    fn a_recording_at_another_rate_is_stepped_to_the_output_rate() {
        let mut line = Line::new();
        line.play(Box::new(Ramp {
            rate: 2 * OUTPUT_RATE,
            next: 0,
            len: 8000,
        }));
        let mut out = [0; OUTPUT_BUFFER];
        line.fill(&mut out);
        assert_eq!(&out[..8], [0, 0, 2, -2, 4, -4, 6, -6], "every other frame");
        line.fill(&mut out);
        let carried = i16::try_from(2 * OUTPUT_FRAMES).unwrap();
        assert_eq!(out[0], carried, "the next call carries on where the last stopped");
        line.stop();
        line.fill(&mut out);
        assert!(out.iter().all(|&sample| sample == 0));
    }
}
