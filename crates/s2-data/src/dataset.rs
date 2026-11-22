//! The stacked view of every layer that later crates read game files through.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use crate::audio::CdAudio;
use crate::edition::{self, Edition};
use crate::error::{Error, IoContext, Result};
use crate::layer::{self, DataReader, DirEntry, Entry, EntryKind, Layer, Metadata};
use crate::path;

const WORLD_CAMPAIGN: &str = "DATA/MAPS2/AFRICA.WLD";

/// The game's files as one tree, built from layers where later layers override earlier ones.
///
/// Paths are relative to the install root, separated by `/` or `\`, and matched without regard
/// to ASCII case, so `data\txt\names.eng` and `DATA/TXT/NAMES.ENG` name the same file.
///
/// ```no_run
/// use s2_data::{DataSet, Layer};
///
/// let data = DataSet::discover("Settlers2.cue")?.with_layer(Layer::folder("v151")?);
/// let names = data.read("DATA/TXT/NAMES.ENG")?;
/// # Ok::<(), s2_data::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct DataSet {
    layers: Vec<Layer>,
    audio: Option<CdAudio>,
}

impl DataSet {
    /// Stacks `layers` in order: each one overrides those before it.
    #[must_use]
    pub fn new(layers: impl IntoIterator<Item = Layer>) -> Self {
        Self {
            layers: layers.into_iter().collect(),
            audio: None,
        }
    }

    /// Builds a data set from whatever a player would point at.
    ///
    /// Accepts an installed game folder (plain DOS install or the GOG folder), a folder holding
    /// the retail CD's files, a `.cue`/`.ins` sheet, a `.bin`/`.gog` image or an `.iso`. A folder
    /// without `S2.EXE` is searched for a `.cue`, `.ins` or `.iso` that holds the game. Next to
    /// an installed game, a `.ins` or `.cue` sheet (GOG's `SETTLERS2.ins`) supplies the CD audio.
    ///
    /// # Errors
    ///
    /// [`Error::GameNotFound`] when nothing there contains `S2.EXE`, or the error from opening
    /// a sheet or image that turned out to be damaged.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if std::fs::metadata(path).at(path)?.is_dir() {
            return Self::discover_folder(path);
        }
        let image = Layer::image(path)?;
        if image.has_game() {
            return Ok(Self::new([image]));
        }
        let folder = Layer::folder(path::folder_of(path))?;
        if folder.has_game() {
            let audio = image.cd_audio().cloned();
            return Ok(Self {
                layers: vec![folder],
                audio,
            });
        }
        Err(Error::GameNotFound(path.to_owned()))
    }

    fn discover_folder(path: &Path) -> Result<Self> {
        let folder = Layer::folder(path)?;
        if folder.has_game() {
            let audio = layer::sheets_in(path)
                .first()
                .map(CdAudio::from_cue)
                .transpose()?;
            return Ok(Self {
                layers: vec![folder],
                audio,
            });
        }
        for candidate in layer::files_with(path, &["cue", "ins", "iso"]) {
            let image = Layer::image(&candidate)?;
            if image.has_game() {
                return Ok(Self::new([image]));
            }
        }
        Err(Error::GameNotFound(path.to_owned()))
    }

    /// Adds `layer` on top, so its files override everything already present.
    #[must_use]
    pub fn with_layer(mut self, layer: Layer) -> Self {
        self.push_layer(layer);
        self
    }

    /// Adds `layer` on top, so its files override everything already present.
    pub fn push_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Uses `audio` for [`DataSet::cd_audio`] instead of any sheet a layer was opened from.
    #[must_use]
    pub fn with_cd_audio(mut self, audio: CdAudio) -> Self {
        self.audio = Some(audio);
        self
    }

    /// The layers, bottom first.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The CD audio in play: the one set explicitly or found by [`DataSet::discover`], otherwise
    /// that of the topmost layer opened from a cue sheet.
    #[must_use]
    pub fn cd_audio(&self) -> Option<&CdAudio> {
        self.audio
            .as_ref()
            .or_else(|| self.layers.iter().rev().find_map(Layer::cd_audio))
    }

    /// Whether a file or directory exists at `path` in any layer.
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.find(path).is_ok()
    }

    /// The kind and size of whatever `path` names, taken from the topmost layer that has it.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if no layer has it, [`Error::InvalidPath`] for paths such as `../x`.
    pub fn metadata(&self, path: &str) -> Result<Metadata> {
        Ok(self.find(path)?.1.meta)
    }

    /// The size in bytes of the file at `path`.
    ///
    /// # Errors
    ///
    /// As [`DataSet::metadata`], plus [`Error::NotAFile`] when `path` is a directory.
    pub fn file_size(&self, path: &str) -> Result<u64> {
        let (shown, entry) = self.find(path)?;
        match entry.meta.kind {
            EntryKind::File => Ok(entry.meta.len),
            EntryKind::Directory => Err(Error::NotAFile(shown)),
        }
    }

    /// Reads the whole file at `path`.
    ///
    /// # Errors
    ///
    /// As [`DataSet::file_size`], plus [`Error::Io`] if the underlying file or image cannot be
    /// read.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let mut reader = self.open(path)?;
        let mut out = Vec::with_capacity(crate::sector::to_usize(reader.len()));
        reader.read_to_end(&mut out).map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
        Ok(out)
    }

    /// Opens the file at `path` for streaming, for large files such as the intro videos.
    ///
    /// # Errors
    ///
    /// As [`DataSet::read`].
    pub fn open(&self, path: &str) -> Result<DataReader> {
        let (shown, entry) = self.find(path)?;
        entry.open(&shown)
    }

    /// Lists the directory at `path`, merging every layer. Where names clash, the topmost layer
    /// wins. Entries are sorted by name, ignoring case. Use `""` for the root.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if no layer has the directory, [`Error::NotADirectory`] if it is a file.
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let parts = path::split(path)?;
        let mut merged = BTreeMap::new();
        let mut found = false;
        for layer in self.layers.iter().rev() {
            found |= layer.list_into(&parts, &mut merged);
        }
        if found {
            Ok(merged.into_values().collect())
        } else if self.find(path).is_ok() {
            Err(Error::NotADirectory(path::display(&parts)))
        } else {
            Err(Error::NotFound(path::display(&parts)))
        }
    }

    /// Works out which build of the game this is by reading `S2.EXE` and checking for the World
    /// Campaign maps. Not cached: each call reads the executable again.
    ///
    /// # Errors
    ///
    /// Fails if `S2.EXE` cannot be read, or with [`Error::UnknownEdition`] if it carries no
    /// Settlers II version banner.
    pub fn edition(&self) -> Result<Edition> {
        let exe = self.read("S2.EXE")?;
        let world_campaign = self
            .metadata(WORLD_CAMPAIGN)
            .is_ok_and(|m| m.kind == EntryKind::File);
        edition::classify(&exe, world_campaign)
    }

    fn find(&self, path: &str) -> Result<(String, Entry<'_>)> {
        let parts = path::split(path)?;
        let shown = path::display(&parts);
        match self.layers.iter().rev().find_map(|layer| layer.stat(&parts)) {
            Some(entry) => Ok((shown, entry)),
            None => Err(Error::NotFound(shown)),
        }
    }
}
