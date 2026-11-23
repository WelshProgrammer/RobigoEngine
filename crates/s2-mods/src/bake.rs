//! Baking the player's data into the base mod.

use std::collections::BTreeMap;
use std::path::Path;

use rayon::prelude::*;
use s2_assets::ROM_ANIM;
use s2_audio::Effects;
use s2_data::{CdAudio, DataSet, Release};
use s2_sim::SimTables;

use crate::bobs::{bake_bob, bob_files};
use crate::cd::{CD_SOURCE, cd_path, cd_table, cd_tracks, rip};
use crate::effects::{EFFECTS_TABLE, SOUND_ARCHIVE, sample_files, table_file};
use crate::gold::{GOLD, GOLD_NAME, is_gold, is_gold_item};
use crate::interface::{bake_fonts, bake_pair, pairs};
use crate::maps::{map_files, map_path};
use crate::missions::{mission_files, mission_path};
use crate::music::{Rendition, SongJob, music_files, render, score_files, song_jobs, song_path};
use crate::pictures::{REMAP, REMAP_BAKED, bake_picture, picture_files};
use crate::sim::{bake_script, table_files};
use crate::sprites::{Baked, archives, bake_archive};
use crate::tables::table_files as look_files;
use crate::terrain::{bake_terrain, landscapes, terrain_files};
use crate::texts::{bake_text, text_files};
use crate::video::{video_files, video_path};
use crate::{BASE, BASE_NAME, Error, MANIFEST, Manifest, Result};

/// The baker's version. A base mod baked by another version is baked again.
pub const BAKER: u32 = 1;

/// The file in the base mod that says what it was baked from.
const RECORD: &str = "bake.toml";

/// The original files every bake reads besides the sprite archives, named pairs, maps, BOB
/// files, terrain, texts and music, whose hashes decide when to bake again.
const SOURCES: [&str; 7] = [
    SOUND_ARCHIVE,
    ROM_ANIM,
    REMAP,
    "GFX/PALETTE/PALETTI8.BBM",
    "GFX/PALETTE/PAL5.BBM",
    "GFX/PALETTE/PAL6.BBM",
    "GFX/PALETTE/PAL7.BBM",
];

/// Where a bake is written before its mods take the old ones' places, so a bake cut short never
/// leaves a broken mod behind.
const BAKING: &str = ".baking";

/// Where the old baked mods wait while the new ones move in.
const RETIRED: &str = ".retired";

/// What the base mod says about itself.
const BASE_DESCRIPTION: &str = "The base game, baked from the player's copy of Settlers 2.";

/// What the Gold Edition's mod says about itself.
const GOLD_DESCRIPTION: &str = "The Gold Edition's extras: the World Campaign, the winter landscape, the map editor and twelve new maps.";

/// What the base mod was baked from: the baker, the edition, and a hash of each source file, of
/// each video's size and of the disc image's track table. The videos and the image are copied or
/// ripped as they are, so their sizes stand in for their tens of megabytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BakeRecord {
    /// [`BAKER`] at the time.
    pub baker: u32,
    /// The data set's edition, as `s2_data::Edition` prints it.
    pub edition: String,
    /// Each source file's 64-bit content hash, in hexadecimal like a replay header's.
    pub sources: BTreeMap<String, String>,
}

impl BakeRecord {
    /// The record a bake of `data` would write now.
    ///
    /// # Errors
    ///
    /// [`Error::Data`] when a source file cannot be read.
    pub fn of(data: &DataSet) -> Result<Self> {
        let edition = data
            .edition()
            .map_or_else(|_| "unknown".into(), |edition| edition.to_string());
        let paths: Vec<String> = SOURCES
            .iter()
            .map(|&path| path.to_owned())
            .filter(|path| data.exists(path))
            .chain(archives(data))
            .chain(
                pairs(data)
                    .into_iter()
                    .flat_map(|stem| [format!("{stem}.IDX"), format!("{stem}.DAT")]),
            )
            .chain(map_files(data))
            .chain(mission_files(data))
            .chain(bob_files(data).into_iter().map(str::to_owned))
            .chain(terrain_files(data))
            .chain(picture_files(data))
            .chain(text_files(data))
            .chain(music_files(data))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut sources: BTreeMap<String, String> = paths
            .par_iter()
            .map(|path| {
                let hash = s2_sim::replay::content_hash(&data.read(path)?);
                Ok((path.clone(), format!("{hash:016x}")))
            })
            .collect::<Result<_>>()?;
        for path in video_files(data) {
            let hash = s2_sim::replay::content_hash(&data.file_size(&path)?.to_le_bytes());
            sources.insert(path, format!("{hash:016x}"));
        }
        if let Some(audio) = data.cd_audio() {
            let hash = s2_sim::replay::content_hash(cd_table(audio).as_bytes());
            sources.insert(CD_SOURCE.into(), format!("{hash:016x}"));
        }
        Ok(Self {
            baker: BAKER,
            edition,
            sources,
        })
    }
}

