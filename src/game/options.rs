//! The choices the options window steps through: window sizes, ambient paces and sound fonts.

use s2_mods::{Bank, ModStack, OFFERED, Offered, SOUND_FONT};
use s2_ui::text::Label;

use crate::mods::{mod_font_id, mod_font_setting};
use crate::settings::{AMBIENT, MOST_AMBIENT};

/// The window sizes the options step through, smallest first: S2's 640x480 and its multiples,
/// the 4:3 sizes of the time, and the common 16:9 screens.
pub(crate) const SIZES: [(u32, u32); 10] = [
    (640, 480),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 960),
    (1600, 900),
    (1600, 1200),
    (1920, 1080),
    (1920, 1440),
    (2560, 1440),
];

/// The ambient paces the options step through: a 486's, the default Pentium's, and the Gold
/// Edition's own emulator settings on a modern machine.
const AMBIENT_PACES: [u16; 6] = [30, AMBIENT, 150, 300, 700, MOST_AMBIENT];

/// The ambient pace a step of `step` from `passes` reaches, held at either end.
#[must_use]
pub(crate) fn stepped_ambient(passes: u16, step: i8) -> u16 {
    let reached = if step > 0 {
        AMBIENT_PACES.into_iter().find(|&pace| pace > passes)
    } else {
        AMBIENT_PACES.into_iter().rev().find(|&pace| pace < passes)
    };
    reached.unwrap_or(passes)
}

/// The sound fonts the options offer: those Robigo offers, then those the loaded mods ship, then
/// the player's own file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fonts {
    /// The `sound_font` setting that picks each.
    settings: &'static [String],
    /// The name the options show for each: a mod's sound font goes by the mod's name.
    pub(crate) titles: &'static [&'static str],
}

impl Fonts {
    /// The sound fonts for a game with the mods of `stack` whose settings named `setting` at
    /// the start. The list is kept for as long as the game runs, since a label holds its text
    /// that long; a start makes it once.
    #[must_use]
    pub(crate) fn offered(stack: Option<&ModStack>, setting: &str) -> Self {
        let holders = stack.map(|stack| stack.holders(SOUND_FONT)).unwrap_or_default();
        let mods: Vec<(&str, &str)> = holders
            .iter()
            .map(|(manifest, _)| (manifest.id.as_str(), manifest.name.as_str()))
            .collect();
        Self::listed(&mods, setting)
    }

    /// The same for mods given by id and name.
    fn listed(mods: &[(&str, &str)], setting: &str) -> Self {
        let mut settings: Vec<String> = OFFERED.iter().map(|offered| offered.name.to_owned()).collect();
        let mut titles: Vec<&'static str> = OFFERED.iter().map(|offered| offered.title).collect();
        for &(id, name) in mods {
            settings.push(mod_font_setting(id));
            titles.push(Box::leak(shortened(name).into_boxed_str()));
        }
        if let Some(own) = own_sound_font(setting) {
            settings.push(own.to_owned());
            titles.push("Your own");
        }
        Self {
            settings: Vec::leak(settings),
            titles: Vec::leak(titles),
        }
    }

    /// Where the sound font `setting` names sits in the list.
    #[must_use]
    pub(crate) fn place(self, setting: &str) -> Option<usize> {
        self.settings
            .iter()
            .position(|each| each.eq_ignore_ascii_case(setting))
    }

    /// The setting for the sound font at `place`.
    #[must_use]
    pub(crate) fn setting(self, place: usize) -> Option<&'static str> {
        self.settings.get(place).map(String::as_str)
    }
}

/// How many characters of a mod's name the sound font control has room for.
const TITLE_CHARACTERS: usize = 18;

/// What stands for the rest of a name cut short.
const ELLIPSIS: &str = "...";

/// `name` as the sound font control shows it: whole when it fits, and otherwise cut short with
/// an ellipsis.
fn shortened(name: &str) -> String {
    if name.chars().count() <= TITLE_CHARACTERS {
        return name.to_owned();
    }
    let kept: String = name.chars().take(TITLE_CHARACTERS - ELLIPSIS.len()).collect();
    format!("{}{ELLIPSIS}", kept.trim_end())
}

/// The player's own sound font the settings name, if they name a file.
#[must_use]
fn own_sound_font(setting: &str) -> Option<&str> {
    (mod_font_id(setting).is_none() && matches!(Bank::from_setting(setting), Bank::File(_)))
        .then_some(setting)
}

/// What the notice says when the player picks `offered` and it is not installed.
#[must_use]
pub(crate) fn not_installed(offered: &Offered) -> Vec<Label> {
    vec![
        Label::Text(offered.title),
        Label::Text("is not installed. The next start"),
        Label::Text("asks to download it:"),
        Label::Megabytes(i32::try_from(offered.megabytes()).unwrap_or(i32::MAX)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_paces_step_and_hold() {
        assert_eq!(stepped_ambient(AMBIENT, 1), 150);
        assert_eq!(stepped_ambient(AMBIENT, -1), 30);
        assert_eq!(stepped_ambient(30, -1), 30);
        assert_eq!(stepped_ambient(50, 1), AMBIENT);
        assert_eq!(stepped_ambient(MOST_AMBIENT, 1), MOST_AMBIENT);
    }

    #[test]
    fn the_offered_sound_fonts_come_first_and_the_players_own_last() {
        let fonts = Fonts::listed(&[], "/sc55.sf2");
        assert_eq!(fonts.titles, ["GeneralUser GS", "MuseScore General", "Your own"]);
        assert_eq!(fonts.place("GeneralUser-GS"), Some(0), "whatever its case");
        assert_eq!(fonts.setting(2), Some("/sc55.sf2"));
        assert_eq!(fonts.setting(3), None);
        assert_eq!(Fonts::listed(&[], "musescore-general").titles.len(), 2);
    }

    #[test]
    fn a_mods_sound_font_goes_by_the_mods_name_and_is_no_file_of_the_players() {
        let mods = [("roland", "Roland sounds")];
        let fonts = Fonts::listed(&mods, "mod:roland");
        assert_eq!(
            fonts.titles,
            ["GeneralUser GS", "MuseScore General", "Roland sounds"]
        );
        assert_eq!(fonts.place("mod:roland"), Some(2));
        assert_eq!(fonts.setting(2), Some("mod:roland"));
        assert_eq!(Fonts::listed(&[], "mod:gone").place("mod:gone"), None);
    }

    #[test]
    fn a_mod_name_too_long_for_the_control_is_cut_short() {
        let mods = [("sc55", "Roland Sound Canvas SC-55 mkII")];
        let fonts = Fonts::listed(&mods, "mod:sc55");
        assert_eq!(fonts.titles[2], "Roland Sound Ca...");
        assert_eq!(shortened("MuseScore General"), "MuseScore General");
    }
}
