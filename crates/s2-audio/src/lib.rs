//! Sound effects: the original's request path, its slots, its voices and its mix.
//!
//! `S2.EXE` asks for an effect by id and the Miles library linked into it does the rest. All of
//! it is integer arithmetic and is reproduced here: the table the volume and rate come from, the
//! jitter drawn from the C library's generator, the eight slots a request can land in, the twelve
//! voices a later pass hands them to, and the mix that turns the playing sounds into the 11,025 Hz
//! stream the original sends the card. The mix goes out at 44,100 Hz, with a second, smooth path
//! for samples that are not the original's.
//!
//! [`Jukebox`] keeps what music plays, a song or a CD track, by the same rules as `S2.EXE`.
//!
//! The device is not here. A backend takes the stream [`Stream::mix`] makes and plays it.

#![forbid(unsafe_code)]

pub mod ambient;
pub mod fm;
pub mod jukebox;
pub mod line;
pub mod mixer;
pub mod rand;
pub mod render;
pub mod sequencer;
pub mod stream;
pub mod table;

pub use ambient::{Ambient, BOAT, Counters, DUCK, FIRE, MILL, PIGS, SHEEP, SHIP, TERRAIN, TREES};
pub use fm::{FmDriver, OPL_CHANNELS};
pub use jukebox::{CdMode, Cue, Jukebox, Source};
pub use line::{FULL, Frames, Line};
pub use mixer::{
    CENTRE, Mixer, PRIORITY, Refused, SLOTS, Slot, State, Switches, VOICES, Voice, service_order,
};
pub use rand::{LARGEST, Rand};
pub use render::{GeneralMidi, Instrument, MIDI_RATE, OPL_RATE, Opl3, Rendered, Song, render, render_song};
pub use sequencer::{NOTE_SLOTS, Score, Sequencer};
pub use stream::{
    BUFFER, CHANNELS, FRAMES, OUTPUT_BUFFER, OUTPUT_FRAMES, OUTPUT_RATE, RATE, SOUNDS, Sample, Step, Stream,
    UPSAMPLING, Upsampler, gain,
};
pub use table::{
    EFFECT_IDS, Effect, Effects, EffectsError, ITEM_BASE, LAST_EFFECT, MOST_JITTER, MOST_VOLUME, Playback,
};
