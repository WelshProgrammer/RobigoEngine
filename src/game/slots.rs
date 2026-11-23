//! Robigo's saved games: eleven slots, as S2 keeps eleven in `SAVE` with `DIR.DAT` naming them.
//! Robigo never writes the player's data, so the slots live in a `saves` folder in the user's
//! preferences folder, each a save file, with `directory.json` holding each slot's number and
//! name as `DIR.DAT` does. The last slot is the automatic save's, which the load screen offers
//! and the save screen does not.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use s2_formats::text::cp437;
use s2_ui::windows::saves;

use crate::settings::APP;

/// The folder in the preferences folder.
const FOLDER: &str = "saves";

/// The file naming the slots.
const DIRECTORY: &str = "directory.json";

/// The slot the automatic save writes to (`[0x1f7925]`, 10).
pub(crate) const AUTOMATIC: usize = saves::SLOTS - 1;

/// How many steps each automatic save period runs, by the save screen's buttons, the first
/// being off (`0x1fc0c4`). The buttons call them 15 to 120 minutes.
pub(crate) const PERIODS: [u32; saves::PERIODS] = [0, 7_200, 14_400, 28_800, 57_600];

/// One slot's line, as `DIR.DAT` holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    /// The save's number, one more than the last save's.
    pub(crate) number: u32,
    /// What the save is called.
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Directory {
    /// How many saves have been made, which numbers the next.
    count: u32,
    slots: Vec<Option<Entry>>,
}

/// The saved games in one folder.
#[derive(Debug, Clone)]
pub(crate) struct Slots {
    folder: PathBuf,
    directory: Directory,
}

impl Slots {
    /// The slots in the preferences folder, or `None` when the platform names no folder.
    pub(crate) fn open() -> Option<Self> {
        s2_platform::preferences_dir(APP).map(|folder| Self::in_folder(&folder.join(FOLDER)))
    }

    /// The slots in `folder`. A directory file that cannot be read counts as no saves, with a
    /// warning.
    pub(crate) fn in_folder(folder: &Path) -> Self {
        let path = folder.join(DIRECTORY);
        let directory = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                log::warn!("{}: {error}; the saved games are not listed", path.display());
                Directory::default()
            }),
            Err(_) => Directory::default(),
        };
        Self {
            folder: folder.to_owned(),
            directory,
        }
    }

    /// Each slot's number and name in code page 437, `None` for a slot with no save.
    pub(crate) fn entries(&self) -> Vec<Option<(i32, Vec<u8>)>> {
        (0..saves::SLOTS)
            .map(|slot| {
                self.entry(slot).map(|entry| {
                    let name = entry
                        .name
                        .chars()
                        .map(|c| cp437::encode_char(c).unwrap_or(b'?'))
                        .collect();
                    (i32::try_from(entry.number).unwrap_or(i32::MAX), name)
                })
            })
            .collect()
    }

    /// Slot `slot`'s line, when it holds a save whose file is there.
    fn entry(&self, slot: usize) -> Option<&Entry> {
        self.directory
            .slots
            .get(slot)
            .and_then(Option::as_ref)
            .filter(|_| self.path(slot).is_file())
    }

    /// Where slot `slot`'s save is written.
    pub(crate) fn path(&self, slot: usize) -> PathBuf {
        self.folder.join(format!("slot{slot:02}.save"))
    }

    /// The folder the saves are written to, made if needed.
    ///
    /// # Errors
    ///
    /// The I/O error from making it.
    pub(crate) fn make_folder(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.folder)
    }

    /// Names slot `slot` `name` with the next number, as `0x4c708` does once its save is
    /// written, and writes the directory. A failure is logged.
    pub(crate) fn record(&mut self, slot: usize, name: String) {
        self.directory.count = self.directory.count.saturating_add(1);
        if self.directory.slots.len() < saves::SLOTS {
            self.directory.slots.resize(saves::SLOTS, None);
        }
        if let Some(held) = self.directory.slots.get_mut(slot) {
            *held = Some(Entry {
                number: self.directory.count,
                name,
            });
        }
        let path = self.folder.join(DIRECTORY);
        let written = serde_json::to_string_pretty(&self.directory)
            .map_err(std::io::Error::other)
            .and_then(|text| std::fs::write(&path, text + "\n"));
        if let Err(error) = written {
            log::warn!("{}: {error}; the save is not listed", path.display());
        }
    }

    /// The slot holding the save with the highest number, which Resume last game loads
    /// (`0x4c463`).
    pub(crate) fn latest(&self) -> Option<usize> {
        (0..saves::SLOTS)
            .filter_map(|slot| self.entry(slot).map(|entry| (entry.number, slot)))
            .max()
            .map(|(_, slot)| slot)
    }
}

/// What a save of a game on the map titled `title` is called after `steps` steps:
/// `'<title>'-h:mm`, from the step count over 8 as seconds (`0x4c708`, `0x534f4`).
pub(crate) fn name(title: &str, steps: u32) -> String {
    let seconds = steps >> 3;
    format!("'{title}'-{}:{:02}", seconds / 3_600, seconds / 60 % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    #[test]
    fn a_save_is_named_from_the_map_and_the_clock() {
        assert_eq!(name("Africa", 0), "'Africa'-0:00");
        assert_eq!(name("Africa", 8 * (3_600 + 5 * 60)), "'Africa'-1:05");
    }

    #[test]
    fn saves_are_numbered_listed_and_the_latest_found() {
        let folder = Scratch::new("slots-listed");
        let mut slots = Slots::in_folder(&folder);
        assert_eq!(slots.latest(), None);
        for slot in [3, 0] {
            std::fs::write(slots.path(slot), b"save").unwrap();
        }
        slots.record(3, "'Europe'-0:10".into());
        slots.record(0, "'Japan'-2:00".into());
        let read = Slots::in_folder(&folder);
        let entries = read.entries();
        assert_eq!(entries[3], Some((1, b"'Europe'-0:10".to_vec())));
        assert_eq!(entries[0], Some((2, b"'Japan'-2:00".to_vec())));
        assert_eq!(read.latest(), Some(0));
        std::fs::remove_file(read.path(0)).unwrap();
        assert_eq!(read.latest(), Some(3), "a slot whose file is gone is not offered");
    }
}
