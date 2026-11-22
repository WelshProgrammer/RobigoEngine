//! A single source of game files: a folder on disk or an ISO9660 CD image.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::audio::{self, CdAudio};
use crate::cue::TrackMode;
use crate::error::{Error, IoContext, Result};
use crate::iso9660::{IsoKind, IsoTree};
use crate::path::{self, find_on_disk, starts_with_ci};
use crate::sector::{Geometry, SpanReader};

/// Where the game's install root sits inside a source, most specific first.
///
/// The retail CD keeps `S2.EXE` in `S2/ROOT/` and everything else in `S2/`. The installer merges
/// the two, so both are mounted at the root with `ROOT` itself hidden.
const LAYOUTS: [&[&[&str]]; 3] = [&[&[]], &[&["S2", "ROOT"], &["S2"]], &[&["S2"]]];
const GAME_EXE: &str = "S2.EXE";

/// What kind of thing a [`DirEntry`] or [`Metadata`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// Size and kind of a file or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    /// File or directory.
    pub kind: EntryKind,
    /// Length in bytes; 0 for directories.
    pub len: u64,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The name as stored by the source that provided it (CD names are upper case).
    pub name: String,
    /// File or directory.
    pub kind: EntryKind,
    /// Length in bytes; 0 for directories.
    pub len: u64,
}

/// Whether a [`Layer`] reads from a folder or from a CD image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    /// A folder on disk, such as an installed game or the v1.51 patch.
    Folder,
    /// An ISO9660 image: `.iso`, raw `.bin`/`.gog`, or the data track of a cue sheet.
    Image,
}

/// One source of game files. Several layers stack into a [`crate::DataSet`].
///
/// Paths inside a layer are relative to the game's install root: when a source holds the retail
/// CD layout (`S2/ROOT/S2.EXE`), its `S2/ROOT` and `S2` folders both appear at the root.
#[derive(Debug, Clone)]
pub struct Layer {
    source: PathBuf,
    backend: Backend,
    mounts: Vec<Vec<String>>,
    audio: Option<CdAudio>,
}

#[derive(Debug, Clone)]
enum Backend {
    Folder,
    Image(Image),
}

#[derive(Debug, Clone)]
struct Image {
    path: PathBuf,
    geometry: Geometry,
    base: u64,
    tree: IsoTree,
}

#[derive(Debug)]
pub(crate) struct Entry<'a> {
    pub meta: Metadata,
    loc: Loc<'a>,
}

#[derive(Debug)]
enum Loc<'a> {
    Disk(PathBuf),
    Iso(&'a Image, u32),
    IsoDir,
}

