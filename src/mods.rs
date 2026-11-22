//! The base mod baked from the player's data, and the mods the game loads over it
//! (`docs/design/mods.md`).

use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};
use s2_data::{DataSet, Layer};
use s2_mods::{Bank, GENERALUSER_GS, GOLD, ModStack, Offered, Progress, SOUND_FONT};

use crate::settings;

/// The folder the mods live in, beside the settings file.
const FOLDER: &str = "mods";

/// The folder the sound fonts Robigo fetches are kept in, beside the mods.
const SOUND_FONTS: &str = "soundfonts";

/// What the `sound_font` setting starts with when it names a mod's sound font by the mod's id.
const MOD_FONT: &str = "mod:";

/// How the bake's bar looks in a terminal.
const BAR: &str = "{msg} [{bar:40}] {pos}/{len} files";

/// The mods folder in the player's preferences folder.
pub(crate) fn root() -> Option<PathBuf> {
    s2_platform::preferences_dir(settings::APP).map(|folder| folder.join(FOLDER))
}

/// The folder fetched sound fonts are kept in, beside the mods folder.
pub(crate) fn sound_font_cache() -> Option<PathBuf> {
    root().map(|root| {
        root.parent()
            .map_or_else(|| root.join(SOUND_FONTS), |folder| folder.join(SOUND_FONTS))
    })
}

/// The `sound_font` setting that picks the sound font the mod `id` ships.
pub(crate) fn mod_font_setting(id: &str) -> String {
    format!("{MOD_FONT}{id}")
}

/// The id of the mod whose sound font `setting` picks, if it picks a mod's.
pub(crate) fn mod_font_id(setting: &str) -> Option<&str> {
    setting.strip_prefix(MOD_FONT)
}

/// The sound font `setting` names: one Robigo offers, a file of the player's, or the file the mod
/// it names ships. `None` for a mod that is not in `stack` or ships no sound font.
fn bank_of(setting: &str, stack: &ModStack) -> Option<Bank> {
    let Some(id) = mod_font_id(setting) else {
        return Some(Bank::from_setting(setting));
    };
    stack
        .holders(SOUND_FONT)
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .map(|(_, file)| Bank::File(file))
}

/// Whether the sound font `setting` names has to be fetched before it can render the songs. The
/// sound font Robigo plays with unless told otherwise ships with the game and never asks.
pub(crate) fn needs_fetching(setting: &str) -> Option<Offered> {
    if mod_font_id(setting).is_some() {
        return None;
    }
    let Bank::Offered(offered) = Bank::from_setting(setting) else {
        return None;
    };
    let cache = sound_font_cache()?;
    (offered != GENERALUSER_GS && !Bank::Offered(offered).installed(&cache)).then_some(offered)
}

/// What the start-up asks of the player and tells them while the General MIDI songs are made.
pub(crate) trait Prompt {
    /// Whether to fetch `offered`, which is not installed.
    fn fetch(&mut self, offered: &Offered) -> bool;
    /// That `got` bytes of `offered` have arrived.
    fn fetching(&mut self, offered: &Offered, got: u64);
    /// Whether to try fetching `offered` again after it failed for `reason`.
    fn failed(&mut self, offered: &Offered, reason: &str) -> bool;
    /// That the songs are being rendered.
    fn rendering(&mut self);
    /// That the bake has done `done` of its `work`, none before it has read the data, and wrote
    /// `last` last, a file's kind and path, empty before the first.
    fn baking(&mut self, done: u64, work: u64, last: &str);
    /// That there are no mods to play from, for `reason`.
    fn unprepared(&mut self, reason: &str);
}

/// The prompt with no player to ask: every fetch goes ahead and nothing is shown.
pub(crate) struct Unattended;

impl Prompt for Unattended {
    fn fetch(&mut self, _offered: &Offered) -> bool {
        true
    }

    fn fetching(&mut self, _offered: &Offered, _got: u64) {}

    fn failed(&mut self, _offered: &Offered, _reason: &str) -> bool {
        false
    }

    fn rendering(&mut self) {}

    fn baking(&mut self, _done: u64, _work: u64, _last: &str) {}

    fn unprepared(&mut self, _reason: &str) {}
}

