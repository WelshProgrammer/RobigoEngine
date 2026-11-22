//! The game's effects and music: a request goes through the original's slots and voices into the
//! mix, a CD track plays beside it, and the mix goes to the sound card at 44,100 Hz, which SDL
//! converts to its own rate. The jukebox decides what music plays by the original's rules. The
//! ambient sounds and the per-draw rolls get their chances at a fixed number of passes a second,
//! where `S2.EXE` takes one per pass of its main loop at whatever speed the machine runs.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use s2_audio::mixer::{SLOTS, State, Switches};
use s2_audio::{
    Ambient, BOAT, CHANNELS, CdMode, Counters, Cue, Effects, FIRE, FULL, Frames, Jukebox, Line, Mixer,
    OUTPUT_BUFFER, OUTPUT_FRAMES, OUTPUT_RATE, Sample, Source, Stream,
};
use s2_data::{CdAudio, DataSet, TrackReader};
use s2_formats::lst::{Archive, Item, Sound as Stored};
use s2_mods::{AudioStream, ModStack, Rendition, SONGS, Sounds, cd_paths, song_paths};
use s2_platform::{AudioOut, Platform};
use s2_render::Heard;
use s2_ui::windows::Play;

use super::desktop::Music;
use crate::scene::Start;

/// Where the samples live.
const SAMPLES: &str = "DATA/SOUNDDAT/SOUND.LST";

/// Frames kept queued for the card: two calls of the mix, about 46 ms.
const AHEAD: usize = 2 * OUTPUT_FRAMES;

/// The ambient picker's seed. Any will do: the picker has its own generator so the simulation's
/// is never touched, and nothing replays it.
const AMBIENT_SEED: u32 = 305_463_004;

/// The longest stretch of owed passes kept, so a stall does not come back as a burst of sounds.
const MOST_OWED: Duration = Duration::from_millis(250);

/// The highest track number a CD can have.
const LAST_TRACK: u16 = 99;

/// The rate of the counter a random track is picked from, the original's at `0x218474`.
const TICKS_PER_SECOND: u128 = 60;

/// Frames read from a disc image at a time.
const IMAGE_READ: usize = 4096;

/// The card, the library's request side, the mix and the samples it plays from.
pub(crate) struct Sound {
    out: AudioOut,
    library: Mixer,
    stream: Stream,
    samples: Vec<Option<Sample>>,
    started: [bool; SLOTS],
    /// The slots holding a sound of the world rather than of the interface, which a pause cuts.
    world: [bool; SLOTS],
    ambient: Ambient,
    /// Ambient passes a second.
    passes: u16,
    /// Time that has gone by since the last pass.
    owed: Duration,
    /// What music plays.
    jukebox: Jukebox,
    /// The song or CD track playing.
    line: Line,
    /// Whether a song or the disc holds the line.
    on_line: Source,
    /// The songs the mods hold, by number: the OPL3's, then General MIDI's.
    songs: [BTreeMap<u8, PathBuf>; 2],
    /// Which of the two the songs play from.
    rendition: Rendition,
    /// The game menu's music volume, which the songs follow.
    music_volume: u8,
    /// Where the disc's tracks come from.
    disc: Disc,
    /// The disc's playable tracks, in order.
    tracks: Vec<u16>,
    /// When the game started, which the random track counts its ticks from.
    opened: Instant,
    /// Whether a film's sound holds the line, which keeps the jukebox from starting anything.
    filming: bool,
}

impl Sound {
    /// Opens the card and takes the effects from `mods`, or without mods reads the samples from
    /// the data `start` names. `None`, with a warning, when either is missing, which leaves the
    /// game silent.
    pub(crate) fn open(
        platform: &Platform,
        start: &Start,
        passes: u16,
        mods: Option<&ModStack>,
        rendition: Rendition,
    ) -> Option<Self> {
        let loaded = match mods {
            Some(stack) => s2_mods::effects(stack).map_err(|error| error.to_string()),
            None => samples(start).map(|samples| Sounds {
                table: Effects::ORIGINAL,
                samples,
            }),
        };
        let Sounds { table, samples } = match loaded {
            Ok(sounds) => sounds,
            Err(error) => {
                log::warn!("no sound effects: {error}");
                return None;
            }
        };
        let out = match platform.open_audio(OUTPUT_RATE) {
            Ok(out) => out,
            Err(error) => {
                log::warn!("no sound card: {error}");
                return None;
            }
        };
        let mut library = Mixer::new();
        library.switches = Switches::ready();
        library.effects = table;
        let disc = Disc::find(start, mods);
        let tracks = disc.tracks();
        let mut sound = Self {
            out,
            library,
            stream: Stream::new(),
            samples,
            started: [false; SLOTS],
            world: [false; SLOTS],
            ambient: Ambient::new(AMBIENT_SEED),
            passes: passes.max(1),
            owed: Duration::ZERO,
            jukebox: Jukebox::START,
            line: Line::new(),
            on_line: Source::Off,
            songs: [Rendition::Opl3, Rendition::GeneralMidi]
                .map(|each| mods.map(|stack| find_songs(stack, each)).unwrap_or_default()),
            rendition,
            music_volume: FULL,
            disc,
            tracks,
            opened: Instant::now(),
            filming: false,
        };
        if let (Some(&first), Some(&last)) = (sound.tracks.first(), sound.tracks.last()) {
            sound.jukebox.tracks(first, last);
        }
        let cues = sound.jukebox.resume();
        sound.cue(cues);
        Some(sound)
    }

