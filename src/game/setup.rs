//! The set-up a start with no game data runs before anything else: the player picks the folder
//! their Settlers 2 is in, the game keeps a copy of it in its own `RETAIL` folder to bake from
//! again, and the start carries on from there. The picked folder is only read.

use std::path::{Path, PathBuf};

use s2_platform::{Tone, Window};

use super::window::TITLE;
use crate::{mods, retail};

/// Bytes in the megabyte a copy's size is told in.
const MEGABYTE: u64 = 1_000_000;

/// The game data to start from and the patch over it, once the player has picked where their
/// Settlers 2 is. `outdated` is a start that found a base mod another version of Robigo baked.
/// `show` tells the player how far the copy is, as a line of text and a per cent. `None` when the player
/// quits, or when the system has no folder dialog, which leaves them told to copy the game
/// themselves.
pub(crate) fn find(
    window: &Window,
    outdated: bool,
    show: &mut dyn FnMut(&str, u64),
) -> Option<(PathBuf, Option<PathBuf>)> {
    let why = if outdated {
        "Due to an update, Robigo needs to rebuild its game data."
    } else {
        "Robigo requires a copy of Settlers 2 the first time it runs."
    };

    let mut message = format!(
        "{why}\n\n\
        The data can be from v1.51 or the Gold Edition,\n\
        or from v1.03 with the v1.51 patch\n\n\
        Choose a folder containing: \n\
        - An installed copy of the DOS version,\n\
        - An installed copy of the GOG version,\n\
        - A CD image (ISO/CUE+BIN)"
    );
    loop {
        if !ask(window, Tone::Information, &message, "Browse", "Quit") {
            return None;
        }
        let folder = match window.pick_folder() {
            Ok(Some(folder)) => folder,
            Ok(None) => continue,
            Err(error) => {
                log::warn!("no folder dialog: {error}");
                tell(window, &by_hand(why, retail::set_up().as_deref()));
                return None;
            }
        };
        let Some((data, patch)) = retail::held(&folder) else {
            message = format!(
                "No copy of Settlers 2 was found in\n\n    {}\n\n\
                 Choose the folder the game is installed in, or one that holds its CD image.",
                folder.display()
            );
            continue;
        };
        if !mods::whole(&data) {
            message = format!(
                "Only the v1.51 patch was found in\n\n    {}\n\n\
                 Choose the folder the game itself is in first. Robigo asks for the patch\n\
                 afterwards if the game needs it.",
                folder.display()
            );
            continue;
        }
        let patch = match patch {
            None if mods::unpatched(&data, None) => Some(patch_for(window, &data)?),
            patch => patch,
        };
        let kept = kept(window, &folder, patch.as_deref(), show);
        return Some(kept.unwrap_or((data, patch)));
    }
}

/// The folder the v1.51 patch's files are in, which the player picks for the copy at `data`,
/// older than v1.51. A folder whose files do not bring the copy to v1.51 is asked for again.
/// `None` when the player quits, or when the system has no folder dialog.
fn patch_for(window: &Window, data: &Path) -> Option<PathBuf> {
    let mut message = "Your copy of Settlers 2 is older than v1.51, so Robigo needs the official\n\
         v1.51 patch as well.\n\n\
         Choose the folder the patch's files are in. The patch has to be unpacked first:\n\
         it is usually a ZIP from Blue Byte's website. The self-extracting version\n\
         opens in most archive software too."
        .to_owned();
    loop {
        if !ask(window, Tone::Information, &message, "Browse", "Quit") {
            return None;
        }
        let folder = match window.pick_folder() {
            Ok(Some(folder)) => folder,
            Ok(None) => continue,
            Err(error) => {
                log::warn!("no folder dialog: {error}");
                return None;
            }
        };
        if !mods::unpatched(data, Some(&folder)) {
            return Some(folder);
        }
        message = format!(
            "The v1.51 patch was not found in\n\n    {}\n\n\
             Choose the folder the patch was unpacked into, the one that holds its S2.EXE.",
            folder.display()
        );
    }
}

/// The data and patch in the game's own copy of the game in `folder` and of the patch's files
/// in `patch`, made now if the player agrees to it. `None` when they do not, when there is
/// nowhere to keep one or when the copy fails, which they are told.
fn kept(
    window: &Window,
    folder: &Path,
    patch: Option<&Path>,
    show: &mut dyn FnMut(&str, u64),
) -> Option<(PathBuf, Option<PathBuf>)> {
    let into = retail::kept()?;
    if into == folder {
        return None;
    }
    let mut files = retail::files(folder);
    if let Some(patch) = patch {
        files.retain(|(file, _)| !folder.join(file).starts_with(patch));
    }
    let patches = patch.map(|patch| (patch, retail::every_file(patch)));
    let size: u64 = files
        .iter()
        .chain(patches.iter().flat_map(|(_, files)| files))
        .map(|(_, size)| size)
        .sum();
    let question = format!(
        "Keep a copy of the game for Robigo? It takes {} MB and will be stored in\n\n{}\n\n\
         Robigo will use the copied data again after an update.\n\
         Without one, you'll need to provide it next time.",
        size.div_ceil(MEGABYTE),
        into.display()
    );
    if !ask(window, Tone::Information, &question, "Keep a copy", "Not now") {
        return None;
    }
    let patches = patches.as_ref().map(|(patch, files)| (*patch, files.as_slice()));
    let copied = retail::keep((folder, &files), patches, &into, &mut |done| {
        show("Copying Settlers 2 data", done * 100 / size.max(1));
    });
    match copied {
        Ok(()) => retail::held(&into),
        Err(error) => {
            log::warn!("no copy of the game in {}: {error}", into.display());
            tell(
                window,
                &format!(
                    "The game could not be copied.\n\n{error}\n\nRobigo carries on from the folder you chose."
                ),
            );
            None
        }
    }
}

/// What a start tells the player when it cannot ask for the folder: to copy the game into
/// `folder`, the `RETAIL` folder, themselves.
fn by_hand(why: &str, folder: Option<&Path>) -> String {
    let into = folder.map_or_else(
        || "the RETAIL folder beside Robigo".to_owned(),
        |folder| folder.display().to_string(),
    );
    format!(
        "{why} Copy it into\n\n    {into}\n\n\
         as the note in that folder says, then start Robigo again. It prepares what it needs\n\
         from there and never changes the files."
    )
}

/// The player's answer to `message`, `no` when the dialog cannot be shown.
fn ask(window: &Window, tone: Tone, message: &str, yes: &str, no: &str) -> bool {
    window.ask(tone, TITLE, message, yes, no).unwrap_or_else(|error| {
        log::warn!("no dialog to ask: {error}");
        false
    })
}

/// Tells the player `message` as a warning.
pub(crate) fn tell(window: &Window, message: &str) {
    if let Err(error) = window.tell(Tone::Warning, TITLE, message) {
        log::warn!("no dialog to say {message:?}: {error}");
    }
}
