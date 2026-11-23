//! The maps: every file of the original map folders, copied as it is into `maps/`, the folder
//! below `DATA` lower case. `DATA/MAPS3/OMAP00.WLD` is `maps/maps3/omap00.wld`. The World
//! Campaign's and the new maps' folders go into the `gold` mod.
//!
//! Replays and saves name a map by its data path, so the game asks for maps by data path and
//! [`Sheets::map_file`] finds the mod's copy.

use s2_data::DataSet;

use crate::sprites::Sheets;
use crate::{Error, Result};

/// Where the maps go in a mod.
pub const MAPS: &str = "maps";

/// The folders of the original data that hold maps.
const FOLDERS: [&str; 4] = ["DATA/MAPS", "DATA/MAPS2", "DATA/MAPS3", "DATA/MAPS4"];

/// Where the map file at data path `path` goes in a mod, or `None` for a path outside the map
/// folders, such as a savegame slot.
#[must_use]
pub fn map_path(path: &str) -> Option<String> {
    let upper = path.replace('\\', "/").to_ascii_uppercase();
    let (folder, name) = upper.rsplit_once('/')?;
    FOLDERS
        .contains(&folder)
        .then(|| format!("{MAPS}/{}/{}", folder.trim_start_matches("DATA/"), name).to_ascii_lowercase())
}

impl Sheets<'_> {
    /// The bytes of the map file at data path `path`: the loaded mods' copy when the game has
    /// mods and the path is in a map folder, otherwise the file in the data.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when no loaded mod holds the map, [`Error::Io`] when its copy cannot
    /// be read, and [`Error::Data`] for a file the data cannot give.
    pub fn map_file(&self, path: &str) -> Result<Vec<u8>> {
        match (self.stack, map_path(path)) {
            (Some(stack), Some(baked)) => stack.read(&baked)?.ok_or(Error::NotInMods(baked)),
            _ => Ok(self.data.read(path)?),
        }
    }

    /// Whether [`Self::map_file`] would find `path`.
    #[must_use]
    pub fn has_map_file(&self, path: &str) -> bool {
        match (self.stack, map_path(path)) {
            (Some(stack), Some(baked)) => stack.file(&baked).is_some(),
            _ => self.data.exists(path),
        }
    }

    /// Whether the game takes `path` from the mods: it has mods and the path is in a map folder.
    #[must_use]
    pub fn maps_from_mods(&self, path: &str) -> bool {
        self.stack.is_some() && map_path(path).is_some()
    }
}

impl Sheets<'_> {
    /// The maps in the data folder `folder`, such as `DATA/MAPS3`, whose names end in `ending`,
    /// such as `.WLD`, by data path in upper case and sorted: the loaded mods' when the game has
    /// mods, otherwise the data's.
    #[must_use]
    pub fn maps_in(&self, folder: &str, ending: &str) -> Vec<String> {
        let folder = folder.replace('\\', "/").to_ascii_uppercase();
        let ending = ending.to_ascii_uppercase();
        let names: Vec<String> = match self.stack {
            Some(stack) => {
                let baked = format!("{MAPS}/{}", folder.trim_start_matches("DATA/")).to_ascii_lowercase();
                stack.list(&baked)
            }
            None => self
                .data
                .list_dir(&folder)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry.kind == s2_data::EntryKind::File)
                .map(|entry| entry.name)
                .collect(),
        };
        let mut found: Vec<String> = names
            .into_iter()
            .map(|name| format!("{folder}/{}", name.to_ascii_uppercase()))
            .filter(|path| path.ends_with(&ending))
            .collect();
        found.sort();
        found.dedup();
        found
    }
}

/// Every file in the data's map folders, by data path.
pub(crate) fn map_files(data: &DataSet) -> Vec<String> {
    let mut found: Vec<String> = FOLDERS
        .iter()
        .flat_map(|folder| {
            data.list_dir(folder)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry.kind == s2_data::EntryKind::File)
                .map(move |entry| format!("{folder}/{}", entry.name.to_ascii_uppercase()))
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_folder_path_goes_below_maps() {
        assert_eq!(
            map_path("DATA/MAPS3/OMAP00.WLD").as_deref(),
            Some("maps/maps3/omap00.wld")
        );
        assert_eq!(
            map_path("data\\maps\\WORLD100.DAT").as_deref(),
            Some("maps/maps/world100.dat")
        );
        assert_eq!(map_path("SAVE/WORLD900.DAT"), None);
        assert_eq!(map_path("DATA/MAPS3/SUB/OMAP00.WLD"), None);
        assert_eq!(map_path("OMAP00.WLD"), None);
    }
}
