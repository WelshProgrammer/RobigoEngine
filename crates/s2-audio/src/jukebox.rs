//! What plays: a song, a CD track or nothing, as `S2.EXE` keeps it in the words at `0x1f7909`
//! and `0x1f790b`.
//!
//! The jukebox holds only that state and the rules that change it. Each call says what the
//! players must do as a list of [`Cue`]s; whoever owns the song player and the disc carries them
//! out and reports back whether a track is still playing.

/// Where the music comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Nothing plays.
    Off,
    /// A song from the MIDI player.
    Song,
    /// A track of the disc.
    Disc,
}

/// How the disc carries on once a track ends (`0x1f7929`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdMode {
    /// The same track plays again.
    Repeat,
    /// A track is picked from the tick count.
    Random,
    /// The next track plays, back to the first past the last.
    Through,
}

impl CdMode {
    /// The mode the original stores as `stored`; any value but 1 and 2 repeats.
    #[must_use]
    pub const fn from_stored(stored: u32) -> Self {
        match stored {
            1 => Self::Random,
            2 => Self::Through,
            _ => Self::Repeat,
        }
    }
}

/// What the players must do, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// Stop the song and play song `n` from its start.
    Song(u16),
    /// Stop the song.
    StopSong,
    /// Play track `n` of the disc from its start.
    Track(u16),
    /// Stop the disc.
    StopDisc,
}

/// The track word's value once the disc has stopped.
const NO_TRACK: u16 = u16::MAX;

/// The song the music switch starts.
const FIRST_SONG: u16 = 1;

/// The state of the music and the disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Jukebox {
    /// Where the music comes from.
    pub source: Source,
    /// The song or the track, from 1; [`u16::MAX`] once the disc has stopped.
    pub number: u16,
    /// How the disc carries on.
    pub mode: CdMode,
    /// The first playable track, which the CD player sets when it opens.
    pub first: u16,
    /// The last playable track.
    pub last: u16,
}

impl Default for Jukebox {
    fn default() -> Self {
        Self::START
    }
}

impl Jukebox {
    /// The state the executable starts with: song 1, the disc repeating, no tracks known yet.
    pub const START: Self = Self {
        source: Source::Song,
        number: FIRST_SONG,
        mode: CdMode::Repeat,
        first: 0,
        last: 0,
    };

    /// Plays song `song` (`0x140ac`), restarting it when it already plays. Song 0 only stops the
    /// music.
    pub fn song(&mut self, song: u16) -> Vec<Cue> {
        self.source = Source::Song;
        self.number = song;
        if song == 0 {
            vec![Cue::StopSong]
        } else {
            vec![Cue::Song(song)]
        }
    }

    /// Stops the music (`0x14110`), keeping the song's number.
    pub fn stop_music(&mut self) -> Vec<Cue> {
        self.source = Source::Off;
        vec![Cue::StopSong]
    }

    /// Plays track `track` of the disc (`0x14120`); the music carries on unless the caller stops
    /// it first.
    pub fn track(&mut self, track: u16) -> Vec<Cue> {
        self.source = Source::Disc;
        self.number = track;
        vec![Cue::Track(track)]
    }

    /// Stops the disc (`0x14155`), forgetting its track.
    pub fn stop_disc(&mut self) -> Vec<Cue> {
        self.source = Source::Off;
        self.number = NO_TRACK;
        vec![Cue::StopDisc]
    }

    /// Stops both and starts again what the source names (`0x140cf`).
    pub fn resume(&mut self) -> Vec<Cue> {
        let (source, number) = (self.source, self.number);
        let mut cues = self.stop_music();
        cues.extend(self.stop_disc());
        match source {
            Source::Song => cues.extend(self.song(number)),
            Source::Disc => cues.extend(self.track(number)),
            Source::Off => {}
        }
        cues
    }

    /// The game menu's music switch (`0x4b656`): stops both when anything plays, otherwise
    /// starts song 1.
    pub fn switch(&mut self) -> Vec<Cue> {
        if self.source == Source::Off {
            self.song(FIRST_SONG)
        } else {
            let mut cues = self.stop_music();
            cues.extend(self.stop_disc());
            cues
        }
    }

    /// A return to the main menu, which S2 leaves to whatever plays. Song 1 takes over from any
    /// other song and from the disc. Song 1 carries on, and music switched off stays off.
    pub fn menu(&mut self) -> Vec<Cue> {
        match (self.source, self.number) {
            (Source::Off, _) | (Source::Song, FIRST_SONG) => Vec::new(),
            _ => self.press_song(FIRST_SONG),
        }
    }

    /// A press of the MIDI player's song button `song`: the disc stops, then the song plays.
    pub fn press_song(&mut self, song: u16) -> Vec<Cue> {
        let mut cues = self.stop_disc();
        cues.extend(self.song(song));
        cues
    }

    /// A press of the CD player's track button `track`: the music stops, then the track plays.
    pub fn press_track(&mut self, track: u16) -> Vec<Cue> {
        let mut cues = self.stop_music();
        cues.extend(self.track(track));
        cues
    }

