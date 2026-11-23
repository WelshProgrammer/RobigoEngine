//! The player's settings: the size the window opens at, whether it is fullscreen, the view's
//! zoom, the interface's scale, how often ambient sounds get their chance, who draws the mouse
//! pointer, whether the intro plays and whether new games fix S2's engine bugs, kept between runs as `settings.json` in the user's
//! preferences folder. A flag given on the command line overrides a setting for that run and is
//! never written back. What the player changes in the window is, when the window closes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use s2_render::{Zoom, overlay};

/// The folder name the preferences go under.
pub(crate) const APP: &str = "robigo";

/// The file in that folder.
const FILE: &str = "settings.json";

/// What the player has chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a setting of its own the player flips"
)]
pub(crate) struct Settings {
    /// The window's size in window coordinates.
    pub(crate) size: (u32, u32),
    /// Whether the window opens fullscreen.
    pub(crate) fullscreen: bool,
    /// The view's zoom in quarters, or none for the largest whole zoom that fills the window.
    pub(crate) zoom: Option<u8>,
    /// How many output pixels one interface pixel covers.
    pub(crate) ui_scale: u8,
    /// How many main-loop passes a second the ambient sounds get, each a chance to play. `S2.EXE`
    /// takes one per pass, as fast as the machine runs.
    pub(crate) ambient: u16,
    /// Whether the game draws the mouse pointer into its own frame, as S2 does, instead of
    /// handing the picture to the system, which shows it without waiting for a frame.
    pub(crate) software_pointer: bool,
    /// Whether dragging the map moves the view the way the pointer goes, as S2 does, instead of
    /// pulling the map along with the pointer.
    pub(crate) drag_moves_view: bool,
    /// Whether the game starts on the title picture without playing the intro first.
    pub(crate) skip_intro: bool,
    /// Whether a picture shows for a few seconds while a game starts, as S2 shows one while it
    /// loads the map.
    pub(crate) loading_pictures: bool,
    /// Which automatic save period the save screen set, 0 being off.
    pub(crate) autosave: usize,
    /// Whether a new game fixes the engine bugs copied from S2. Off plays them as S2 does,
    /// for perfect emulation.
    pub(crate) bug_fixes: bool,
    /// The mods to load over the base mod, in order, each after the mods it depends on.
    pub(crate) mods: Vec<String>,
    /// Mods the player has switched off, by id, which stay installed. `gold` leaves out the Gold
    /// Edition's extras: the World Campaign, the winter landscape, the map editor and the new maps.
    pub(crate) switched_off: Vec<String>,
    /// The language the texts are read in, the name of a folder under a mod's `texts/`: `en`,
    /// or `de` with a copy of the game that has the German texts. A text the language lacks
    /// is read in English.
    pub(crate) language: String,
    /// Which rendering of the songs plays: `opl3`, as S2 plays them on an FM card, or
    /// `general-midi`, through the sound font.
    pub(crate) music: String,
    /// The sound font the General MIDI songs are rendered with: `generaluser-gs`,
    /// `musescore-general`, `mod:` and the id of a loaded mod that ships one, or the path of an
    /// SF2 file.
    pub(crate) sound_font: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            size: DEFAULT_WINDOW,
            fullscreen: false,
            zoom: None,
            ui_scale: overlay::DEFAULT_SCALE,
            ambient: AMBIENT,
            software_pointer: false,
            drag_moves_view: false,
            skip_intro: false,
            loading_pictures: true,
            autosave: 0,
            bug_fixes: true,
            mods: Vec::new(),
            switched_off: Vec::new(),
            language: s2_mods::ENGLISH.into(),
            music: OPL3.into(),
            sound_font: s2_mods::GENERALUSER_GS.name.into(),
        }
    }
}

/// The `music` setting for the OPL3 rendering.
const OPL3: &str = "opl3";

/// The `music` setting for the General MIDI rendering.
const GENERAL_MIDI: &str = "general-midi";

/// Ambient passes a second when nothing chooses: the pace of a Pentium of the time, at about
/// 880,000 emulated cycles a pass. A 486 managed about 30, and a modern machine running the
/// Gold Edition's own emulator settings about 700.
pub(crate) const AMBIENT: u16 = 70;

/// The most ambient passes a second the setting takes.
pub(crate) const MOST_AMBIENT: u16 = 1000;

/// The window's size when nothing chooses one: the original's 640x480, doubled.
pub(crate) const DEFAULT_WINDOW: (u32, u32) = (1280, 960);

/// The smallest window the game opens or lets the player resize to.
pub(crate) const SMALLEST_WINDOW: (u32, u32) = (320, 400);