impl Layer {
    /// Opens a folder or an image file, whichever `path` is.
    ///
    /// # Errors
    ///
    /// See [`Layer::folder`] and [`Layer::image`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.is_dir() {
            Self::folder(path)
        } else {
            Self::image(path)
        }
    }

    /// Opens a folder whose names are matched without regard to case.
    ///
    /// If a folder holds two names that differ only in case (possible on Linux, never on DOS),
    /// an exact match wins, then the first in byte order. The two are not merged.
    ///
    /// # Errors
    ///
    /// Fails if `path` does not exist or is not a folder.
    pub fn folder(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !std::fs::metadata(path).at(path)?.is_dir() {
            return Err(Error::UnsupportedSource(path.to_owned()));
        }
        Ok(Self::with_layout(path, Backend::Folder, None))
    }

    /// Opens a CD image: a `.cue` or GOG `.ins` sheet, a raw `.bin`/`.gog`, or an `.iso`. A
    /// sheet's data track is read and its audio tracks become [`Layer::cd_audio`].
    ///
    /// A `.bin` or `.gog` with a cue sheet beside it that names it is opened through that sheet,
    /// which brings its audio tracks. The sector size (2048 or 2352) is taken from the sheet, or
    /// recognised from the sync pattern when there is no sheet.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, has an extension this crate does not handle, or holds
    /// no readable ISO9660 volume.
    pub fn image(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match ext.as_str() {
            "cue" | "ins" => Self::from_sheet(path),
            "bin" | "gog" => match sheet_for(path) {
                Some(sheet) => Self::from_sheet(&sheet),
                None => Self::raw_image(path),
            },
            "iso" | "img" => Self::raw_image(path),
            _ => Err(Error::UnsupportedSource(path.to_owned())),
        }
    }

    fn from_sheet(path: &Path) -> Result<Self> {
        let sheet = audio::read_sheet(path)?;
        let audio = CdAudio::from_sheet(path, &sheet)?;
        let fail = |reason: String| Error::Cue {
            path: path.to_owned(),
            line: 0,
            reason,
        };
        let (file, track) = sheet
            .files
            .iter()
            .find_map(|f| {
                f.tracks
                    .iter()
                    .find(|t| t.mode != TrackMode::Audio)
                    .map(|t| (f, t))
            })
            .ok_or_else(|| fail("the sheet has no data track".into()))?;
        let geometry = match &track.mode {
            TrackMode::Data(g) if file.binary => *g,
            TrackMode::Unsupported(mode) => return Err(fail(format!("track mode {mode} is not supported"))),
            _ => {
                return Err(fail(format!(
                    "the data track is in {}, which is not a BINARY file",
                    file.name
                )));
            }
        };
        let data = audio::locate(path::folder_of(path), &file.name)
            .ok_or_else(|| fail(format!("{} is not next to the cue sheet", file.name)))?;
        let image = Image::open(data, geometry, u64::from(track.start) * geometry.sector)?;
        Ok(Self::with_layout(path, Backend::Image(image), Some(audio)))
    }

    fn raw_image(path: &Path) -> Result<Self> {
        let mut head = [0; 16];
        let mut file = File::open(path).at(path)?;
        let n = file.read(&mut head).at(path)?;
        let image = Image::open(path.to_owned(), Geometry::detect(&head[..n]), 0)?;
        Ok(Self::with_layout(path, Backend::Image(image), None))
    }

    fn with_layout(source: &Path, backend: Backend, audio: Option<CdAudio>) -> Self {
        let mut layer = Self {
            source: source.to_owned(),
            backend,
            mounts: vec![Vec::new()],
            audio,
        };
        let layout = LAYOUTS.iter().find(|layout| {
            let mut exe = layout[0].to_vec();
            exe.push(GAME_EXE);
            layer
                .backend_stat(&exe)
                .is_some_and(|e| e.meta.kind == EntryKind::File)
        });
        if let Some(layout) = layout {
            layer.mounts = layout
                .iter()
                .map(|m| m.iter().map(|&s| s.to_owned()).collect())
                .collect();
        }
        layer
    }

    /// The folder, image or cue sheet this layer was opened from.
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Whether this layer is a folder or a CD image.
    #[must_use]
    pub fn kind(&self) -> LayerKind {
        match self.backend {
            Backend::Folder => LayerKind::Folder,
            Backend::Image(_) => LayerKind::Image,
        }
    }

    /// The audio tracks of the cue sheet this layer was opened from, if any.
    #[must_use]
    pub fn cd_audio(&self) -> Option<&CdAudio> {
        self.audio.as_ref()
    }

    pub(crate) fn has_game(&self) -> bool {
        self.stat(&[GAME_EXE])
            .is_some_and(|e| e.meta.kind == EntryKind::File)
    }

    pub(crate) fn stat(&self, parts: &[&str]) -> Option<Entry<'_>> {
        self.mounted(parts).find_map(|(_, full)| self.backend_stat(&full))
    }

    /// Adds this layer's entries under `parts` to `into` (keyed by upper-case name, existing keys
    /// win). Returns false when the path is not a directory in this layer.
    pub(crate) fn list_into(&self, parts: &[&str], into: &mut BTreeMap<String, DirEntry>) -> bool {
        let mut found = false;
        for (mount, full) in self.mounted(parts) {
            let Some(entries) = self.backend_list(&full) else {
                continue;
            };
            found = true;
            for entry in entries {
                let mut child = full.clone();
                child.push(&entry.name);
                if !self.hidden(mount, &child) {
                    into.entry(entry.name.to_ascii_uppercase()).or_insert(entry);
                }
            }
        }
        found
    }

    fn mounted<'a>(&'a self, parts: &'a [&'a str]) -> impl Iterator<Item = (usize, Vec<&'a str>)> + 'a {
        self.mounts.iter().enumerate().filter_map(move |(i, mount)| {
            let full: Vec<&str> = mount
                .iter()
                .map(String::as_str)
                .chain(parts.iter().copied())
                .collect();
            (!self.hidden(i, &full)).then_some((i, full))
        })
    }

    /// A path seen through one mount is hidden when it runs into a more specific mount.
    fn hidden(&self, mount: usize, full: &[&str]) -> bool {
        let own = self.mounts[mount].len();
        self.mounts
            .iter()
            .any(|m| m.len() > own && starts_with_ci(full, m))
    }

    fn backend_stat(&self, parts: &[&str]) -> Option<Entry<'_>> {
        match &self.backend {
            Backend::Folder => {
                let path = find_on_disk(&self.source, parts)?;
                let meta = disk_meta(&std::fs::metadata(&path).ok()?);
                Some(Entry {
                    meta,
                    loc: Loc::Disk(path),
                })
            }
            Backend::Image(image) => {
                let node = image.tree.walk(parts)?;
                Some(match image.tree.node(node).kind {
                    IsoKind::Dir(_) => Entry {
                        meta: Metadata {
                            kind: EntryKind::Directory,
                            len: 0,
                        },
                        loc: Loc::IsoDir,
                    },
                    IsoKind::File { lba, len } => Entry {
                        meta: Metadata {
                            kind: EntryKind::File,
                            len: u64::from(len),
                        },
                        loc: Loc::Iso(image, lba),
                    },
                })
            }
        }
    }

    fn backend_list(&self, parts: &[&str]) -> Option<Vec<DirEntry>> {
        match &self.backend {
            Backend::Folder => {
                let dir = find_on_disk(&self.source, parts)?;
                let entries = std::fs::read_dir(dir).ok()?.filter_map(|entry| {
                    let entry = entry.ok()?;
                    let name = entry.file_name().into_string().ok()?;
                    let meta = disk_meta(&std::fs::metadata(entry.path()).ok()?);
                    Some(DirEntry {
                        name,
                        kind: meta.kind,
                        len: meta.len,
                    })
                });
                Some(entries.collect())
            }
            Backend::Image(image) => {
                let dir = image.tree.walk(parts)?;
                if !matches!(image.tree.node(dir).kind, IsoKind::Dir(_)) {
                    return None;
                }
                let entries = image.tree.children(dir).iter().map(|&c| {
                    let node = image.tree.node(c);
                    let (kind, len) = match node.kind {
                        IsoKind::Dir(_) => (EntryKind::Directory, 0),
                        IsoKind::File { len, .. } => (EntryKind::File, u64::from(len)),
                    };
                    DirEntry {
                        name: node.name.clone(),
                        kind,
                        len,
                    }
                });
                Some(entries.collect())
            }
        }
    }
}

