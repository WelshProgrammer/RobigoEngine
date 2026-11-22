//! The mix: every playing effect turned into one 44,100 Hz stream.
//!
//! A sound plays one of two ways. An [`Sample::Original`] plays as the Miles library in `S2.EXE`
//! plays it: 256 frames a call at 11,025 Hz, each sound walking its 8-bit sample with a 16.16
//! step and no interpolation, starting every call half a source byte in, and the voices adding up
//! in 32 bits into 16-bit stereo. That stream, which [`Stream::card`] gives back, is what the
//! original sends the card, frame for frame. An [`Upsampler`] takes it to 44,100 Hz. A
//! [`Sample::Smooth`] plays from its own rate straight to 44,100 Hz through a cubic
//! interpolation and adds in there.

use std::sync::Arc;

/// Frames a second the original's mix runs at: the Sound Blaster 16's nearest rate to the
/// 11,000 Hz the game asks for.
pub const RATE: u32 = 11_025;

/// Frames mixed per call at [`RATE`], the driver record's `+0x48`.
pub const FRAMES: usize = 256;

/// Channels in the stream, left then right.
pub const CHANNELS: usize = 2;

/// Samples in one call's worth of the original's stream.
pub const BUFFER: usize = FRAMES * CHANNELS;

/// Output frames for each frame of the original's stream.
pub const UPSAMPLING: usize = 4;

/// Frames a second the mix goes to the card at.
pub const OUTPUT_RATE: u32 = 44_100;

const _: () = assert!(RATE as usize * UPSAMPLING == OUTPUT_RATE as usize);

/// Output frames a call.
pub const OUTPUT_FRAMES: usize = FRAMES * UPSAMPLING;

/// Samples in one call's worth of the output.
pub const OUTPUT_BUFFER: usize = OUTPUT_FRAMES * CHANNELS;

/// How many sounds the mix holds at once, the driver record's `+0x60`.
pub const SOUNDS: usize = 16;

/// The digital master at full, which is where it starts.
pub const FULL: u8 = 127;

/// How far a step may sit from one source byte per frame and still play byte for byte: library
/// preference 6, 1% of [`ONE`].
pub const TOLERANCE: u32 = 655;

/// One source byte per frame as a 16.16 step.
const ONE: u64 = 1 << 16;

/// Half a source byte in the 32-bit accumulator the fraction runs in.
const HALF: u32 = 1 << 31;

/// The byte an unsigned 8-bit sample is silent at.
const SILENT: i32 = 128;

/// How many output units one step of the sample and one step of the gain are worth.
const UNIT: i32 = 2;

/// The loudest volume or gain a sample record holds.
const LOUDEST: u32 = 127;

/// How a sound moves through its sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// One byte per frame, for a rate within [`TOLERANCE`] of [`RATE`].
    Straight,
    /// `whole` bytes per frame plus `fraction`, which is the step's lower 16 bits moved to the
    /// top of a 32-bit accumulator.
    Resampled {
        /// Whole bytes per frame.
        whole: usize,
        /// The fractional part, scaled to the 32-bit accumulator.
        fraction: u32,
    },
}

impl Step {
    /// The step for a sample recorded at `rate` Hz.
    #[must_use]
    pub fn new(rate: u16) -> Self {
        let step = u64::from(rate) * ONE / u64::from(RATE);
        if step.abs_diff(ONE) <= u64::from(TOLERANCE) {
            return Self::Straight;
        }
        let whole = usize::try_from(step / ONE).unwrap_or(usize::MAX);
        let low = u32::try_from(step % ONE).unwrap_or_default();
        Self::Resampled {
            whole,
            fraction: low << 16,
        }
    }
}

/// The gain a sound plays at for `volume` under the digital `master`: their product over 127,
/// held to 127, and one more unless it is 0 (`0xa0950`, an 8-bit mono sample).
#[must_use]
pub fn gain(volume: u16, master: u8) -> i32 {
    let volume = u32::from(volume).min(LOUDEST);
    let scaled = (volume * u32::from(master) / LOUDEST).min(LOUDEST);
    let gain = if scaled == 0 { 0 } else { scaled + 1 };
    i32::try_from(gain).unwrap_or_default()
}

