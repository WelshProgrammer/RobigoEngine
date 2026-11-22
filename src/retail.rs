//! The folder the player's copy of Settlers 2 goes in: `RETAIL` beside the game's binary, or in
//! the preferences folder for an install whose own folder cannot be written. `RETAIL/PATCH` holds
//! the v1.51 patch for a copy older than that. Each folder carries a note saying what goes in it,
//! the files under `dist/RETAIL`, which an installer or an archive of the game ships as they are.

use std::path::{Path, PathBuf};

use s2_data::DataSet;

use crate::settings;

/// The folder's name.
const FOLDER: &str = "RETAIL";

/// The patch's folder inside it.
const PATCH: &str = "PATCH";

/// The notes each folder carries, by file name.
const NOTE: (&str, &str) = ("RETAIL.txt", include_str!("../dist/RETAIL/RETAIL.txt"));
const PATCH_NOTE: (&str, &str) = ("PATCH.txt", include_str!("../dist/RETAIL/PATCH/PATCH.txt"));

/// The game's executable, which an installed game or a CD's files have at the top.
const GAME: &str = "S2.EXE";

/// The endings of the files a CD image is made of: sheets, images and the tracks a sheet names.
const IMAGE_FILES: [&str; 9] = ["cue", "ins", "iso", "bin", "gog", "wav", "ogg", "flac", "mp3"];

/// How many bytes a copy moves between reports of how far it is.
const CHUNK: usize = 1 << 20;

/// The endings of a CD image's sheet or of an image the data can be read from.
const IMAGES: [&str; 3] = ["cue", "ins", "iso"];

/// The places a `RETAIL` folder can be, in the order they are looked in.
fn places() -> Vec<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|binary| binary.parent().map(|folder| folder.join(FOLDER)));
    let preferences = s2_platform::preferences_dir(settings::APP).map(|folder| folder.join(FOLDER));
    beside.into_iter().chain(preferences).collect()
}

/// The data the first `RETAIL` folder that holds any holds, and the patch over it when its
/// `PATCH` folder holds one.
pub(crate) fn find() -> Option<(PathBuf, Option<PathBuf>)> {
    places().iter().find_map(|folder| held(folder))
}

/// The data `folder` holds, and the patch over it when its `PATCH` folder holds one.
pub(crate) fn held(folder: &Path) -> Option<(PathBuf, Option<PathBuf>)> {
    data_in(folder).map(|data| (data, patch_in(folder)))
}

/// What in `folder` reads as game data: the folder itself, as an installed game or a CD's files,
/// or else the first sheet or image in it.
fn data_in(folder: &Path) -> Option<PathBuf> {
    if !folder.is_dir() {
        return None;
    }
    if DataSet::discover(folder).is_ok() {
        return Some(folder.to_path_buf());
    }
    let mut images: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ending| ending.to_str())
                .is_some_and(|ending| IMAGES.iter().any(|image| image.eq_ignore_ascii_case(ending)))
        })
        .collect();
    images.sort();
    images.into_iter().find(|image| DataSet::discover(image).is_ok())
}

/// `folder`'s `PATCH` folder when it holds anything besides its note.
fn patch_in(folder: &Path) -> Option<PathBuf> {
    let patch = folder.join(PATCH);
    let holds = std::fs::read_dir(&patch)
        .ok()?
        .filter_map(Result::ok)
        .any(|entry| entry.file_name() != PATCH_NOTE.0);
    holds.then_some(patch)
}

/// Where the player is to copy the game: the first `RETAIL` folder there is, or else one made
/// with its notes, beside the binary when that folder can be written and in the preferences
/// folder when not. `None` when neither can be made.
pub(crate) fn set_up() -> Option<PathBuf> {
    let places = places();
    if let Some(found) = places.iter().find(|folder| folder.is_dir()) {
        return Some(found.clone());
    }
    places.into_iter().find(|folder| make(folder).is_ok())
}

/// The files of the game in `source`, each with its size, as paths inside it: everything in an
/// installed game's folder or a CD's files, and for a folder that holds a CD image, the files
/// at its top an image is made of.
pub(crate) fn files(source: &Path) -> Vec<(PathBuf, u64)> {
    let top: Vec<PathBuf> = std::fs::read_dir(source)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    let named = |path: &Path, name: &str| {
        path.file_name()
            .is_some_and(|file| file.eq_ignore_ascii_case(name))
    };
    if top.iter().any(|path| named(path, GAME)) {
        return every_file(source);
    }
    let mut found = Vec::new();
    {
        found.extend(top.into_iter().filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ending| ending.to_str())
                    .is_some_and(|ending| IMAGE_FILES.iter().any(|image| image.eq_ignore_ascii_case(ending)))
        }));
    }
    sized(source, found)
}

/// Every file in `source` and the folders under it, each with its size, as paths inside it:
/// what a copy of the v1.51 patch's files takes.
pub(crate) fn every_file(source: &Path) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    let mut folders = vec![source.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push(path);
            }
        }
    }
    sized(source, found)
}

