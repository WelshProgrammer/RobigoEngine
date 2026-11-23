use std::path::{Path, PathBuf};

use crate::{BASE, Error, Manifest, ROBIGO_VERSION, Result};

/// A merged table: one row a key, each row a table of fields.
pub type Table = toml::Table;

/// The field that takes a row out of a table a mod below supplied.
const REMOVED: &str = "removed";

/// The base mod and the player's mods, in load order.
#[derive(Debug, Clone)]
pub struct ModStack {
    mods: Vec<(Manifest, PathBuf)>,
    language: String,
}

impl ModStack {
    /// The base mod in `root`, then each mod `order` names. A mod loads after every mod it
    /// depends on, so `order` lists dependencies first.
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] for a mod with no folder or manifest, [`Error::Toml`] for a manifest
    /// that does not parse, [`Error::Depends`] for a mod listed before what it needs.
    pub fn open(root: &Path, order: &[String]) -> Result<Self> {
        let mut mods: Vec<(Manifest, PathBuf)> = Vec::new();
        for id in std::iter::once(BASE).chain(order.iter().map(String::as_str)) {
            let dir = root.join(id);
            if !dir.join(crate::MANIFEST).is_file() {
                return Err(Error::Missing(id.into()));
            }
            let manifest = Manifest::read(&dir)?;
            if !manifest.fits(ROBIGO_VERSION) {
                log::warn!(
                    "mod {id} was written for Robigo {}, and this is {ROBIGO_VERSION}",
                    manifest.robigo.as_deref().unwrap_or_default()
                );
            }
            for needs in &manifest.depends {
                if !mods.iter().any(|(loaded, _)| &loaded.id == needs) {
                    return Err(Error::Depends {
                        id: id.into(),
                        needs: needs.clone(),
                    });
                }
            }
            mods.push((manifest, dir));
        }
        Ok(Self {
            mods,
            language: crate::texts::ENGLISH.into(),
        })
    }

    /// The stack reading its texts in `language`, the name of a folder under `texts/` such as
    /// `de`, where a mod has them.
    #[must_use]
    pub fn in_language(mut self, language: &str) -> Self {
        language.clone_into(&mut self.language);
        self
    }

    /// The language the stack reads its texts in. English until [`Self::in_language`] says
    /// otherwise.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    /// The mods, base first.
    pub fn mods(&self) -> impl Iterator<Item = &Manifest> {
        self.mods.iter().map(|(manifest, _)| manifest)
    }

    /// Whether the mod `id` is loaded.
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.mods().any(|manifest| manifest.id == id)
    }

    /// The file at `path` inside a mod, from the last mod that has one.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<PathBuf> {
        self.mods
            .iter()
            .rev()
            .map(|(_, dir)| dir.join(path))
            .find(|file| file.is_file())
    }

    /// The first of `paths` inside the last mod that has any of them, and that mod's id.
    #[must_use]
    pub fn file_of(&self, paths: &[String]) -> Option<(&str, PathBuf)> {
        self.mods.iter().rev().find_map(|(manifest, dir)| {
            paths
                .iter()
                .map(|path| dir.join(path))
                .find(|file| file.is_file())
                .map(|file| (manifest.id.as_str(), file))
        })
    }

    /// Every mod that has a file at `path`, in load order, each with its file.
    #[must_use]
    pub fn holders(&self, path: &str) -> Vec<(&Manifest, PathBuf)> {
        self.mods
            .iter()
            .map(|(manifest, dir)| (manifest, dir.join(path)))
            .filter(|(_, file)| file.is_file())
            .collect()
    }

    /// The names of the files in `folder` inside any of the mods, sorted, each once.
    #[must_use]
    pub fn list(&self, folder: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .mods
            .iter()
            .filter_map(|(_, dir)| std::fs::read_dir(dir.join(folder)).ok())
            .flatten()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The bytes of the file at `path`, from the last mod that has one, or `None` when no mod
    /// does.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file is there but cannot be read.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        self.file(path)
            .map(|file| std::fs::read(&file).map_err(Error::io(&file)))
            .transpose()
    }

    /// The table at `path`, merged over every mod that has it, base first: a mod's row adds
    /// a row or replaces the fields it names in one below, and a row holding `removed = true`
    /// takes the row out.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] or [`Error::Toml`] for a table file that cannot be read or parsed.
    pub fn table(&self, path: &str) -> Result<Table> {
        let mut merged = Table::new();
        for (_, dir) in &self.mods {
            let file = dir.join(path);
            if !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file).map_err(Error::io(&file))?;
            let table: Table = toml::from_str(&text).map_err(|source| Error::Toml {
                path: file,
                source: Box::new(source),
            })?;
            merge(&mut merged, table);
        }
        Ok(merged)
    }
}

/// Lays `over` on `under` row by row.
fn merge(under: &mut Table, over: Table) {
    for (key, value) in over {
        match value {
            toml::Value::Table(row) if row.get(REMOVED).and_then(toml::Value::as_bool) == Some(true) => {
                under.remove(&key);
            }
            toml::Value::Table(row) => match under.get_mut(&key) {
                Some(toml::Value::Table(existing)) => existing.extend(row),
                _ => {
                    under.insert(key, toml::Value::Table(row));
                }
            },
            other => {
                under.insert(key, other);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn a_mod_changes_only_the_fields_it_names() {
        let mut base = table("[27]\nvolume = 60\nrate = 9000\n\n[28]\nvolume = 70\nrate = 16000\n");
        merge(
            &mut base,
            table("[27]\nvolume = 90\n\n[40]\nvolume = 10\nrate = 11025\n"),
        );
        assert_eq!(
            base,
            table(
                "[27]\nvolume = 90\nrate = 9000\n\n[28]\nvolume = 70\nrate = 16000\n\n[40]\nvolume = 10\nrate = 11025\n"
            )
        );
    }

    #[test]
    fn a_removed_row_is_gone() {
        let mut base = table("[27]\nvolume = 60\n\n[28]\nvolume = 70\n");
        merge(&mut base, table("[28]\nremoved = true\n"));
        assert_eq!(base, table("[27]\nvolume = 60\n"));
    }
}