/// `size` made at least [`SMALLEST_WINDOW`] each way.
#[must_use]
pub(crate) fn held_size(size: (u32, u32)) -> (u32, u32) {
    (size.0.max(SMALLEST_WINDOW.0), size.1.max(SMALLEST_WINDOW.1))
}

impl Settings {
    /// The rendering of the songs the `music` setting names; anything but `general-midi` is the
    /// OPL3's.
    #[must_use]
    pub(crate) fn rendition(&self) -> s2_mods::Rendition {
        if self.music.eq_ignore_ascii_case(GENERAL_MIDI) {
            s2_mods::Rendition::GeneralMidi
        } else {
            s2_mods::Rendition::Opl3
        }
    }

    /// Names `rendition` in the `music` setting.
    pub(crate) fn set_rendition(&mut self, rendition: s2_mods::Rendition) {
        self.music = match rendition {
            s2_mods::Rendition::Opl3 => OPL3,
            s2_mods::Rendition::GeneralMidi => GENERAL_MIDI,
        }
        .into();
    }

    /// Where the settings live for this user, or `None` when the platform names no folder.
    #[must_use]
    pub(crate) fn file() -> Option<PathBuf> {
        s2_platform::preferences_dir(APP).map(|folder| folder.join(FILE))
    }

    /// The settings in `path`. A missing file gives the defaults; one that cannot be read or
    /// parsed gives them too, with a warning. Values out of range are held to the nearest one
    /// the game accepts.
    #[must_use]
    pub(crate) fn read(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                log::warn!("{}: {error}; using the default settings", path.display());
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(settings) => settings.held(),
            Err(error) => {
                log::warn!("{}: {error}; using the default settings", path.display());
                Self::default()
            }
        }
    }

    /// Writes the settings to `path`, creating its folder if needed. A failure is logged and
    /// otherwise ignored, since losing a setting is no reason to stop the game.
    pub(crate) fn write(&self, path: &Path) {
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
                std::fs::write(path, text + "\n")
            });
        match written {
            Ok(()) => log::info!("saved the settings to {}", path.display()),
            Err(error) => log::warn!("{}: {error}; the settings were not saved", path.display()),
        }
    }

    /// The zoom, where the stored quarters name one.
    #[must_use]
    pub(crate) fn zoom(&self) -> Option<Zoom> {
        self.zoom.and_then(Zoom::from_quarters)
    }

    /// The same settings with every value inside the range the game accepts.
    fn held(self) -> Self {
        Self {
            size: held_size(self.size),
            zoom: self
                .zoom
                .filter(|&quarters| Zoom::from_quarters(quarters).is_some()),
            ui_scale: self.ui_scale.clamp(1, overlay::LARGEST_SCALE),
            ambient: self.ambient.clamp(1, MOST_AMBIENT),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("robigo-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join(FILE)
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        assert_eq!(Settings::read(&scratch("missing")), Settings::default());
    }

    #[test]
    fn what_is_written_reads_back() {
        let path = scratch("round");
        let chosen = Settings {
            size: (1600, 900),
            fullscreen: true,
            zoom: Some(8),
            ui_scale: 3,
            ambient: 30,
            software_pointer: true,
            drag_moves_view: true,
            skip_intro: true,
            loading_pictures: false,
            autosave: 2,
            bug_fixes: false,
            mods: vec!["bigger-storehouses".into()],
            switched_off: vec!["gold".into()],
            language: "de".into(),
            music: GENERAL_MIDI.into(),
            sound_font: "/fonts/sc55.sf2".into(),
        };
        chosen.write(&path);
        assert_eq!(Settings::read(&path), chosen);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_with_some_values_keeps_the_defaults_for_the_rest() {
        let path = scratch("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{ "ui_scale": 4 }"#).unwrap();
        let read = Settings::read(&path);
        assert_eq!(read.ui_scale, 4);
        assert_eq!(read.size, DEFAULT_WINDOW);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn values_out_of_range_are_held_to_ones_the_game_takes() {
        let held = Settings {
            size: (0, 0),
            fullscreen: false,
            zoom: Some(0),
            ui_scale: 99,
            ambient: 0,
            software_pointer: false,
            drag_moves_view: false,
            mods: Vec::new(),
            switched_off: Vec::new(),
            ..Settings::default()
        }
        .held();
        assert_eq!(held.size, SMALLEST_WINDOW);
        assert_eq!(held.zoom, None);
        assert_eq!(held.ui_scale, overlay::LARGEST_SCALE);
        assert_eq!(held.ambient, 1);
    }

    #[test]
    fn a_broken_file_gives_the_defaults() {
        let path = scratch("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::read(&path), Settings::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