/// What a sound plays from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sample {
    /// Unsigned 8-bit mono, played as the original plays it at its row's rate.
    Original(Arc<[u8]>),
    /// 16-bit stereo frames recorded at `rate`, played from that rate through the cubic
    /// interpolation.
    Smooth {
        /// Frames a second it was recorded at.
        rate: u32,
        /// The frames, left then right.
        frames: Arc<[[i16; CHANNELS]]>,
    },
}

impl Sample {
    /// The rate a sound plays at for a row whose rate is `nominal` and came out of the jitter as
    /// `jittered`: `jittered` itself for an original sample, and the sample's own rate moved by
    /// the same proportion for a smooth one.
    #[must_use]
    pub fn rate(&self, jittered: u16, nominal: u16) -> u32 {
        match self {
            Self::Original(_) => u32::from(jittered),
            Self::Smooth { rate, .. } if nominal == 0 => *rate,
            Self::Smooth { rate, .. } => {
                let moved = u64::from(*rate) * u64::from(jittered) / u64::from(nominal);
                u32::try_from(moved).unwrap_or(u32::MAX)
            }
        }
    }
}

/// How one sound moves through what it plays from.
#[derive(Debug, Clone)]
enum Voice {
    Original {
        samples: Arc<[u8]>,
        position: usize,
        step: Step,
    },
    Smooth {
        frames: Arc<[[i16; CHANNELS]]>,
        /// Where it is, in frames as 32.32 fixed point.
        position: u64,
        /// How far it moves an output frame, in the same units.
        step: u64,
    },
}

/// One sound in the mix.
#[derive(Debug, Clone)]
struct Playing {
    voice: Voice,
    volume: u16,
    tag: usize,
}

