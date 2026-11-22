//! The campaign's mission scripts: every file of `DATA/MISSIONS`, copied as it is into
//! `missions/` in lower case. `DATA/MISSIONS/MIS_0001.RTX` is `missions/mis_0001.rtx`. The base
//! game and the Gold Edition ship the same scripts, World Campaign's included, so all of them go
//! into the base mod.

use s2_data::DataSet;

use crate::sprites::Sheets;
use crate::{Error, Result};

/// Where the mission scripts go in a mod.
pub const MISSIONS: &str = "missions";

/// The data folder that holds the mission scripts.
const FOLDER: &str = "DATA/MISSIONS";

/// Where the file at data path `path` goes in a mod, or `None` for a path outside the mission
/// folder.
#[must_use]
pub fn mission_path(path: &str) -> Option<String> {
    let upper = path.replace('\\', "/").to_ascii_uppercase();
    let (folder, name) = upper.rsplit_once('/')?;
    (folder == FOLDER).then(|| format!("{MISSIONS}/{name}").to_ascii_lowercase())
}

impl Sheets<'_> {
    /// The bytes of mission `number`'s script: the loaded mods' copy when the game has mods,
    /// otherwise the file in the data.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when no loaded mod holds the script, [`Error::Io`] when its copy
    /// cannot be read, and [`Error::Data`] for a file the data cannot give.
    pub fn mission_script(&self, number: u16) -> Result<Vec<u8>> {
        let path = s2_sim::script_path(number);
        match (self.stack, mission_path(&path)) {
            (Some(stack), Some(baked)) => stack.read(&baked)?.ok_or(Error::NotInMods(baked)),
            _ => Ok(self.data.read(&path)?),
        }
    }
}

/// Every file in the data's mission folder, by data path.
pub(crate) fn mission_files(data: &DataSet) -> Vec<String> {
    let mut found: Vec<String> = data
        .list_dir(FOLDER)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.kind == s2_data::EntryKind::File)
        .map(|entry| format!("{FOLDER}/{}", entry.name.to_ascii_uppercase()))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mission_path_goes_below_missions() {
        assert_eq!(
            mission_path(&s2_sim::script_path(1)).as_deref(),
            Some("missions/mis_0001.rtx")
        );
        assert_eq!(
            mission_path("data\\missions\\MIS_1000.RTX").as_deref(),
            Some("missions/mis_1000.rtx")
        );
        assert_eq!(mission_path("DATA/MAPS/MISS200.WLD"), None);
        assert_eq!(mission_path("MIS_0001.RTX"), None);
    }
}