    /// Acts on what the player asked of the music.
    pub(crate) fn ask(&mut self, asked: Music) {
        let ticks = self.ticks();
        let cues = match asked {
            Music::Switch => self.jukebox.switch(),
            Music::Play(Play::Track(track)) => self.jukebox.press_track(track),
            Music::Play(Play::Song(song)) => self.jukebox.press_song(song),
            Music::Play(Play::StopDisc) => self.jukebox.stop_disc(),
            Music::Play(Play::StopMusic) => self.jukebox.stop_music(),
            Music::Play(Play::Mode(mode)) => {
                self.jukebox.press_mode(CdMode::from_stored(mode.stored()), ticks)
            }
        };
        self.cue(cues);
    }

    /// Takes the music back to song 1 for the main menu, unless it is switched off.
    pub(crate) fn enter_menu(&mut self) {
        let cues = self.jukebox.menu();
        self.cue(cues);
    }

    /// Plays a film's `frames` on the line in place of the music, at full gain, until
    /// [`Self::end_film`].
    pub(crate) fn play_film(&mut self, frames: Box<dyn Frames>) {
        self.line.play(frames);
        self.line.set_gain(FULL);
        self.on_line = Source::Off;
        self.filming = true;
    }

    /// Stops the film's sound and starts again the music the jukebox names.
    pub(crate) fn end_film(&mut self) {
        if !self.filming {
            return;
        }
        self.filming = false;
        self.line.stop();
        let cues = self.jukebox.resume();
        self.cue(cues);
    }

    /// The pass the original makes once a main-loop pass: a CD track that has ended gives way
    /// to the next.
    pub(crate) fn carry_on(&mut self) {
        if self.filming {
            return;
        }
        let disc_playing = self.on_line == Source::Disc && self.line.playing();
        let cues = self
            .jukebox
            .advance(!self.tracks.is_empty(), disc_playing, self.ticks());
        self.cue(cues);
    }

    /// Follows the game menu's music volume, 0 to 127, which the songs play at. S2 sets the
    /// volume on the music driver alone, so a CD track plays at its own level.
    pub(crate) fn follow_music(&mut self, volume: u8) {
        self.music_volume = volume.min(FULL);
        if self.on_line == Source::Song {
            self.line.set_gain(self.music_volume);
        }
    }

    /// Plays the songs from `rendition` from now on, starting the song the jukebox is on again
    /// from it, whether or not the rendering it leaves had that song.
    pub(crate) fn play_rendition(&mut self, rendition: Rendition) {
        if rendition == self.rendition {
            return;
        }
        self.rendition = rendition;
        if self.jukebox.source == Source::Song && !self.filming {
            self.cue(vec![Cue::Song(self.jukebox.number)]);
        }
    }

    /// Sets the ambient passes a second.
    pub(crate) fn set_passes(&mut self, passes: u16) {
        self.passes = passes.max(1);
    }

    /// The disc's playable tracks.
    pub(crate) fn tracks(&self) -> &[u16] {
        &self.tracks
    }

    /// Whether a song or the disc is the source, which the game menu's music switch shows.
    pub(crate) fn music_on(&self) -> bool {
        self.jukebox.source != Source::Off
    }

    /// Ticks of the original's 60 Hz counter since the game started.
    fn ticks(&self) -> u32 {
        let ticks = self.opened.elapsed().as_millis() * TICKS_PER_SECOND / 1000;
        u32::try_from(ticks % (u128::from(u32::MAX) + 1)).unwrap_or_default()
    }