impl Entry<'_> {
    /// Opens the entry for reading; `shown` names it in errors.
    pub(crate) fn open(&self, shown: &str) -> Result<DataReader> {
        let inner = match &self.loc {
            Loc::Disk(path) if self.meta.kind == EntryKind::File => {
                Inner::Disk(File::open(path).at(path)?, self.meta.len)
            }
            Loc::Iso(image, lba) => {
                let file = File::open(&image.path).at(&image.path)?;
                let start = u64::from(*lba) * Geometry::COOKED.payload;
                Inner::Image(SpanReader::new(
                    file,
                    image.geometry,
                    image.base,
                    start,
                    self.meta.len,
                ))
            }
            Loc::Disk(_) | Loc::IsoDir => return Err(Error::NotAFile(shown.to_owned())),
        };
        Ok(DataReader(inner))
    }
}

impl Image {
    fn open(path: PathBuf, geometry: Geometry, base: u64) -> Result<Self> {
        let file = File::open(&path).at(&path)?;
        let file_len = file.metadata().at(&path)?.len();
        let raw = file_len.saturating_sub(base);
        let size = if geometry == Geometry::COOKED {
            raw
        } else {
            raw / geometry.sector * geometry.payload
        };
        let mut reader = SpanReader::new(file, geometry, base, 0, size);
        let tree = IsoTree::read(&mut reader, size).map_err(|reason| Error::Image {
            path: path.clone(),
            reason,
        })?;
        Ok(Self {
            path,
            geometry,
            base,
            tree,
        })
    }
}

fn disk_meta(meta: &std::fs::Metadata) -> Metadata {
    if meta.is_dir() {
        Metadata {
            kind: EntryKind::Directory,
            len: 0,
        }
    } else {
        Metadata {
            kind: EntryKind::File,
            len: meta.len(),
        }
    }
}

/// Finds a `.cue` or `.ins` beside `image` whose `FILE` line names it.
fn sheet_for(image: &Path) -> Option<PathBuf> {
    let name = image.file_name()?.to_str()?;
    sheets_in(path::folder_of(image)).into_iter().find(|sheet| {
        audio::read_sheet(sheet).is_ok_and(|s| {
            s.files.iter().any(|f| {
                path::split(&f.name)
                    .ok()
                    .and_then(|parts| parts.last().map(|last| last.eq_ignore_ascii_case(name)))
                    .unwrap_or(false)
            })
        })
    })
}

/// Cue sheets in a folder, `.ins` before `.cue`, each group sorted by name.
pub(crate) fn sheets_in(folder: &Path) -> Vec<PathBuf> {
    files_with(folder, &["ins", "cue"])
}

/// Files in `folder` with one of `exts` (lower case), ordered by extension rank then name.
pub(crate) fn files_with(folder: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(usize, PathBuf)> = entries
        .filter_map(|e| {
            let path = e.ok()?.path();
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            let rank = exts.iter().position(|&x| x == ext)?;
            path.is_file().then_some((rank, path))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// A readable, seekable handle on one game file, from whichever layer provides it.
#[derive(Debug)]
pub struct DataReader(Inner);

#[derive(Debug)]
enum Inner {
    Disk(File, u64),
    Image(SpanReader),
}

impl DataReader {
    /// Length of the file in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        match &self.0 {
            Inner::Disk(_, len) => *len,
            Inner::Image(r) => r.len(),
        }
    }

    /// Whether the file is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Read for DataReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            Inner::Disk(f, _) => f.read(buf),
            Inner::Image(r) => r.read(buf),
        }
    }
}

impl Seek for DataReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        match &mut self.0 {
            Inner::Disk(f, _) => f.seek(to),
            Inner::Image(r) => r.seek(to),
        }
    }
}
