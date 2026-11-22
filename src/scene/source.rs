//! Finds and decodes the map the viewer was pointed at.

use std::path::PathBuf;

use s2_assets::Landscape;
use s2_formats::map::{self, Map, Terrain};
use s2_mods::Sheets;
use s2_sim::replay::content_hash;

use crate::error::{Error, Result};

/// A decoded map with what the viewer shows about it.
pub(crate) struct LoadedMap {
    pub map: Map,
    /// The header's title, or the file name for headerless worlds.
    pub title: String,
    /// The header's landscape: `Ok(None)` without a header, `Err(byte)` for an unknown set.
    pub landscape: std::result::Result<Option<Landscape>, u8>,
    /// [`content_hash`] of the map file, or of a world file followed by its area table.
    pub hash: u64,
    /// Whether the map came from the game's data rather than a file on disk.
    pub in_data: bool,
    /// The bytes of a `.WLD` file from disk, which a save keeps.
    pub file: Option<Vec<u8>>,
}

/// Where a file came from, so its siblings are read from the same place.
enum Origin<'a> {
    Data(&'a Sheets<'a>),
    Disk,
}

/// Loads `spec`: a path inside the game's data, from the loaded mods for a map folder's path, or
/// failing that a file on disk.
///
/// A `WORLD_V1.0` file decodes as a `.WLD`. A file named `WORLDnnn.DAT` decodes with the
/// `CONTInnn.DAT` beside it, which covers both the v1.02 worlds and savegame slots.
pub(crate) fn load(sheets: &Sheets<'_>, spec: &str) -> Result<LoadedMap> {
    let origin = if sheets.has_map_file(spec) || sheets.maps_from_mods(spec) {
        Origin::Data(sheets)
    } else {
        Origin::Disk
    };
    let in_data = matches!(origin, Origin::Data(_));
    let bytes = origin.read(spec)?;
    let name = file_name(spec);
    if bytes.starts_with(map::SIGNATURE) {
        return wld(spec, bytes, in_data);
    }
    let Some(number) = world_number(name) else {
        return Err(Error::UnknownMap(spec.to_owned()));
    };
    let conti_path = sibling(spec, &format!("CONTI{number}.DAT"));
    let conti = origin.read(&conti_path)?;
    let map = Map::parse_v102(&bytes, &conti).map_err(|e| Error::Map(spec.to_owned(), e))?;
    let mut both = bytes;
    both.extend(conti);
    Ok(LoadedMap {
        map,
        title: name.to_owned(),
        landscape: Ok(None),
        hash: content_hash(&both),
        in_data,
        file: None,
    })
}

/// Decodes `bytes`, the `.WLD` file `spec` names.
pub(crate) fn wld(spec: &str, bytes: Vec<u8>, in_data: bool) -> Result<LoadedMap> {
    let map = Map::parse_wld(&bytes).map_err(|e| Error::Map(spec.to_owned(), e))?;
    let header = map.header();
    let title = header
        .map(map::Header::title)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| file_name(spec).to_owned());
    let landscape = match header.map(map::Header::terrain) {
        None => Ok(None),
        Some(Terrain::Other(byte)) => Err(byte),
        Some(terrain) => Ok(Landscape::from_terrain(terrain)),
    };
    Ok(LoadedMap {
        map,
        title,
        landscape,
        hash: content_hash(&bytes),
        in_data,
        file: (!in_data).then_some(bytes),
    })
}

impl Origin<'_> {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Self::Data(sheets) => Ok(sheets.map_file(path)?),
            Self::Disk => std::fs::read(path).map_err(|source| Error::Io {
                path: PathBuf::from(path),
                source,
            }),
        }
    }
}

fn file_name(spec: &str) -> &str {
    spec.rsplit(['/', '\\']).next().unwrap_or(spec)
}

/// The three digits of a `WORLDnnn.DAT` name, any case.
fn world_number(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    let digits = upper.strip_prefix("WORLD")?.strip_suffix(".DAT")?;
    (digits.len() == 3 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.to_owned())
}

/// `name` in the same folder as `spec`, keeping its separator.
fn sibling(spec: &str, name: &str) -> String {
    match spec.rfind(['/', '\\']) {
        Some(at) => format!("{}{name}", &spec[..=at]),
        None => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_files_are_recognised_by_name() {
        assert_eq!(world_number("WORLD900.DAT").as_deref(), Some("900"));
        assert_eq!(world_number("world107.dat").as_deref(), Some("107"));
        assert_eq!(world_number("WORLD90.DAT"), None);
        assert_eq!(world_number("OMAP00.WLD"), None);
    }

    #[test]
    fn siblings_share_the_folder() {
        assert_eq!(sibling("SAVE/WORLD900.DAT", "CONTI900.DAT"), "SAVE/CONTI900.DAT");
        assert_eq!(
            sibling("data\\maps\\world100.dat", "CONTI100.DAT"),
            "data\\maps\\CONTI100.DAT"
        );
        assert_eq!(sibling("WORLD100.DAT", "CONTI100.DAT"), "CONTI100.DAT");
        assert_eq!(file_name("DATA/MAPS3/OMAP00.WLD"), "OMAP00.WLD");
    }

    #[test]
    fn unknown_files_are_refused() {
        let dir = std::env::temp_dir().join(format!("robigo-view-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NOTAMAP.BIN");
        std::fs::write(&path, b"hello").unwrap();
        let data = s2_data::DataSet::default();
        let sheets = Sheets {
            stack: None,
            data: &data,
        };
        let result = load(&sheets, &path.display().to_string());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(result, Err(Error::UnknownMap(_))));
    }
}