/// The mix and the sounds in it.
#[derive(Debug, Clone)]
pub struct Stream {
    sounds: [Option<Playing>; SOUNDS],
    master: u8,
    sums: Vec<i32>,
    card: [i16; BUFFER],
    upsampler: Upsampler,
    wide: Vec<f32>,
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream {
    /// A mix with nothing playing and the master at [`FULL`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            sounds: Default::default(),
            master: FULL,
            sums: vec![0; BUFFER],
            card: [0; BUFFER],
            upsampler: Upsampler::new(),
            wide: vec![0.0; OUTPUT_BUFFER],
        }
    }

    /// The digital master volume, 0 to [`FULL`].
    #[must_use]
    pub const fn master(&self) -> u8 {
        self.master
    }

    /// Sets the digital master, held to [`FULL`]. Sounds already playing take it from the next
    /// call, as the original recomputes every gain when the master changes.
    pub fn set_master(&mut self, master: u8) {
        self.master = master.min(FULL);
    }

    /// How many sounds are playing.
    #[must_use]
    pub fn playing(&self) -> usize {
        self.sounds.iter().flatten().count()
    }

    /// Starts `sample` at `volume` (0 to 127), from the top of the next call, for a row whose
    /// rate is `nominal` and came out of the jitter as `jittered` ([`Sample::rate`]). `tag`
    /// comes back from [`Stream::mix`] when the sound ends. Gives back false, starting nothing,
    /// when [`SOUNDS`] are already playing.
    pub fn start(&mut self, tag: usize, sample: Sample, volume: u16, jittered: u16, nominal: u16) -> bool {
        let Some(free) = self.sounds.iter_mut().find(|sound| sound.is_none()) else {
            return false;
        };
        let rate = sample.rate(jittered, nominal);
        let voice = match sample {
            Sample::Original(samples) => Voice::Original {
                samples,
                position: 0,
                step: Step::new(jittered),
            },
            Sample::Smooth { frames, .. } => Voice::Smooth {
                frames,
                position: 0,
                step: (u64::from(rate) << FIXED) / u64::from(OUTPUT_RATE),
            },
        };
        *free = Some(Playing { voice, volume, tag });
        true
    }

    /// Stops the sound started with `tag` at once, and says whether one was playing.
    pub fn stop(&mut self, tag: usize) -> bool {
        let found = self
            .sounds
            .iter_mut()
            .find(|sound| sound.as_ref().is_some_and(|sound| sound.tag == tag));
        found.map(Option::take).is_some()
    }

    /// Mixes the next [`OUTPUT_FRAMES`] frames into `out`, left and right interleaved, and gives
    /// back the tag of every sound that ran out during them.
    pub fn mix(&mut self, out: &mut [i16; OUTPUT_BUFFER]) -> Vec<usize> {
        self.mix_over(&[0; OUTPUT_BUFFER], out)
    }

    /// Mixes as [`Stream::mix`] does, over `line`, a recording already at the output rate that
    /// adds in at its own level, as a CD's audio reaches the card's mixer beside the sound card's.
    pub fn mix_over(&mut self, line: &[i16; OUTPUT_BUFFER], out: &mut [i16; OUTPUT_BUFFER]) -> Vec<usize> {
        let mut ended = self.mix_card();
        self.upsampler.run(&self.card, &mut self.wide);
        for (sum, &sample) in self.wide.iter_mut().zip(line) {
            *sum += f32::from(sample);
        }
        for place in &mut self.sounds {
            let Some(Playing {
                voice:
                    Voice::Smooth {
                        frames,
                        position,
                        step,
                    },
                volume,
                tag,
            }) = place
            else {
                continue;
            };
            #[expect(clippy::cast_precision_loss, reason = "a gain is at most 128")]
            let gain = gain(*volume, self.master) as f32 / SMOOTH_UNIT;
            for frame in self.wide.as_chunks_mut::<CHANNELS>().0 {
                let Some(value) = cubic(frames, *position) else {
                    break;
                };
                for (sum, value) in frame.iter_mut().zip(value) {
                    *sum += value * gain;
                }
                *position += *step;
            }
            if usize::try_from(*position >> FIXED).unwrap_or(usize::MAX) >= frames.len() {
                ended.push(*tag);
                *place = None;
            }
        }
        for (out, &sum) in out.iter_mut().zip(&self.wide) {
            #[expect(clippy::cast_possible_truncation, reason = "held to the 16-bit range first")]
            let held = sum.round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
            *out = held;
        }
        ended
    }

    /// The last call's frames of the original's stream at [`RATE`]: what the original sends the
    /// card for the original samples playing, left and right interleaved.
    #[must_use]
    pub const fn card(&self) -> &[i16; BUFFER] {
        &self.card
    }

    /// Mixes the next [`FRAMES`] frames of the original samples into [`Stream::card`], giving back
    /// the tag of every one that ran out during them.
    fn mix_card(&mut self) -> Vec<usize> {
        self.sums.fill(0);
        let mut ended = Vec::new();
        for place in &mut self.sounds {
            let Some(Playing {
                voice:
                    Voice::Original {
                        samples,
                        position,
                        step,
                    },
                volume,
                tag,
            }) = place
            else {
                continue;
            };
            let gain = gain(*volume, self.master) * UNIT;
            let mut fraction = HALF;
            for frame in self.sums.as_chunks_mut::<CHANNELS>().0 {
                let Some(&byte) = samples.get(*position) else {
                    break;
                };
                let value = (i32::from(byte) - SILENT) * gain;
                for sum in frame {
                    *sum += value;
                }
                *position += match *step {
                    Step::Straight => 1,
                    Step::Resampled { whole, fraction: by } => {
                        let (next, carry) = fraction.overflowing_add(by);
                        fraction = next;
                        whole + usize::from(carry)
                    }
                };
            }
            if *position >= samples.len() {
                ended.push(*tag);
                *place = None;
            }
        }
        for (out, &sum) in self.card.iter_mut().zip(&self.sums) {
            *out = i16::try_from(sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).unwrap_or_default();
        }
        ended
    }
}