/// The work a song or a CD track is counted as, in files copied or encoded: a bake spends most
/// of its time on these few, so a count by files alone would sit just short of done for most
/// of it.
const SLOW_WORK: u64 = 1000;

/// The kinds of file a bake writes, by the folder of the mod each is in, as a player would name
/// them.
const STEPS: [(&str, &str); 15] = [
    ("bobs", "Settlers"),
    ("cd", "CD audio"),
    ("fonts", "Fonts"),
    ("interface", "Interface"),
    ("maps", "Maps"),
    ("missions", "Campaign"),
    ("music", "Music"),
    ("pictures", "Pictures"),
    ("sim", "Game rules"),
    ("sounds", "Sound effects"),
    ("sprites", "Graphics"),
    ("tables", "Tables"),
    ("terrain", "Terrain"),
    ("texts", "Texts"),
    ("video", "Videos"),
];

/// What kind of file the bake wrote at `path`, as [`Progress::wrote`] gives it, and the file's
/// path within that kind's folder.
#[must_use]
pub fn step(path: &str) -> (&'static str, &str) {
    let inside = path.split_once('/').map_or(path, |(_, inside)| inside);
    let (folder, file) = inside.split_once('/').unwrap_or(("", inside));
    let kind = STEPS
        .iter()
        .find(|(of, _)| *of == folder)
        .map_or("Files", |(_, kind)| kind);
    (kind, file)
}

/// How a bake reports on its way: how much work it has, then each file as it lands with the
/// work it was. A file is one unit of work, and a song or a CD track many. The files are written
/// from several threads at once.
pub trait Progress: Sync {
    /// The bake is about to do `work` units of work.
    fn begin(&self, work: u64);
    /// It wrote the file at `path`, the mod's id first, which was `work` units.
    fn wrote(&self, path: &str, work: u64);
    /// It finished.
    fn end(&self);
}

/// Progress that reports nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Quiet;

impl Progress for Quiet {
    fn begin(&self, _work: u64) {}
    fn wrote(&self, _path: &str, _work: u64) {}
    fn end(&self) {}
}

/// Whether the base mod in `root` is a bake of `data` by this baker.
#[must_use]
pub fn is_current(data: &DataSet, root: &Path) -> bool {
    let path = root.join(BASE).join(RECORD);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(baked) = toml::from_str::<BakeRecord>(&text) else {
        return false;
    };
    BakeRecord::of(data).is_ok_and(|now| now == baked)
}

/// Whether the base mod in `root` was baked by this baker, whatever data it was baked from. A
/// base mod from another baker may lack files and table rows the game reads.
#[must_use]
pub fn is_usable(root: &Path) -> bool {
    std::fs::read_to_string(root.join(BASE).join(RECORD))
        .ok()
        .and_then(|text| toml::from_str::<BakeRecord>(&text).ok())
        .is_some_and(|baked| baked.baker == BAKER)
}

/// Bakes `data` into the base mod in `root`, and the Gold Edition's extras into the `gold` mod
/// when the data has them, replacing both mods if already there, and gives back what it baked
/// from. Data with no extras leaves no `gold` mod.
///
/// # Errors
///
/// [`Error::Unpatched`] for data older than v1.51, [`Error::Data`], [`Error::Archive`],
/// [`Error::Index`] or [`Error::Music`] for a source that cannot be read or decoded,
/// [`Error::Rip`] for a CD track and [`Error::Song`] for a song that does not encode, and
/// [`Error::Io`] for a file that cannot be written.
pub fn bake(data: &DataSet, root: &Path, progress: &dyn Progress) -> Result<BakeRecord> {
    let edition = data.edition()?;
    if !matches!(edition.release(), Release::V151 | Release::Gold) {
        return Err(Error::Unpatched(edition.to_string()));
    }
    let record = BakeRecord::of(data)?;
    let mut jobs = jobs(data)?;
    let record_file = toml::to_string(&record).map_err(|source| Error::Write {
        path: RECORD.into(),
        source,
    })?;
    jobs.push(Job::file(BASE, RECORD, record_file.into_bytes()));
    install(&jobs, root, progress)?;
    Ok(record)
}

