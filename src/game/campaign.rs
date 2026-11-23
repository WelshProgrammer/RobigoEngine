//! The campaigns' progress: which Roman chapters the chapter list lets the player start, and which
//! World Campaign continents are won. S2 keeps them in `SAVE/MISSION.DAT` and `SAVE/MISSION2.DAT`
//! beside the game. Robigo never writes the player's data, so it keeps its own copy as
//! `campaign.json` in the user's preferences folder, started from the player's files when the
//! data holds them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use s2_formats::save::{MissionProgress, WORLD_CONTINENTS, WorldCampaignProgress};
use s2_ui::screens::chapters::CHAPTERS;

use crate::scene::Start;
use crate::settings::APP;

/// The file in the preferences folder.
const FILE: &str = "campaign.json";

/// Where S2 keeps the progress, inside the game's data.
const MISSION_DAT: &str = "SAVE/MISSION.DAT";
const MISSION2_DAT: &str = "SAVE/MISSION2.DAT";

/// The World Campaign's progress: which continents are won and which one the world screen marks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct World {
    won: Vec<bool>,
    chosen: u8,
}

/// Which chapters are open, by chapter from 0, and the World Campaign's progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Progress {
    open: Vec<bool>,
    /// Missing from a file written before the World Campaign was played.
    #[serde(default)]
    world: Option<World>,
}

impl Default for Progress {
    /// The first chapter alone, for a player with no record at all.
    fn default() -> Self {
        let mut open = vec![false; CHAPTERS];
        open[0] = true;
        Self { open, world: None }
    }
}

impl Progress {
    /// Where the progress lives for this user, or `None` when the platform names no folder.
    pub(crate) fn file() -> Option<PathBuf> {
        s2_platform::preferences_dir(APP).map(|folder| folder.join(FILE))
    }

    /// The progress for the data `start` names: the file at `path` when there is one, otherwise
    /// the data's own `MISSION.DAT`, otherwise the first chapter alone. A file that cannot be read
    /// is logged and passed over.
    pub(crate) fn read(path: Option<&Path>, start: &Start) -> Self {
        let mut progress = path
            .and_then(Self::from_file)
            .or_else(|| Self::from_data(start))
            .unwrap_or_default();
        if progress.world.is_none() {
            progress.world = Self::world_from_data(start);
        }
        progress
    }

    fn from_file(path: &Path) -> Option<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                log::warn!(
                    "{}: {error}; reading the campaign from the game's data",
                    path.display()
                );
                return None;
            }
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(progress) => Some(progress.held()),
            Err(error) => {
                log::warn!(
                    "{}: {error}; reading the campaign from the game's data",
                    path.display()
                );
                None
            }
        }
    }

    fn from_data(start: &Start) -> Option<Self> {
        let bytes = start.data_set().ok()?.read(MISSION_DAT).ok()?;
        match MissionProgress::parse(&bytes) {
            Ok(progress) => Some(Self {
                open: progress.unlocked().to_vec(),
                world: None,
            }),
            Err(error) => {
                log::warn!("{MISSION_DAT}: {error}; the campaign starts at the first chapter");
                None
            }
        }
    }

    fn world_from_data(start: &Start) -> Option<World> {
        let bytes = start.data_set().ok()?.read(MISSION2_DAT).ok()?;
        match WorldCampaignProgress::parse(&bytes) {
            Ok(progress) => Some(World {
                won: progress.won().to_vec(),
                chosen: progress.chosen(),
            }),
            Err(error) => {
                log::warn!("{MISSION2_DAT}: {error}; the World Campaign starts afresh");
                None
            }
        }
    }

    /// Whether each continent is won, by number.
    pub(crate) fn won_continents(&self) -> [bool; WORLD_CONTINENTS] {
        let mut won = [false; WORLD_CONTINENTS];
        if let Some(world) = &self.world {
            for (flag, &held) in won.iter_mut().zip(&world.won) {
                *flag = held;
            }
        }
        won
    }

    /// The continent the world screen marks as chosen.
    pub(crate) fn chosen_continent(&self) -> u8 {
        self.world.as_ref().map_or(0, |world| world.chosen)
    }

    /// Marks `continent` as the chosen one.
    pub(crate) fn choose_continent(&mut self, continent: u8) {
        self.world.get_or_insert_with(World::default).chosen = continent;
    }

    /// Marks `continent` won, as `0x1734a` does. Gives back whether it was not won before.
    pub(crate) fn win_continent(&mut self, continent: u8) -> bool {
        let world = self.world.get_or_insert_with(World::default);
        let at = usize::from(continent);
        if world.won.len() <= at {
            world.won.resize(at + 1, false);
        }
        !std::mem::replace(&mut world.won[at], true)
    }

    /// The list of exactly [`CHAPTERS`] flags, the first chapter always among them.
    fn held(mut self) -> Self {
        self.open.resize(CHAPTERS, false);
        self.open[0] = true;
        self
    }

    /// Whether each chapter is open.
    pub(crate) fn open(&self) -> [bool; CHAPTERS] {
        let mut open = [false; CHAPTERS];
        for (flag, &held) in open.iter_mut().zip(&self.open) {
            *flag = held;
        }
        open
    }

    /// Opens `chapters`, as a won chapter's final event names them, leaving out any past the
    /// list. Gives back whether any was closed before.
    pub(crate) fn open_chapters(&mut self, chapters: &[u16]) -> bool {
        let mut changed = false;
        for &chapter in chapters {
            if let Some(flag) = self.open.get_mut(usize::from(chapter))
                && !*flag
            {
                *flag = true;
                changed = true;
            }
        }
        changed
    }

    /// Writes the progress to `path`, creating its folder if needed. A failure is logged.
    pub(crate) fn write(&self, path: &Path) {
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
                std::fs::write(path, text + "\n")
            });
        if let Err(error) = written {
            log::warn!(
                "{}: {error}; the campaign's progress was not saved",
                path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    #[test]
    fn with_no_record_only_the_first_chapter_is_open() {
        let open = Progress::default().open();
        assert!(open[0]);
        assert!(open[1..].iter().all(|&flag| !flag));
    }

    #[test]
    fn opened_chapters_are_written_and_read_back() {
        let folder = Scratch::new("campaign-round-trip");
        let path = folder.join(FILE);
        let mut progress = Progress::default();
        assert!(progress.open_chapters(&[1, 12]));
        assert!(!progress.open_chapters(&[1]));
        progress.write(&path);
        let read = Progress::from_file(&path).unwrap();
        assert_eq!(read.open()[..3], [true, true, false]);
    }

    #[test]
    fn won_continents_and_the_chosen_one_are_written_and_read_back() {
        let folder = Scratch::new("campaign-world");
        let path = folder.join(FILE);
        let mut progress = Progress::default();
        assert!(progress.win_continent(1));
        assert!(!progress.win_continent(1));
        progress.choose_continent(4);
        progress.write(&path);
        let read = Progress::from_file(&path).unwrap();
        assert_eq!(read.won_continents()[..3], [false, true, false]);
        assert_eq!(read.chosen_continent(), 4);
    }

    #[test]
    fn a_short_or_broken_file_is_held_or_passed_over() {
        let folder = Scratch::new("campaign-short");
        let path = folder.join(FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"open":[false,false,true]}"#).unwrap();
        let read = Progress::from_file(&path).unwrap().open();
        assert_eq!(read[..4], [true, false, true, false]);
        std::fs::write(&path, "not json").unwrap();
        assert!(Progress::from_file(&path).is_none());
    }
}