    /// A press of one of the CD player's mode buttons. Random also plays a track picked from
    /// `ticks`, the 60 Hz count since the game started, with the music stopped first.
    pub fn press_mode(&mut self, mode: CdMode, ticks: u32) -> Vec<Cue> {
        self.mode = mode;
        if mode != CdMode::Random {
            return Vec::new();
        }
        let mut cues = self.stop_music();
        cues.extend(self.track(self.random(ticks)));
        cues
    }

    /// The CD player opening on a disc whose playable tracks run from `first` to `last`.
    pub fn tracks(&mut self, first: u16, last: u16) {
        self.first = first;
        self.last = last;
    }

    /// The pass the main loop makes (`0x1417a`): with the disc as the source, a disc in the
    /// drive and a track set, a track that has ended gives way to the next the mode picks.
    pub fn advance(&mut self, disc: bool, playing: bool, ticks: u32) -> Vec<Cue> {
        if self.source != Source::Disc || !disc || self.number == NO_TRACK || playing {
            return Vec::new();
        }
        let next = match self.mode {
            CdMode::Repeat => self.number,
            CdMode::Random => self.random(ticks),
            CdMode::Through => {
                let next = self.number.wrapping_add(1);
                if next > self.last { self.first } else { next }
            }
        };
        self.number = next;
        vec![Cue::Track(next)]
    }

    /// A track from the tick count: `ticks` modulo the number of tracks, from the first.
    fn random(self, ticks: u32) -> u16 {
        let span = u32::from(self.last.saturating_sub(self.first)) + 1;
        let offset = u16::try_from(ticks % span).unwrap_or_default();
        self.first.saturating_add(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc() -> Jukebox {
        let mut jukebox = Jukebox::START;
        jukebox.tracks(2, 9);
        jukebox
    }

    #[test]
    fn starts_on_song_one_and_resumes_it() {
        let mut jukebox = Jukebox::START;
        assert_eq!(
            jukebox.resume(),
            [Cue::StopSong, Cue::StopDisc, Cue::Song(1)],
            "the resume stops both and plays the song the words name"
        );
        assert_eq!((jukebox.source, jukebox.number), (Source::Song, 1));
    }

    #[test]
    fn the_switch_stops_everything_and_comes_back_on_song_one() {
        let mut jukebox = disc();
        jukebox.press_track(5);
        assert_eq!(jukebox.switch(), [Cue::StopSong, Cue::StopDisc]);
        assert_eq!((jukebox.source, jukebox.number), (Source::Off, NO_TRACK));
        assert_eq!(
            jukebox.resume(),
            [Cue::StopSong, Cue::StopDisc],
            "nothing to resume"
        );
        assert_eq!(
            jukebox.switch(),
            [Cue::Song(1)],
            "song 1, not the track that played"
        );
    }

    #[test]
    fn the_main_menu_takes_the_music_back_to_song_one() {
        let mut jukebox = disc();
        assert_eq!(jukebox.menu(), [], "song 1 carries on");
        jukebox.press_song(5);
        assert_eq!(jukebox.menu(), [Cue::StopDisc, Cue::Song(1)]);
        jukebox.press_track(4);
        assert_eq!(jukebox.menu(), [Cue::StopDisc, Cue::Song(1)]);
        assert_eq!((jukebox.source, jukebox.number), (Source::Song, 1));
        jukebox.switch();
        assert_eq!(jukebox.menu(), [], "music switched off stays off");
    }

    #[test]
    fn a_track_carries_on_by_its_mode() {
        let mut jukebox = disc();
        assert_eq!(jukebox.press_track(8), [Cue::StopSong, Cue::Track(8)]);
        assert_eq!(jukebox.advance(true, true, 0), [], "still playing");
        assert_eq!(jukebox.advance(true, false, 0), [Cue::Track(8)], "repeat");
        jukebox.press_mode(CdMode::Through, 0);
        assert_eq!(jukebox.advance(true, false, 0), [Cue::Track(9)]);
        assert_eq!(
            jukebox.advance(true, false, 0),
            [Cue::Track(2)],
            "back to the first past the last"
        );
        assert_eq!(jukebox.advance(false, false, 0), [], "no disc");
    }

    #[test]
    fn random_picks_from_the_tick_count() {
        let mut jukebox = disc();
        assert_eq!(
            jukebox.press_mode(CdMode::Random, 13),
            [Cue::StopSong, Cue::Track(7)],
            "13 modulo 8 tracks is 5, from track 2"
        );
        assert_eq!(jukebox.advance(true, false, 16), [Cue::Track(2)]);
        assert_eq!(jukebox.mode, CdMode::Random);
    }

    #[test]
    fn a_song_stops_the_disc_and_the_advance_leaves_songs_alone() {
        let mut jukebox = disc();
        jukebox.press_track(3);
        assert_eq!(jukebox.press_song(4), [Cue::StopDisc, Cue::Song(4)]);
        assert_eq!(jukebox.advance(true, false, 0), []);
        assert_eq!(jukebox.song(0), [Cue::StopSong], "song 0 only stops");
    }
}