/// `found` in order, each as its path inside `source` with its size.
fn sized(source: &Path, mut found: Vec<PathBuf>) -> Vec<(PathBuf, u64)> {
    found.sort();
    found
        .into_iter()
        .filter_map(|path| {
            let size = std::fs::metadata(&path).ok()?.len();
            Some((path.strip_prefix(source).ok()?.to_path_buf(), size))
        })
        .collect()
}

/// The `RETAIL` folder the game keeps its own copy of the player's game in: the one in the
/// preferences folder.
pub(crate) fn kept() -> Option<PathBuf> {
    s2_platform::preferences_dir(settings::APP).map(|folder| folder.join(FOLDER))
}

/// One set of files to keep: the folder they are in and their paths inside it, with sizes.
pub(crate) type Kept<'a> = (&'a Path, &'a [(PathBuf, u64)]);

/// Copies the files of `game` into `into`, made with its notes first, and those of `patch`
/// into its `PATCH` folder, telling `copied` how many bytes have gone over as it goes. The
/// sources are only read.
///
/// # Errors
///
/// The first file or folder that cannot be read or written.
pub(crate) fn keep(
    game: Kept<'_>,
    patch: Option<Kept<'_>>,
    into: &Path,
    copied: &mut dyn FnMut(u64),
) -> std::io::Result<()> {
    make(into)?;
    let mut done = 0;
    copy(game, into, &mut done, copied)?;
    if let Some(patch) = patch {
        copy(patch, &into.join(PATCH), &mut done, copied)?;
    }
    Ok(())
}

/// Copies `files` of `source` into `into`, adding each stretch copied to `done` and telling
/// `copied` the sum.
fn copy(
    (source, files): Kept<'_>,
    into: &Path,
    done: &mut u64,
    copied: &mut dyn FnMut(u64),
) -> std::io::Result<()> {
    use std::io::{Read, Write};

    let mut chunk = vec![0; CHUNK];
    for (file, _) in files {
        let to = into.join(file);
        if let Some(folder) = to.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut from = std::fs::File::open(source.join(file))?;
        let mut to = std::fs::File::create(to)?;
        loop {
            let read = from.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            to.write_all(&chunk[..read])?;
            *done += u64::try_from(read).unwrap_or_default();
            copied(*done);
        }
    }
    Ok(())
}

/// Makes `folder` and its `PATCH` folder, each with its note.
fn make(folder: &Path) -> std::io::Result<()> {
    let patch = folder.join(PATCH);
    std::fs::create_dir_all(&patch)?;
    std::fs::write(folder.join(NOTE.0), NOTE.1)?;
    std::fs::write(patch.join(PATCH_NOTE.0), PATCH_NOTE.1)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("robigo-retail-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_folder_with_only_its_notes_holds_no_game() {
        let folder = scratch("notes");
        make(&folder).unwrap();
        assert_eq!(data_in(&folder), None);
        assert_eq!(patch_in(&folder), None);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_installed_game_is_kept_whole_and_a_cd_image_by_its_own_files() {
        let source = scratch("source");
        std::fs::create_dir_all(source.join("DATA")).unwrap();
        std::fs::write(source.join("s2.exe"), b"game").unwrap();
        std::fs::write(source.join("DATA").join("MAP.WLD"), b"a map").unwrap();
        let installed = files(&source);
        assert_eq!(
            installed,
            [
                (PathBuf::from("DATA").join("MAP.WLD"), 5),
                (PathBuf::from("s2.exe"), 4)
            ]
        );
        let into = scratch("kept");
        let mut reports = Vec::new();
        let patch = scratch("patch-files");
        std::fs::create_dir_all(patch.join("DATA")).unwrap();
        std::fs::write(patch.join("DATA").join("NEW.LST"), b"newer").unwrap();
        let patches = every_file(&patch);
        keep(
            (&source, &installed),
            Some((&patch, &patches)),
            &into,
            &mut |done| reports.push(done),
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&patch);
        assert_eq!(
            patch_in(&into),
            Some(into.join(PATCH)),
            "the patch is layered from there"
        );
        assert_eq!(
            std::fs::read(into.join("DATA").join("MAP.WLD")).unwrap(),
            b"a map"
        );
        assert!(into.join(NOTE.0).is_file(), "the folder's note is there too");
        assert_eq!(reports, [5, 9, 14], "the patch's bytes count on from the game's");

        std::fs::remove_file(source.join("s2.exe")).unwrap();
        std::fs::write(source.join("disc.cue"), b"sheet").unwrap();
        std::fs::write(source.join("disc.BIN"), b"image").unwrap();
        std::fs::write(source.join("notes.txt"), b"other").unwrap();
        let image: Vec<PathBuf> = files(&source).into_iter().map(|(path, _)| path).collect();
        assert_eq!(image, [PathBuf::from("disc.BIN"), PathBuf::from("disc.cue")]);
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&into);
    }

    #[test]
    fn a_patch_folder_with_files_in_it_is_layered() {
        let folder = scratch("patch");
        make(&folder).unwrap();
        std::fs::write(folder.join(PATCH).join("S2.EXE"), b"").unwrap();
        assert_eq!(patch_in(&folder), Some(folder.join(PATCH)));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