/// Every file a bake of `data` writes, but for the record of what it baked from.
fn jobs(data: &DataSet) -> Result<Vec<Job>> {
    let table = Effects::ORIGINAL;
    let mut jobs = vec![
        Job::file(
            BASE,
            MANIFEST,
            manifest(BASE, BASE_NAME, BASE_DESCRIPTION)
                .to_toml()?
                .into_bytes(),
        ),
        Job::file(BASE, EFFECTS_TABLE, table_file(&table)?),
    ];
    jobs.extend(
        table_files(&SimTables::ORIGINAL)?
            .into_iter()
            .chain(look_files()?)
            .map(|(path, text)| Job::file(BASE, &path, text.into_bytes())),
    );
    let (script, bytes) = bake_script(data)?;
    jobs.push(Job::file(BASE, &script, bytes));
    jobs.extend(music_jobs(data)?);
    for path in video_files(data) {
        let baked = video_path(&path).unwrap_or_default();
        jobs.push(Job::file(BASE, &baked, data.read(&path)?));
    }
    if let Some(audio) = data.cd_audio() {
        jobs.extend(
            cd_tracks(data)
                .into_iter()
                .map(|track| Job::Track(audio.clone(), track)),
        );
    }
    jobs.extend(
        sample_files(data, &table)?
            .into_iter()
            .map(|(path, bytes)| Job::file(BASE, &path, bytes)),
    );
    let mut sheets = archives(data)
        .par_iter()
        .map(|path| Ok((if is_gold(path) { GOLD } else { BASE }, bake_archive(data, path)?)))
        .collect::<Result<Vec<_>>>()?;
    for stem in pairs(data) {
        if is_gold(stem) {
            sheets.extend(bake_pair(data, stem, |_| true)?.map(|pair| (GOLD, pair)));
        } else {
            sheets.extend(bake_pair(data, stem, |name| !is_gold_item(stem, name))?.map(|pair| (BASE, pair)));
            sheets.extend(bake_pair(data, stem, |name| is_gold_item(stem, name))?.map(|pair| (GOLD, pair)));
        }
    }
    sheets.extend(bake_fonts(data)?.into_iter().map(|font| (BASE, font)));
    for landscape in landscapes(data) {
        jobs.extend(
            bake_terrain(data, landscape)?
                .into_iter()
                .map(|(path, bytes)| Job::file(BASE, &path, bytes)),
        );
    }
    for path in picture_files(data) {
        let (baked, bytes) = bake_picture(data, &path)?;
        jobs.push(Job::file(if is_gold(&path) { GOLD } else { BASE }, &baked, bytes));
    }
    if data.exists(REMAP) {
        jobs.push(Job::file(BASE, REMAP_BAKED, data.read(REMAP)?));
    }
    for path in bob_files(data) {
        sheets.extend(bake_bob(data, path)?.into_iter().map(|part| (BASE, part)));
    }
    let maps = map_files(data)
        .par_iter()
        .map(|path| {
            let into = if is_gold(path) { GOLD } else { BASE };
            let baked = map_path(path).unwrap_or_default();
            Ok(Job::file(into, &baked, data.read(path)?))
        })
        .collect::<Result<Vec<_>>>()?;
    let gold = sheets.iter().any(|&(into, _)| into == GOLD)
        || maps
            .iter()
            .any(|job| matches!(job, Job::File(path, _) if path.starts_with(&format!("{GOLD}/"))));
    jobs.extend(maps);
    for path in mission_files(data) {
        let baked = mission_path(&path).unwrap_or_default();
        jobs.push(Job::file(BASE, &baked, data.read(&path)?));
    }
    for path in text_files(data) {
        let (baked, bytes) = bake_text(data, &path)?;
        jobs.push(Job::file(if is_gold(&path) { GOLD } else { BASE }, &baked, bytes));
    }
    for (into, archive) in sheets {
        let (path, bytes) = archive.table;
        jobs.push(Job::file(into, &path, bytes));
        jobs.extend(archive.items.into_iter().map(|baked| Job::Sprite(into, baked)));
    }
    if gold {
        let mut extras = manifest(GOLD, GOLD_NAME, GOLD_DESCRIPTION);
        extras.depends.push(BASE.into());
        jobs.push(Job::file(GOLD, MANIFEST, extras.to_toml()?.into_bytes()));
    }
    Ok(jobs)
}

/// The songs of `data` to render through the OPL3, and their XMIDI files to keep.
fn music_jobs(data: &DataSet) -> Result<Vec<Job>> {
    let scores = score_files(data)?
        .into_iter()
        .map(|(path, bytes)| Job::file(BASE, &path, bytes));
    Ok(song_jobs(data)?
        .into_iter()
        .map(Job::Song)
        .chain(scores)
        .collect())
}