    /// Carries out what the jukebox asked of the players.
    fn cue(&mut self, cues: Vec<Cue>) {
        for cue in cues {
            match cue {
                Cue::Song(song) => {
                    let opened = u8::try_from(song).ok().and_then(|song| {
                        [self.rendition, Rendition::Opl3]
                            .iter()
                            .filter_map(|&rendition| self.songs[rendition_index(rendition)].get(&song))
                            .find_map(|file| SongFile::open(file))
                    });
                    self.hold(Source::Song, opened, self.music_volume);
                    if self.on_line == Source::Off {
                        log::debug!("song {song} is in no loaded mod");
                    }
                }
                Cue::Track(track) => {
                    let opened = self.disc.open(track);
                    self.hold(Source::Disc, opened, FULL);
                    if self.on_line == Source::Off {
                        log::debug!("CD track {track} is not on the disc");
                    }
                }
                Cue::StopSong => self.release(Source::Song),
                Cue::StopDisc => self.release(Source::Disc),
            }
        }
    }

    /// Puts `frames` on the line for `source` at `gain`, or stops the line when there are none.
    fn hold(&mut self, source: Source, frames: Option<Box<dyn Frames>>, gain: u8) {
        if let Some(frames) = frames {
            self.line.play(frames);
            self.line.set_gain(gain);
            self.on_line = source;
        } else {
            self.line.stop();
            self.on_line = Source::Off;
        }
    }

    /// Stops the line if `source` holds it.
    fn release(&mut self, source: Source) {
        if self.on_line == source {
            self.line.stop();
            self.on_line = Source::Off;
        }
    }

    /// Asks for the interface's effect `id` as `0x23b0e` does: a click, a roll-up or a refusal,
    /// which play whether or not the game is paused. A refused request is dropped, as in the
    /// original.
    pub(crate) fn play(&mut self, id: u8) {
        self.request(id, false);
    }

    /// Plays effect `id` for the sound test, as the interface's effects play, with the jitter
    /// the game gives it or the table's own volume and rate.
    pub(crate) fn test(&mut self, id: u8, jitter: bool) {
        let asked = if jitter {
            self.library.request(id)
        } else {
            self.library.request_unjittered(id)
        };
        if let Err(refused) = asked {
            log::info!("effect {id} not played: {refused:?}");
        }
    }

    /// Whether effect `id` has a sample to play.
    pub(crate) fn sampled(&self, id: u8) -> bool {
        self.samples
            .get(usize::from(Effects::item(id)))
            .is_some_and(Option::is_some)
    }

    /// Asks for effect `id` on behalf of the world: an animation, an ambient sound or a per-draw
    /// roll. A pause cuts it.
    pub(crate) fn play_world(&mut self, id: u8) {
        self.request(id, true);
    }

    fn request(&mut self, id: u8, world: bool) {
        match self.library.request(id) {
            Ok(place) => {
                if let Some(slot) = self.world.get_mut(place) {
                    *slot = world;
                }
            }
            Err(refused) => log::debug!("effect {id} not played: {refused:?}"),
        }
    }

    /// Stops every sound of the world at once and frees its slot, which a pause does.
    fn cut_world(&mut self) {
        for place in 0..SLOTS {
            if self.world[place] {
                self.stream.stop(place);
                self.started[place] = false;
                self.world[place] = false;
                self.library.finished(place);
            }
        }
    }

    /// Gives the ambient picker and the per-draw rolls their chances for `elapsed` time, at the
    /// set passes a second, from what the view lets the player hear. A paused game is silent
    /// but for the interface: every sound of the world stops and none starts until the pause
    /// comes off.
    pub(crate) fn listen(&mut self, elapsed: Duration, paused: bool, heard: &Heard) {
        if paused {
            self.cut_world();
            self.owed = Duration::ZERO;
            return;
        }
        let pass = Duration::from_secs(1) / u32::from(self.passes);
        self.owed = (self.owed + elapsed).min(MOST_OWED);
        let counters = Counters {
            ducks: heard.ducks,
            pigs: heard.pigs,
            trees: heard.trees,
            terrain: heard.terrain,
            sheep: heard.sheep,
        };
        while self.owed >= pass {
            self.owed -= pass;
            if let Some(id) = self.ambient.pick(counters) {
                self.play_world(id);
            }
            for _ in 0..heard.mills {
                if let Some(id) = self.ambient.mill() {
                    self.play_world(id);
                }
            }
            for _ in 0..heard.ships {
                if let Some(id) = self.ambient.ship() {
                    self.play_world(id);
                }
            }
            for _ in 0..heard.fires {
                self.play_world(FIRE);
            }
            for _ in 0..heard.boats {
                self.play_world(BOAT);
            }
        }
    }