/// Fractional bits of a smooth voice's position and step.
pub(crate) const FIXED: u32 = 32;

/// What a 16-bit smooth sample is divided by, after its gain, to land on the same scale as an
/// original sample: an 8-bit step is 256 of a 16-bit one, and is worth [`UNIT`] output units.
const SMOOTH_UNIT: f32 = 128.0;

/// The frame `frames` holds at `position`, 32.32 fixed point, by a Catmull-Rom cubic through the
/// frames either side, or `None` past the last frame. Frames before the first and after the last
/// count as the first and the last.
pub(crate) fn cubic(frames: &[[i16; CHANNELS]], position: u64) -> Option<[f32; CHANNELS]> {
    let at = usize::try_from(position >> FIXED).ok()?;
    let last = frames.len().checked_sub(1)?;
    if at > last {
        return None;
    }
    #[expect(clippy::cast_precision_loss, reason = "the low 32 bits as a fraction of one")]
    let along = (position & u64::from(u32::MAX)) as f32 / (u64::from(u32::MAX) + 1) as f32;
    let frame = |index: usize| frames[index.min(last)];
    let [before, here, next, after] = [
        frame(at.saturating_sub(1)),
        frame(at),
        frame(at + 1),
        frame(at + 2),
    ];
    Some(std::array::from_fn(|channel| {
        let [p0, p1, p2, p3] = [before, here, next, after].map(|frame| f32::from(frame[channel]));
        let squared = along * along;
        let cubed = squared * along;
        0.5 * (2.0 * p1
            + (p2 - p0) * along
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * squared
            + (3.0 * p1 - p0 - 3.0 * p2 + p3) * cubed)
    }))
}

/// Taps of the upsampler's filter for each output phase.
const TAPS: usize = 48;

/// How much of the original's band the filter keeps, short of its Nyquist frequency.
const PASSBAND: f32 = 0.88;

/// Takes the original's stream from [`RATE`] to [`OUTPUT_RATE`]: a windowed sinc low-pass,
/// one set of [`TAPS`] coefficients for each of the [`UPSAMPLING`] output phases, each set
/// summing to one so a steady level comes through as it went in.
#[derive(Debug, Clone)]
pub struct Upsampler {
    phases: [[f32; TAPS]; UPSAMPLING],
    history: [[f32; CHANNELS]; TAPS],
    next: usize,
}

impl Default for Upsampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Upsampler {
    /// An upsampler whose history is silence.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "tap numbers are tiny")]
    pub fn new() -> Self {
        let length = TAPS * UPSAMPLING;
        let centre = (length - 1) as f32 / 2.0;
        let mut phases = [[0.0; TAPS]; UPSAMPLING];
        for (phase, taps) in phases.iter_mut().enumerate() {
            for (tap, coefficient) in taps.iter_mut().enumerate() {
                let n = (tap * UPSAMPLING + phase) as f32;
                let x = (n - centre) / UPSAMPLING as f32 * PASSBAND;
                let sinc = if x == 0.0 {
                    1.0
                } else {
                    (std::f32::consts::PI * x).sin() / (std::f32::consts::PI * x)
                };
                let w = 2.0 * std::f32::consts::PI * n / (length - 1) as f32;
                let blackman = 0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos();
                *coefficient = sinc * blackman;
            }
            let sum: f32 = taps.iter().sum();
            for coefficient in taps.iter_mut() {
                *coefficient /= sum;
            }
        }
        Self {
            phases,
            history: [[0.0; CHANNELS]; TAPS],
            next: 0,
        }
    }

