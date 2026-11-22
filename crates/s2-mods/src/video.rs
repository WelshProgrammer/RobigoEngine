//! The videos: each `.SMK` file of the original `VIDEO` folder, copied as it is into `video/`
//! under its name in lower case. `VIDEO/INTRO.SMK` is `video/intro.smk`.

use s2_data::DataSet;

use crate::sprites::Sheets;
use crate::{Error, Result};

/// Where the videos go in a mod.
pub const VIDEOS: &str = "video";

/// The folder of the original data that holds the videos.
const FOLDER: &str = "VIDEO";

/// The ending of a video file in the original data.
const ENDING: &str = ".SMK";

/// Where the video at data path `path` goes in a mod, or `None` for a path outside the video
/// folder.
#[must_use]
pub fn video_path(path: &str) -> Option<String> {
    let upper = path.replace('\\', "/").to_ascii_uppercase();
    let (folder, name) = upper.rsplit_once('/')?;
    (folder == FOLDER && name.ends_with(ENDING)).then(|| format!("{VIDEOS}/{}", name.to_ascii_lowercase()))
}

impl Sheets<'_> {
    /// The bytes of the video at data path `path`, such as `VIDEO/INTRO.SMK`: the loaded mods'
    /// copy when the game has mods, otherwise the file in the data.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when no loaded mod holds the video, [`Error::Io`] when its copy
    /// cannot be read, and [`Error::Data`] for a file the data cannot give.
    pub fn video(&self, path: &str) -> Result<Vec<u8>> {
        match (self.stack, video_path(path)) {
            (Some(stack), Some(baked)) => stack.read(&baked)?.ok_or(Error::NotInMods(baked)),
            _ => Ok(self.data.read(path)?),
        }
    }
}

/// Every video in `data`, by data path in upper case, sorted.
pub(crate) fn video_files(data: &DataSet) -> Vec<String> {
    let mut found: Vec<String> = data
        .list_dir(FOLDER)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.kind == s2_data::EntryKind::File)
        .map(|entry| format!("{FOLDER}/{}", entry.name.to_ascii_uppercase()))
        .filter(|path| path.ends_with(ENDING))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn videos_keep_their_names_in_lower_case() {
        assert_eq!(video_path("VIDEO/INTRO.SMK").as_deref(), Some("video/intro.smk"));
        assert_eq!(
            video_path("video\\intro2.smk").as_deref(),
            Some("video/intro2.smk")
        );
        assert_eq!(video_path("VIDEO/README.TXT"), None);
        assert_eq!(video_path("DATA/INTRO.SMK"), None);
    }
}
