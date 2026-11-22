//! Reading and writing the rows of a mod's TOML tables, shared by every table the bake writes.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::sprites::row_error;
use crate::{Error, ModStack, Result};

/// `value` as a TOML integer.
pub(crate) fn integer(value: impl Into<i64>) -> toml::Value {
    toml::Value::Integer(value.into())
}

/// `value` as TOML text, for the file at `path`.
pub(crate) fn write(path: &str, value: &impl Serialize) -> Result<String> {
    toml::to_string(value).map_err(|source| Error::Write {
        path: path.into(),
        source,
    })
}

/// `row` as a TOML value, for the file at `path`.
pub(crate) fn value(path: &str, row: impl Serialize) -> Result<toml::Value> {
    toml::Value::try_from(row).map_err(|source| Error::Write {
        path: path.into(),
        source,
    })
}

/// The rows of the table at `path`, merged through the stack and each decoded as a `T`.
pub(crate) fn rows<T: DeserializeOwned>(stack: &ModStack, path: &str) -> Result<Vec<(String, T)>> {
    let table = merged(stack, path)?;
    values(path, "", table)
}

/// The table at `path`, merged through the stack.
pub(crate) fn merged(stack: &ModStack, path: &str) -> Result<toml::Table> {
    if stack.file(path).is_none() {
        return Err(Error::NotInMods(path.into()));
    }
    stack.table(path)
}

/// The table at `path` decoded whole as a `T`.
pub(crate) fn whole<T: DeserializeOwned>(stack: &ModStack, path: &str) -> Result<T> {
    toml::Value::Table(merged(stack, path)?)
        .try_into()
        .map_err(|error: toml::de::Error| row_error(path, "", error.message()))
}

/// Each value of `table`, part `part` of the file at `path`, decoded as a `T`.
pub(crate) fn values<T: DeserializeOwned>(
    path: &str,
    part: &str,
    table: toml::Table,
) -> Result<Vec<(String, T)>> {
    table
        .into_iter()
        .map(|(key, value)| {
            let row = value
                .try_into()
                .map_err(|error: toml::de::Error| row_error(path, &dotted(part, &key), error.message()))?;
            Ok((key, row))
        })
        .collect()
}

/// `key` inside part `part` of a file, as an error names it.
pub(crate) fn dotted(part: &str, key: &str) -> String {
    if part.is_empty() {
        key.to_owned()
    } else {
        format!("{part}.{key}")
    }
}

/// `rows` in the order of `names`: each row's key is one of the names and each name has a row.
pub(crate) fn by_name<T>(path: &str, part: &str, names: &[&str], rows: Vec<(String, T)>) -> Result<Vec<T>> {
    let mut slots: Vec<Option<T>> = names.iter().map(|_| None).collect();
    for (key, row) in rows {
        let index = names
            .iter()
            .position(|&name| name == key)
            .ok_or_else(|| row_error(path, &dotted(part, &key), "not a name the game has"))?;
        slots[index] = Some(row);
    }
    slots
        .into_iter()
        .zip(names)
        .map(|(slot, name)| slot.ok_or_else(|| row_error(path, &dotted(part, name), "missing")))
        .collect()
}

/// The `N` values of `table`, part `part` of the file at `path`, in the order of `names`.
pub(crate) fn named_values<T: DeserializeOwned, const N: usize>(
    path: &str,
    part: &str,
    names: &[&str],
    table: toml::Table,
) -> Result<[T; N]> {
    let values = by_name(path, part, names, values(path, part, table)?)?;
    values
        .try_into()
        .map_err(|_| row_error(path, part, format!("{N} values")))
}

/// The value named `name` in `pairs`.
pub(crate) fn lookup<T: Copy>(pairs: &[(&str, T)], name: &str) -> Option<T> {
    pairs
        .iter()
        .find(|(known, _)| *known == name)
        .map(|&(_, value)| value)
}

/// The name of `value` in `pairs`.
pub(crate) fn name_of<T: PartialEq>(pairs: &[(&'static str, T)], value: &T) -> &'static str {
    pairs
        .iter()
        .find(|(_, known)| known == value)
        .map_or("", |&(name, _)| name)
}

/// `rows` keyed by their numbers from 0, in number order, each number once and none missing.
pub(crate) fn numbered<T>(path: &str, rows: Vec<(String, T)>) -> Result<Vec<T>> {
    let mut slots: Vec<Option<T>> = rows.iter().map(|_| None).collect();
    for (key, row) in rows {
        let slot = key
            .parse::<usize>()
            .ok()
            .and_then(|index| slots.get_mut(index))
            .filter(|slot| slot.is_none())
            .ok_or_else(|| row_error(path, &key, "the rows are numbered from 0, each once"))?;
        *slot = Some(row);
    }
    Ok(slots.into_iter().flatten().collect())
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::{Path, PathBuf};

    use crate::ModStack;

    /// A scratch mods folder with a base mod holding `files`, and a mod `over` on top of it
    /// holding `extra` when given.
    pub(crate) fn stack(
        name: &str,
        files: &[(String, String)],
        extra: Option<(&str, &str)>,
    ) -> (PathBuf, ModStack) {
        let root = std::env::temp_dir().join(format!("s2-mods-sim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let put = |dir: &Path, path: &str, text: &str| {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        };
        let manifest = |id: &str, depends: &str| {
            format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"1\"\ndepends = [{depends}]\n")
        };
        let base = root.join(crate::BASE);
        put(&base, crate::MANIFEST, &manifest(crate::BASE, ""));
        for (path, text) in files {
            put(&base, path, text);
        }
        let mut order = Vec::new();
        if let Some((path, text)) = extra {
            let over = root.join("over");
            put(&over, crate::MANIFEST, &manifest("over", "\"base\""));
            put(&over, path, text);
            order.push("over".to_owned());
        }
        let stack = ModStack::open(&root, &order).unwrap();
        (root, stack)
    }
}