    /// Writes `input`, frames at [`RATE`], into `output` at [`OUTPUT_RATE`], left and right
    /// interleaved, carrying the history over from the last call.
    pub fn run(&mut self, input: &[i16], output: &mut [f32]) {
        let wide = output.as_chunks_mut::<{ CHANNELS * UPSAMPLING }>().0;
        for (frame, out) in input.as_chunks::<CHANNELS>().0.iter().zip(wide) {
            self.history[self.next] = frame.map(f32::from);
            for (taps, out) in self.phases.iter().zip(out.as_chunks_mut::<CHANNELS>().0) {
                let mut sum = [0.0; CHANNELS];
                for (tap, &coefficient) in taps.iter().enumerate() {
                    let past = self.history[(self.next + TAPS - tap) % TAPS];
                    for (sum, past) in sum.iter_mut().zip(past) {
                        *sum += coefficient * past;
                    }
                }
                out.copy_from_slice(&sum);
            }
            self.next = (self.next + 1) % TAPS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample whose bytes count up from silence, so each output value names the byte it took.
    fn ramp(len: usize) -> Arc<[u8]> {
        (0..len)
            .map(|i| u8::try_from(i % 100).unwrap_or_default() + 128)
            .collect()
    }

    /// One call of `stream`'s mix, giving back the original's stream it sent the card.
    fn card_mix(stream: &mut Stream) -> [i16; BUFFER] {
        let mut out = [0; OUTPUT_BUFFER];
        stream.mix(&mut out);
        *stream.card()
    }

    /// The source byte each frame of `frames` took, read back from a ramp at gain `g`.
    fn positions(out: &[i16], g: i32) -> Vec<i32> {
        out.as_chunks::<CHANNELS>()
            .0
            .iter()
            .map(|frame| i32::from(frame[0]) / (g * UNIT))
            .collect()
    }

    #[test]
    fn the_gain_matches_the_captures() {
        assert_eq!(gain(100, 127), 101);
        assert_eq!(gain(100, 100), 79);
        assert_eq!(gain(100, 64), 51);
        assert_eq!(gain(100, 33), 26);
        assert_eq!(gain(100, 8), 7);
        assert_eq!(gain(100, 1), 0);
        assert_eq!(gain(127, 127), 128);
        assert_eq!(gain(200, 127), 128);
    }

    #[test]
    fn rates_within_one_percent_play_byte_for_byte() {
        assert_eq!(Step::new(11_025), Step::Straight);
        assert_eq!(Step::new(11_000), Step::Straight);
        assert!(matches!(Step::new(11_200), Step::Resampled { whole: 1, .. }));
        assert!(matches!(Step::new(22_000), Step::Resampled { whole: 1, .. }));
        assert!(matches!(Step::new(4_000), Step::Resampled { whole: 0, .. }));
    }

    #[test]
    fn one_step_of_the_sample_at_volume_100_is_202() {
        let mut stream = Stream::new();
        stream.start(
            0,
            Sample::Original(Arc::from([129u8, 127, 128].as_slice())),
            100,
            11_025,
            11_025,
        );
        let out = card_mix(&mut stream);
        assert_eq!(&out[..6], &[202, 202, -202, -202, 0, 0]);
        assert!(out[6..].iter().all(|&s| s == 0));
    }

    #[test]
    fn a_resampled_sound_takes_the_nearest_byte_and_restarts_its_phase_every_call() {
        let rate = 4_000;
        let step = u64::from(rate) * ONE / u64::from(RATE);
        let mut stream = Stream::new();
        stream.start(0, Sample::Original(ramp(1000)), 100, rate, rate);
        let mut start = 0;
        for _ in 0..3 {
            let out = card_mix(&mut stream);
            let got = positions(&out, 101);
            for (frame, &position) in got.iter().enumerate().take(64) {
                let expected = start + (frame as u64 * step + ONE / 2) / ONE;
                assert_eq!(position, i32::try_from(expected % 100).unwrap());
            }
            start += (FRAMES as u64 * step + ONE / 2) / ONE;
        }
    }

    #[test]
    fn a_sound_ends_in_the_call_its_bytes_run_out() {
        let mut stream = Stream::new();
        stream.start(7, Sample::Original(ramp(300)), 100, 11_025, 11_025);
        let mut out = [0; OUTPUT_BUFFER];
        let found = stream.mix(&mut out);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(stream.mix(&mut out), vec![7]);
        assert_eq!(stream.playing(), 0);
        assert!(stream.card()[(300 - FRAMES) * CHANNELS..].iter().all(|&s| s == 0));
    }

    #[test]
    fn a_stopped_sound_goes_silent_at_once_and_reports_no_end() {
        let mut stream = Stream::new();
        stream.start(3, Sample::Original(ramp(3000)), 100, 11_025, 11_025);
        stream.start(4, Sample::Original(ramp(3000)), 100, 11_025, 11_025);
        assert!(stream.stop(3));
        assert!(!stream.stop(3), "nothing left to stop");
        assert_eq!(stream.playing(), 1);
        let mut out = [0; OUTPUT_BUFFER];
        assert!(stream.mix(&mut out).is_empty(), "a stopped sound does not end");
    }

    #[test]
    fn voices_add_up_and_hold_at_the_sixteen_bit_limits() {
        let mut stream = Stream::new();
        let loud: Arc<[u8]> = Arc::from([255u8; 4].as_slice());
        stream.start(0, Sample::Original(Arc::clone(&loud)), 127, 11_025, 11_025);
        stream.start(1, Sample::Original(loud), 127, 11_025, 11_025);
        let out = card_mix(&mut stream);
        assert_eq!(out[0], i16::MAX);
        let mut stream = Stream::new();
        stream.start(
            0,
            Sample::Original(Arc::from([0u8; 4].as_slice())),
            127,
            11_025,
            11_025,
        );
        let out = card_mix(&mut stream);
        assert_eq!(out[0], i16::MIN);
    }

    #[test]
    fn the_master_reaches_sounds_already_playing() {
        let mut stream = Stream::new();
        stream.start(
            0,
            Sample::Original(Arc::from([129u8; 600].as_slice())),
            100,
            11_025,
            11_025,
        );
        let out = card_mix(&mut stream);
        assert_eq!(out[0], 202);
        stream.set_master(8);
        let out = card_mix(&mut stream);
        assert_eq!(out[0], 14);
    }

    #[test]
    fn a_full_mix_starts_nothing_more() {
        let mut stream = Stream::new();
        let one: Arc<[u8]> = Arc::from([128u8; 4].as_slice());
        for tag in 0..SOUNDS {
            assert!(stream.start(tag, Sample::Original(Arc::clone(&one)), 100, 11_025, 11_025));
        }
        assert!(!stream.start(SOUNDS, Sample::Original(one), 100, 11_025, 11_025));
    }

    /// A smooth sample of `frames` frames, each `[value, -value]`.
    fn level(frames: usize, value: i16) -> Sample {
        Sample::Smooth {
            rate: OUTPUT_RATE,
            frames: vec![[value, -value]; frames].into(),
        }
    }

    #[test]
    fn a_smooth_sample_at_the_output_rate_plays_frame_for_frame_on_the_original_scale() {
        let mut stream = Stream::new();
        stream.start(0, level(OUTPUT_FRAMES * 2, 256), 100, 11_000, 11_000);
        let mut out = [0; OUTPUT_BUFFER];
        let found = stream.mix(&mut out);
        assert!(found.is_empty(), "{found:?}");
        assert!(
            out.as_chunks::<CHANNELS>()
                .0
                .iter()
                .all(|&frame| frame == [202, -202]),
            "an 8-bit step of one, widened, comes out as the original's 202 at volume 100"
        );
        assert_eq!(
            stream.mix(&mut out),
            vec![0],
            "it ends in the call its frames run out"
        );
    }

    #[test]
    fn a_smooth_sample_plays_at_its_own_rate_moved_by_the_jitter() {
        let half = Sample::Smooth {
            rate: 22_050,
            frames: vec![[0, 0]; 10].into(),
        };
        assert_eq!(half.rate(11_000, 11_000), 22_050);
        assert_eq!(half.rate(11_550, 11_000), 23_152);
        assert_eq!(
            Sample::Original(Arc::from([128u8].as_slice())).rate(11_550, 11_000),
            11_550
        );
        let mut stream = Stream::new();
        stream.start(0, half, 100, 11_000, 11_000);
        let mut out = [0; OUTPUT_BUFFER];
        assert_eq!(
            stream.mix(&mut out),
            vec![0],
            "ten frames at half the rate last twenty"
        );
    }

    #[test]
    fn the_cubic_passes_through_every_frame_and_between_them_on_a_line() {
        let frames = [[0, 0], [100, -100], [200, -200], [300, -300]];
        assert_eq!(cubic(&frames, 1 << FIXED), Some([100.0, -100.0]));
        assert_eq!(
            cubic(&frames, (1 << FIXED) + (1 << (FIXED - 1))),
            Some([150.0, -150.0])
        );
        assert_eq!(cubic(&frames, 4 << FIXED), None);
    }

    #[test]
    fn the_upsampler_carries_a_steady_level_through_and_keeps_the_band() {
        let mut upsampler = Upsampler::new();
        let input = [1000_i16; BUFFER];
        let mut output = [0.0; OUTPUT_BUFFER];
        upsampler.run(&input, &mut output);
        upsampler.run(&input, &mut output);
        assert!(
            output.iter().all(|&sample| (sample - 1000.0).abs() < 0.5),
            "a steady level"
        );
        let mut upsampler = Upsampler::new();
        let alternating: Vec<i16> = (0..BUFFER)
            .map(|i| {
                if (i / CHANNELS).is_multiple_of(2) {
                    1000
                } else {
                    -1000
                }
            })
            .collect();
        upsampler.run(&alternating, &mut output);
        upsampler.run(&alternating, &mut output);
        let peak = output
            .iter()
            .fold(0.0_f32, |peak, &sample| peak.max(sample.abs()));
        assert!(
            peak < 100.0,
            "the original's Nyquist frequency is filtered out, not imaged: {peak}"
        );
        let mut upsampler = Upsampler::new();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "a test tone"
        )]
        let tone = |call: usize| -> Vec<i16> {
            (0..BUFFER)
                .map(|i| {
                    let t = (call * FRAMES + i / CHANNELS) as f32 / RATE as f32;
                    (10_000.0 * (2.0 * std::f32::consts::PI * 3000.0 * t).sin()) as i16
                })
                .collect()
        };
        for call in 0..3 {
            upsampler.run(&tone(call), &mut output);
        }
        let peak = output
            .iter()
            .fold(0.0_f32, |peak, &sample| peak.max(sample.abs()));
        assert!(
            (9_800.0..10_200.0).contains(&peak),
            "a 3 kHz tone keeps its level: {peak}"
        );
    }

    #[test]
    fn a_line_over_silence_comes_out_sample_for_sample() {
        let mut stream = Stream::new();
        let line: [i16; OUTPUT_BUFFER] =
            std::array::from_fn(|index| i16::try_from(index % 4000).unwrap() - 2000);
        let mut out = [0; OUTPUT_BUFFER];
        for _ in 0..3 {
            stream.mix_over(&line, &mut out);
            assert_eq!(out, line);
        }
    }

    #[test]
    fn original_and_smooth_sounds_mix_together() {
        let mut stream = Stream::new();
        stream.start(
            0,
            Sample::Original(Arc::from([129u8; 2000].as_slice())),
            100,
            11_025,
            11_025,
        );
        stream.start(1, level(OUTPUT_FRAMES * 4, 256), 100, 11_000, 11_000);
        let mut out = [0; OUTPUT_BUFFER];
        stream.mix(&mut out);
        stream.mix(&mut out);
        assert_eq!(
            stream.card()[0],
            202,
            "the card stream holds only the original sound"
        );
        assert_eq!(out[0], 404, "the output holds both: {}", out[0]);
        assert_eq!(out[1], 0);
    }
}