/// Writes `jobs` beside the mods in `root`, then moves the new baked mods into place of the old
/// ones, removing a baked mod the new bake has no files for.
fn install(jobs: &[Job], root: &Path, progress: &dyn Progress) -> Result<()> {
    let baking = root.join(BAKING);
    remove(&baking)?;
    progress.begin(jobs.iter().map(Job::work).sum());
    // One task a job, so the slow ones, the CD tracks above all, never queue behind each other
    // on one thread.
    jobs.par_iter().with_max_len(1).try_for_each(|job| {
        let (path, bytes) = match job {
            Job::File(path, bytes) => (path.as_str(), std::borrow::Cow::Borrowed(bytes.as_slice())),
            Job::Sprite(into, baked) => {
                let path = format!("{into}/{}", baked.path);
                return write(&baking, &path, &baked.png()?, (progress, job.work()));
            }
            Job::Track(audio, track) => {
                let path = format!("{BASE}/{}", cd_path(*track));
                return write(&baking, &path, &rip(audio, *track)?, (progress, job.work()));
            }
            Job::Song(song) => {
                let path = format!("{BASE}/{}", song_path(Rendition::Opl3, song.number));
                return write(&baking, &path, &render(song)?, (progress, job.work()));
            }
        };
        write(&baking, path, &bytes, (progress, job.work()))
    })?;
    let retired = root.join(RETIRED);
    remove(&retired)?;
    std::fs::create_dir_all(&retired).map_err(Error::io(&retired))?;
    for id in [BASE, GOLD] {
        let current = root.join(id);
        if current.exists() {
            std::fs::rename(&current, retired.join(id)).map_err(Error::io(&current))?;
        }
        let baked = baking.join(id);
        if baked.exists() {
            std::fs::rename(&baked, &current).map_err(Error::io(&baked))?;
        }
    }
    remove(&retired)?;
    remove(&baking)?;
    progress.end();
    Ok(())
}

/// The manifest of the baked mod `id`, named `name` and described by `description`.
fn manifest(id: &str, name: &str, description: &str) -> Manifest {
    Manifest {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        robigo: None,
        version: format!("{BAKER}"),
        depends: Vec::new(),
    }
}

/// Writes `bytes` to `path` under `baking` and reports it.
fn write(baking: &Path, path: &str, bytes: &[u8], (progress, work): (&dyn Progress, u64)) -> Result<()> {
    let target = baking.join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(Error::io(parent))?;
    }
    std::fs::write(&target, bytes).map_err(Error::io(&target))?;
    progress.wrote(path, work);
    Ok(())
}

/// One file a bake writes, by its path from the mods folder: its bytes as they are, a sprite for
/// the mod named first, to encode before it is written, an audio track of the disc image, to rip
/// into the base mod, or a song, to render into it.
enum Job {
    File(String, Vec<u8>),
    Sprite(&'static str, Baked),
    Track(CdAudio, u8),
    Song(SongJob),
}

impl Job {
    /// How much of the bake's work this is.
    const fn work(&self) -> u64 {
        match self {
            Self::File(..) | Self::Sprite(..) => 1,
            Self::Track(..) | Self::Song(_) => SLOW_WORK,
        }
    }

    /// The file at `path` inside the mod `into`.
    fn file(into: &str, path: &str, bytes: Vec<u8>) -> Self {
        Self::File(format!("{into}/{path}"), bytes)
    }
}

/// Removes the folder `path` and all it holds, when it is there.
fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(Error::io(path)(error)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_base_mod_of_this_baker_is_usable() {
        let root = std::env::temp_dir().join(format!("s2-mods-usable-{}", std::process::id()));
        let base = root.join(BASE);
        std::fs::create_dir_all(&base).unwrap();
        let usable = |baker| {
            let record = BakeRecord {
                baker,
                edition: "unknown".into(),
                sources: BTreeMap::new(),
            };
            std::fs::write(base.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
            is_usable(&root)
        };
        let results = (is_usable(&root), usable(BAKER), usable(BAKER - 1));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            results,
            (false, true, false),
            "no record, this baker, an older one"
        );
    }

    #[test]
    fn a_written_file_is_named_by_its_kind_and_its_path_in_that_folder() {
        assert_eq!(step("base/music/opl/3.flac"), ("Music", "opl/3.flac"));
        assert_eq!(step("gold/sprites/io/12.png"), ("Graphics", "io/12.png"));
        assert_eq!(step("base/mod.toml"), ("Files", "mod.toml"));
    }
}