    /// Follows the game menu: whether effects are on and their volume, 0 to 127.
    pub(crate) fn follow(&mut self, on: bool, volume: u8) {
        self.library.switches.effects = on;
        self.stream.set_master(volume);
    }

    /// Mixes until the card has [`AHEAD`] frames waiting, starting what the library has queued
    /// at the top of each call.
    pub(crate) fn pump(&mut self) {
        while self.out.queued() < AHEAD {
            self.library.service();
            self.start_queued();
            let mut line = [0; OUTPUT_BUFFER];
            self.line.fill(&mut line);
            let mut buffer = [0; OUTPUT_BUFFER];
            for place in self.stream.mix_over(&line, &mut buffer) {
                self.started[place] = false;
                self.world[place] = false;
                self.library.finished(place);
            }
            if let Err(error) = self.out.push(&buffer) {
                log::warn!("sound card refused the mix: {error}");
                return;
            }
        }
    }

    /// Hands every slot a voice has just taken to the mix.
    fn start_queued(&mut self) {
        for place in 0..SLOTS {
            let slot = self.library.slots()[place];
            if slot.state != State::Playing || self.started[place] {
                continue;
            }
            let item = usize::from(Effects::item(slot.effect));
            let sample = self.samples.get(item).cloned().flatten();
            let volume = self.library.played_volume(place);
            let nominal = self.library.effects.get(slot.effect).rate;
            let started =
                sample.is_some_and(|sample| self.stream.start(place, sample, volume, slot.rate, nominal));
            if started {
                self.started[place] = true;
            } else {
                self.world[place] = false;
                self.library.finished(place);
            }
        }
    }
}

/// The data set `start` names, with its patch over it.
fn data_set(start: &Start) -> Result<DataSet, String> {
    start.data_set().map_err(|error| error.to_string())
}

/// The samples in `SOUND.LST`, by item, with `None` where an item holds no sample.
fn samples(start: &Start) -> Result<Vec<Option<Sample>>, String> {
    let data = data_set(start)?;
    let bytes = data.read(SAMPLES).map_err(|error| error.to_string())?;
    let archive = Archive::parse(&bytes).map_err(|error| error.to_string())?;
    Ok(archive
        .into_items()
        .into_iter()
        .map(|item| match item {
            Item::Sound(Stored::Pcm(bytes)) => Some(Sample::Original(Arc::from(bytes))),
            _ => None,
        })
        .collect())
}

/// Where the disc's tracks come from: the mods' `cd/` files, the data's disc image when the game
/// runs without mods, or nowhere.
enum Disc {
    Files(BTreeMap<u16, PathBuf>),
    Image(CdAudio),
    None,
}

impl Disc {
    /// The tracks for a game with `mods`, or without them the image the data `start` names.
    fn find(start: &Start, mods: Option<&ModStack>) -> Self {
        if let Some(stack) = mods {
            let files: BTreeMap<u16, PathBuf> = (1..=LAST_TRACK)
                .filter_map(|track| {
                    let number = u8::try_from(track).ok()?;
                    stack.file_of(&cd_paths(number)).map(|(_, file)| (track, file))
                })
                .collect();
            return if files.is_empty() {
                Self::None
            } else {
                Self::Files(files)
            };
        }
        match data_set(start).map(|data| data.cd_audio().cloned()) {
            Ok(Some(audio)) => Self::Image(audio),
            Ok(None) => Self::None,
            Err(error) => {
                log::warn!("no CD audio: {error}");
                Self::None
            }
        }
    }

    /// The playable tracks, in order.
    fn tracks(&self) -> Vec<u16> {
        match self {
            Self::Files(files) => files.keys().copied().collect(),
            Self::Image(audio) => audio
                .tracks()
                .iter()
                .map(|track| u16::from(track.number()))
                .collect(),
            Self::None => Vec::new(),
        }
    }