/// Whether there is a base mod this version of Robigo baked, to play from with no game data.
pub(crate) fn usable() -> bool {
    root().is_some_and(|root| s2_mods::is_usable(&root))
}

/// Bakes `set` into `root` on a thread of its own, telling `prompt` how far it is as it goes.
fn bake_shown(set: &DataSet, root: &Path, prompt: &mut dyn Prompt) -> Result<(), s2_mods::Error> {
    let bar = Bar::new();
    std::thread::scope(|scope| {
        let baking = scope.spawn(|| s2_mods::bake(set, root, &bar));
        while !baking.is_finished() {
            prompt.baking(bar.0.position(), bar.0.length().unwrap_or_default(), &bar.last());
            std::thread::sleep(BAKE_POLL);
        }
        baking
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
    .map(|_| ())
}

/// Whether there is a base mod that another version of Robigo baked, which this one cannot play
/// from.
pub(crate) fn outdated() -> bool {
    root().is_some_and(|root| root.join(s2_mods::BASE).is_dir() && !s2_mods::is_usable(&root))
}

/// The data at `data`, with `patch` layered over it.
fn data_set(data: &Path, patch: Option<&Path>) -> Result<DataSet, s2_data::Error> {
    let mut set = DataSet::discover(data)?;
    if let Some(patch) = patch {
        set.push_layer(Layer::folder(patch)?);
    }
    Ok(set)
}

/// A file every copy of the game holds and the v1.51 patch leaves out.
const WHOLE: &str = "DATA/RESOURCE.DAT";

/// Whether the data at `data` is a whole copy of the game. A folder the v1.51 patch alone was
/// unpacked into holds `S2.EXE` as well, and is none.
pub(crate) fn whole(data: &Path) -> bool {
    data_set(data, None).is_ok_and(|set| set.exists(WHOLE))
}

/// Whether the data at `data` with `patch` over it is older than v1.51, which the bake does not
/// take. False for data that cannot be read, which the bake reports itself.
pub(crate) fn unpatched(data: &Path, patch: Option<&Path>) -> bool {
    data_set(data, patch)
        .ok()
        .and_then(|set| set.edition().ok())
        .is_some_and(|edition| edition.release() == s2_data::Release::Original)
}

/// The mods the game plays from: the base mod, baked from `data` first when it is missing or
/// was baked from other data or by another baker, then the Gold Edition's extras when the data
/// has them, then the mods `order` names, leaving out each mod `off` names, reading its texts
/// in `language`. The General MIDI
/// songs are rendered as well, through the sound font `sound_font` names, when they are missing
/// or were rendered through another, so either rendering is there to switch to. `None`, with a
/// warning, when the mods fail, which leaves the game reading the original files; General MIDI
/// songs that cannot be rendered leave that rendering silent. A sound font that has to be
/// fetched first is put to `prompt`, and a refusal sets `sound_font` to the one Robigo ships, as
/// does a setting that names the sound font of a mod no longer loaded.
/// Without `data` the base mod baked last is used as it is, if this version of Robigo baked it.
pub(crate) fn prepare(
    data: Option<&Path>,
    patch: Option<&Path>,
    order: &[String],
    off: &[String],
    language: &str,
    sound_font: &mut String,
    prompt: &mut dyn Prompt,
) -> Option<ModStack> {
    let mut opened = || -> Result<ModStack, Box<dyn std::error::Error>> {
        let root = root().ok_or("no preferences folder for the mods")?;
        let set = match data {
            Some(data) => data_set(data, patch)?,
            None if s2_mods::is_usable(&root) => DataSet::new([]),
            None if outdated() => {
                return Err(
                    "the base mod is from another version of Robigo and there is no game data to bake it again".into(),
                );
            }
            None => return Err("no game data to bake the base mod from".into()),
        };
        if data.is_some() && !s2_mods::is_current(&set, &root) {
            log::info!("baking the base mod into {}", root.display());
            bake_shown(&set, &root, prompt)?;
        }
        let stack = ModStack::open(&root, &load_order(&root, order, off))?.in_language(language);
        if let Some(offered) = needs_fetching(sound_font)
            && !s2_mods::midi_current(&root, offered.digest)
            && !fetched(&offered, prompt)
        {
            log::info!(
                "{} was not fetched; playing with {}",
                offered.name,
                GENERALUSER_GS.name
            );
            GENERALUSER_GS.name.clone_into(sound_font);
        }
        let bank = bank_of(sound_font, &stack).unwrap_or_else(|| {
            log::info!(
                "the mod of sound font {sound_font} is not loaded; playing with {}",
                GENERALUSER_GS.name
            );
            GENERALUSER_GS.name.clone_into(sound_font);
            Bank::Offered(GENERALUSER_GS)
        });
        if let Err(error) = render_midi(&root, &bank, prompt) {
            log::warn!("no General MIDI songs: {error}");
        }
        Ok(stack)
    };
    opened()
        .inspect_err(|error| {
            log::warn!("reading the original files, no mods: {error}");
            prompt.unprepared(&error.to_string());
        })
        .ok()
}

/// The mods to load over the base mod in `root`: the Gold Edition's extras when they are baked,
/// then those `order` names, leaving out each `off` names.
fn load_order(root: &Path, order: &[String], off: &[String]) -> Vec<String> {
    let extras = root.join(GOLD).is_dir().then(|| GOLD.to_owned());
    extras
        .into_iter()
        .chain(order.iter().filter(|id| *id != GOLD).cloned())
        .filter(|id| !off.contains(id))
        .collect()
}

/// Whether `offered` is fetched, once `prompt` agrees to it, trying again for as long as
/// `prompt` asks after a failure.
fn fetched(offered: &Offered, prompt: &mut dyn Prompt) -> bool {
    let Some(cache) = sound_font_cache() else {
        return false;
    };
    if !prompt.fetch(offered) {
        return false;
    }
    loop {
        match Bank::Offered(*offered).fetch(&cache, &mut |got| prompt.fetching(offered, got)) {
            Ok(_) => return true,
            Err(error) => {
                log::warn!("{error}");
                let reason = match error {
                    s2_mods::Error::SoundFont { reason, .. } => reason,
                    other => other.to_string(),
                };
                if !prompt.failed(offered, &reason) {
                    return false;
                }
            }
        }
    }
}

/// Renders the General MIDI songs into the base mod in `root` through `bank`, unless they
/// already were, fetching an offered sound font first if need be.
fn render_midi(root: &Path, bank: &Bank, prompt: &mut dyn Prompt) -> Result<(), Box<dyn std::error::Error>> {
    let identity = bank
        .identity()
        .ok_or("the sound font is not one Robigo offers or a file that is there")?;
    if s2_mods::midi_current(root, &identity) {
        return Ok(());
    }
    let cache = sound_font_cache().ok_or("no preferences folder for the sound fonts")?;
    let font = match bank {
        Bank::Offered(offered) => bank.fetch(&cache, &mut |got| prompt.fetching(offered, got))?,
        Bank::File(_) => bank.fetch(&cache, &mut |_| {})?,
    };
    prompt.rendering();
    log::info!("rendering the General MIDI songs through {}", font.display());
    s2_mods::render_midi(root, &font, &identity)?;
    Ok(())
}

/// Why a game recorded with the mods `recorded` cannot run with `loaded`, both ids in load
/// order, or `None` when they are the same. Versions are not compared.
pub(crate) fn differ(recorded: &[String], loaded: &[String]) -> Option<String> {
    let missing: Vec<&String> = recorded.iter().filter(|id| !loaded.contains(id)).collect();
    if !missing.is_empty() {
        return Some(format!("the mods it needs are not enabled: {}", names(missing)));
    }
    let extra: Vec<&String> = loaded.iter().filter(|id| !recorded.contains(id)).collect();
    if !extra.is_empty() {
        return Some(format!("it was made without: {}", names(extra)));
    }
    (recorded != loaded).then(|| format!("its mods were loaded in another order: {}", names(recorded)))
}

/// The mods `ids` by the names their manifests give. One not installed goes by its id, except
/// the baked mods, which always have their names.
fn names<'a>(ids: impl IntoIterator<Item = &'a String>) -> String {
    let root = root();
    ids.into_iter()
        .map(|id| match id.as_str() {
            s2_mods::BASE => s2_mods::BASE_NAME.to_owned(),
            GOLD => s2_mods::GOLD_NAME.to_owned(),
            _ => root
                .as_ref()
                .and_then(|root| s2_mods::Manifest::read(&root.join(id)).ok())
                .map_or_else(|| id.clone(), |manifest| manifest.name),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `robigo bake`'s arguments.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Game data: an install folder, the GOG folder, a .cue/.ins sheet or an .iso.
    data: PathBuf,
    /// A folder layered over the data, such as the v1.51 patch over a CD image.
    #[arg(long)]
    patch: Option<PathBuf>,
}

/// Runs `robigo bake`: bakes the base mod from the data again, whether or not it is current, and
/// renders the General MIDI songs through the sound font the settings name, fetching it first if
/// need be, so the next start has nothing left to make.
///
/// # Errors
///
/// When the data cannot be read or the base mod cannot be written.
pub fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let root = root().ok_or("no preferences folder for the mods")?;
    let set = data_set(&args.data, args.patch.as_deref())?;
    let record = s2_mods::bake(&set, &root, &Bar::new())?;
    log::info!(
        "baked the base mod from {} into {}",
        record.edition,
        root.join(s2_mods::BASE).display()
    );
    let settings = settings::Settings::file()
        .map(|path| settings::Settings::read(&path))
        .unwrap_or_default();
    let stack = ModStack::open(&root, &load_order(&root, &settings.mods, &settings.switched_off))?;
    let bank = bank_of(&settings.sound_font, &stack).unwrap_or(Bank::Offered(GENERALUSER_GS));
    if let Err(error) = render_midi(&root, &bank, &mut Unattended) {
        log::warn!("no General MIDI songs: {error}");
    }
    Ok(())
}

/// How long the start waits between looks at how far a bake is.
const BAKE_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// A terminal progress bar for a bake, which also keeps the file written last.
struct Bar(ProgressBar, std::sync::Mutex<String>);

impl Bar {
    fn new() -> Self {
        let bar = ProgressBar::hidden();
        if let Ok(style) = ProgressStyle::with_template(BAR) {
            bar.set_style(style.progress_chars("=> "));
        }
        bar.set_message("baking");
        Self(bar, std::sync::Mutex::default())
    }

    /// The file the bake wrote last, as a player would read it: its kind, then its path.
    fn last(&self) -> String {
        let path = self.1.lock().map(|path| path.clone()).unwrap_or_default();
        if path.is_empty() {
            return path;
        }
        let (kind, file) = s2_mods::step(&path);
        format!("{kind}: {file}")
    }
}

impl Progress for Bar {
    fn begin(&self, work: u64) {
        self.0.set_length(work);
        self.0.set_draw_target(indicatif::ProgressDrawTarget::stderr());
    }

    fn wrote(&self, path: &str, work: u64) {
        self.0.inc(work);
        if let Ok(mut last) = self.1.lock() {
            path.clone_into(&mut last);
        }
    }

    fn end(&self) {
        self.0.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|&id| id.to_owned()).collect()
    }

    #[test]
    fn the_patch_alone_is_no_whole_copy() {
        let folder = |name: &str| std::env::var_os(name).map(std::path::PathBuf::from);
        let (Some(patch), Some(game)) = (folder("S2_DATA_PATCH"), folder("S2_DATA_CUE")) else {
            return;
        };
        assert!(!whole(&patch));
        assert!(whole(&game));
        assert!(unpatched(&game, None));
        assert!(!unpatched(&game, Some(&patch)));
    }

    #[test]
    fn the_same_mods_in_the_same_order_match() {
        let mods = ids(&["test-base", "test-extra"]);
        assert_eq!(differ(&mods, &mods), None);
        assert_eq!(differ(&[], &[]), None);
    }

    #[test]
    fn a_missing_mod_is_named_first() {
        let reason = differ(
            &ids(&["test-base", "test-extra"]),
            &ids(&["test-base", "test-other"]),
        );
        assert_eq!(
            reason.as_deref(),
            Some("the mods it needs are not enabled: test-extra")
        );
    }

    #[test]
    fn an_extra_or_reordered_mod_is_refused() {
        let extra = differ(&ids(&["test-base"]), &ids(&["test-base", "test-other"]));
        assert_eq!(extra.as_deref(), Some("it was made without: test-other"));
        let order = differ(&ids(&["test-a", "test-b"]), &ids(&["test-b", "test-a"]));
        assert_eq!(
            order.as_deref(),
            Some("its mods were loaded in another order: test-a, test-b")
        );
    }
}