    /// Track `track` read from its start, or `None` when the disc has no such track or it does
    /// not open.
    fn open(&self, track: u16) -> Option<Box<dyn Frames>> {
        let opened: Result<Box<dyn Frames>, String> = match self {
            Self::Files(files) => {
                let file = files.get(&track)?;
                AudioStream::open(file)
                    .map(|stream| Box::new(FileTrack(stream)) as Box<dyn Frames>)
                    .map_err(|error| error.to_string())
            }
            Self::Image(audio) => audio
                .open_track(u8::try_from(track).ok()?)
                .map(|reader| {
                    Box::new(ImageTrack {
                        reader,
                        bytes: vec![0; IMAGE_READ * FRAME_BYTES],
                    }) as Box<dyn Frames>
                })
                .map_err(|error| error.to_string()),
            Self::None => return None,
        };
        opened
            .map_err(|error| log::warn!("CD track {track}: {error}"))
            .ok()
    }
}

/// A track from a mod's audio file.
struct FileTrack(AudioStream);

impl Frames for FileTrack {
    fn rate(&self) -> u32 {
        self.0.rate()
    }

    fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
        self.0.read(frames).unwrap_or_else(|error| {
            log::warn!("{error}");
            false
        })
    }
}

/// Bytes in one frame of CD audio: two 16-bit samples, left then right.
const FRAME_BYTES: usize = 4;

/// A track read from the disc image's raw audio.
struct ImageTrack {
    reader: TrackReader,
    bytes: Vec<u8>,
}

impl Frames for ImageTrack {
    fn rate(&self) -> u32 {
        CdAudio::SAMPLE_RATE
    }

    fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
        let read = match self.reader.read(&mut self.bytes) {
            Ok(read) => read,
            Err(error) => {
                log::warn!("CD audio: {error}");
                return false;
            }
        };
        frames.extend(
            self.bytes[..read - read % FRAME_BYTES]
                .as_chunks::<FRAME_BYTES>()
                .0
                .iter()
                .map(|&[l0, l1, r0, r1]| [i16::from_le_bytes([l0, l1]), i16::from_le_bytes([r0, r1])]),
        );
        read > 0
    }
}

/// Where `rendition`'s songs sit in [`Sound`]'s two lists.
const fn rendition_index(rendition: Rendition) -> usize {
    match rendition {
        Rendition::Opl3 => 0,
        Rendition::GeneralMidi => 1,
    }
}

/// The songs of `rendition` the mods hold, by number.
fn find_songs(stack: &ModStack, rendition: Rendition) -> BTreeMap<u8, PathBuf> {
    (1..=SONGS)
        .filter_map(|song| {
            stack
                .file_of(&song_paths(rendition, song))
                .map(|(_, file)| (song, file))
        })
        .collect()
}

/// A song read from its file, going back round its repeat when it reaches the end of it.
struct SongFile {
    path: PathBuf,
    stream: AudioStream,
    repeat: Option<(u64, u64)>,
    /// Frames handed on from the file so far, counted from its start.
    at: u64,
}

impl SongFile {
    fn open(path: &std::path::Path) -> Option<Box<dyn Frames>> {
        let stream = AudioStream::open(path)
            .map_err(|error| log::warn!("{error}"))
            .ok()?;
        let repeat = stream.repeat();
        Some(Box::new(Self {
            path: path.to_owned(),
            stream,
            repeat,
            at: 0,
        }))
    }

    /// Opens the file again and reads past the frames before the repeat, handing on the rest.
    fn go_round(&mut self, start: u64, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
        let Ok(stream) = AudioStream::open(&self.path) else {
            return false;
        };
        self.stream = stream;
        let mut read = Vec::new();
        let mut passed = 0u64;
        while passed < start {
            read.clear();
            if !self.read_stream(&mut read) {
                return false;
            }
            passed += count(&read);
        }
        let over = usize::try_from(passed - start).unwrap_or_default();
        frames.extend_from_slice(&read[read.len() - over..]);
        self.at = passed;
        true
    }

    fn read_stream(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
        self.stream.read(frames).unwrap_or_else(|error| {
            log::warn!("{error}");
            false
        })
    }
}

/// How many frames `frames` holds.
fn count(frames: &[[i16; CHANNELS]]) -> u64 {
    u64::try_from(frames.len()).unwrap_or(u64::MAX)
}

impl Frames for SongFile {
    fn rate(&self) -> u32 {
        self.stream.rate()
    }

    fn read(&mut self, frames: &mut Vec<[i16; CHANNELS]>) -> bool {
        let before = frames.len();
        let more = self.read_stream(frames);
        let got = count(&frames[before..]);
        let Some((start, length)) = self.repeat else {
            self.at += got;
            return more;
        };
        let end = start + length;
        if self.at + got < end && more {
            self.at += got;
            return true;
        }
        frames.truncate(before + usize::try_from(end.saturating_sub(self.at)).unwrap_or_default());
        self.go_round(start, frames)
    }
}
