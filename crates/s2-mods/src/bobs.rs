//! The settlers' BOB files: each file bakes to a folder under `bobs/` named after it, holding
//! `bodies/` and `overlays/` with one PNG a part named after its number, a TOML of rows beside
//! each, and `links.toml`, a row an item. An item's row holds `overlay` and `dy`, 96 numbers
//! each: for each frame, each body type and each direction, in that nesting, the overlay drawn
//! over the body and the rows it moves down. The PNGs follow the sprite archives' rules, with
//! every part a player-colour sprite.

use std::sync::Arc;

use s2_assets::{BobSprites, Sprite};
use s2_data::DataSet;
use s2_formats::bob::{BODY_PARTS, Bob, LINKS_PER_ITEM, Link};

use crate::sprites::{Baked, BakedArchive, Kind, Row, Sheets, row_error, row_of, sprites_in, view_palette};
use crate::{Error, ModStack, Result};

/// Where the BOB files go in a mod.
pub const BOBS: &str = "bobs";

/// The BOB files of the original data.
const FILES: [&str; 2] = [s2_assets::CARRIER_BOB, s2_assets::JOBS_BOB];

/// The folders in a BOB file's folder for its two kinds of part.
const BODIES: &str = "bodies";
const OVERLAYS: &str = "overlays";

/// The table of an item's links.
const LINKS: &str = "links.toml";

/// The mod folder the BOB file at `path` bakes to: its file name, lower case, without the
/// extension. `DATA/BOBS/JOBS.BOB` is `bobs/jobs`.
#[must_use]
pub fn bob_folder(path: &str) -> String {
    let name = path.rsplit_once('/').map_or(path, |(_, name)| name);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{BOBS}/{}", stem.to_ascii_lowercase())
}

/// One item's row in `links.toml`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemLinks {
    overlay: Vec<u16>,
    dy: Vec<i16>,
}

impl Sheets<'_> {
    /// The parts and links of the BOB file at data path `path`.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack has no such file, [`Error::Row`] for a table that does
    /// not hold every body or a whole item of links, the stack's errors for one that does not
    /// load, and [`Error::Data`] or [`Error::Bob`] for an original that cannot be read.
    pub fn bob(&self, path: &str) -> Result<BobSprites> {
        let folder = bob_folder(path);
        if let Some(stack) = self.baked(&format!("{folder}/{BODIES}"))? {
            return bob(stack, &folder);
        }
        let bytes = self.data.read(path)?;
        let parsed = Bob::parse(&bytes).map_err(|source| Error::Bob {
            path: path.into(),
            source,
        })?;
        Ok(BobSprites::from_bob(&parsed))
    }
}

/// The BOB file the stack holds in `folder`.
fn bob(stack: &ModStack, folder: &str) -> Result<BobSprites> {
    let bodies = numbered(stack, &format!("{folder}/{BODIES}"))?;
    let body_table = format!("{folder}/{BODIES}.toml");
    if bodies.len() != BODY_PARTS || bodies.iter().any(Option::is_none) {
        return Err(row_error(
            &body_table,
            "",
            format!("a BOB file has {BODY_PARTS} bodies"),
        ));
    }
    let bodies = bodies.into_iter().flatten().collect();
    let mut overlays = numbered(stack, &format!("{folder}/{OVERLAYS}"))?;
    let links = links(stack, folder)?;
    let named = links
        .iter()
        .map(|link| usize::from(link.overlay) + 1)
        .max()
        .unwrap_or(0);
    if overlays.len() < named {
        overlays.resize(named, None);
    }
    Ok(BobSprites::new(bodies, overlays.into(), links.into()))
}

/// The sprites the stack holds in `folder`, by the number each row's key gives.
fn numbered(stack: &ModStack, folder: &str) -> Result<Vec<Option<Sprite>>> {
    let table_path = format!("{folder}.toml");
    let mut rows = Vec::new();
    let mut numbers = Vec::new();
    for (key, value) in stack.table(&table_path)? {
        let number: usize = key
            .parse()
            .map_err(|_| row_error(&table_path, &key, "the key is not a part number"))?;
        let row = row_of(&table_path, &key, value)?;
        if row.kind != Kind::Player {
            return Err(row_error(&table_path, &key, "a part's kind is player"));
        }
        numbers.push(number);
        rows.push((key, row));
    }
    let mut parts: Vec<Option<Sprite>> = Vec::new();
    for (number, (_, sprite)) in numbers.into_iter().zip(sprites_in(stack, folder, rows)?) {
        if parts.len() <= number {
            parts.resize(number + 1, None);
        }
        parts[number] = Some(sprite);
    }
    Ok(parts)
}

/// Every item's links the stack holds for the BOB file in `folder`, item 0 first.
fn links(stack: &ModStack, folder: &str) -> Result<Vec<Link>> {
    let table_path = format!("{folder}/{LINKS}");
    let mut items: Vec<Option<Vec<Link>>> = Vec::new();
    for (key, value) in stack.table(&table_path)? {
        let item: usize = key
            .parse()
            .map_err(|_| row_error(&table_path, &key, "the key is not an item number"))?;
        let row: ItemLinks = value
            .try_into()
            .map_err(|error: toml::de::Error| row_error(&table_path, &key, error.message()))?;
        if row.overlay.len() != LINKS_PER_ITEM || row.dy.len() != LINKS_PER_ITEM {
            return Err(row_error(
                &table_path,
                &key,
                format!("an item has {LINKS_PER_ITEM} overlays and {LINKS_PER_ITEM} dy values"),
            ));
        }
        if items.len() <= item {
            items.resize(item + 1, None);
        }
        items[item] = Some(
            row.overlay
                .into_iter()
                .zip(row.dy)
                .map(|(overlay, dy)| Link { overlay, dy })
                .collect(),
        );
    }
    items
        .into_iter()
        .enumerate()
        .map(|(item, links)| {
            links.ok_or_else(|| {
                row_error(
                    &table_path,
                    &item.to_string(),
                    "the items run 0 upward with none left out",
                )
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(|items| items.into_iter().flatten().collect())
}

/// Every BOB file the data set holds, by data path.
pub(crate) fn bob_files(data: &DataSet) -> Vec<&'static str> {
    FILES.into_iter().filter(|path| data.exists(path)).collect()
}

/// The parts of the BOB file at `path` to bake, with their tables, and its links' table.
pub(crate) fn bake_bob(data: &DataSet, path: &str) -> Result<Vec<BakedArchive>> {
    let sprites = Sheets { stack: None, data }.bob(path)?;
    let palette = Arc::new(view_palette(data, path)?);
    let folder = bob_folder(path);
    let parts = |kind: &str, sprites: Vec<(usize, &Sprite)>| -> Result<BakedArchive> {
        let folder = format!("{folder}/{kind}");
        let mut rows = toml::Table::new();
        let mut items = Vec::with_capacity(sprites.len());
        for (number, sprite) in sprites {
            let origin = sprite.origin();
            let row = Row {
                kind: Kind::Player,
                origin: [origin.x, origin.y],
            };
            rows.insert(number.to_string(), value(&folder, &row)?);
            items.push(Baked::new(
                format!("{folder}/{number}.png"),
                sprite.clone(),
                Kind::Player,
                Arc::clone(&palette),
            ));
        }
        Ok(BakedArchive {
            items,
            table: (format!("{folder}.toml"), table(&folder, &rows)?),
        })
    };
    let bodies = parts(BODIES, sprites.bodies().iter().enumerate().collect())?;
    let overlays = parts(
        OVERLAYS,
        sprites
            .overlays()
            .iter()
            .enumerate()
            .filter_map(|(number, overlay)| Some((number, overlay.as_ref()?)))
            .collect(),
    )?;
    let mut rows = toml::Table::new();
    for (item, links) in sprites.links().chunks(LINKS_PER_ITEM).enumerate() {
        let row = ItemLinks {
            overlay: links.iter().map(|link| link.overlay).collect(),
            dy: links.iter().map(|link| link.dy).collect(),
        };
        rows.insert(item.to_string(), value(&folder, &row)?);
    }
    let links = BakedArchive {
        items: Vec::new(),
        table: (format!("{folder}/{LINKS}"), table(&folder, &rows)?),
    };
    Ok(vec![bodies, overlays, links])
}

/// `row` as a TOML value of a table in `folder`.
fn value(folder: &str, row: &impl serde::Serialize) -> Result<toml::Value> {
    toml::Value::try_from(row).map_err(|source| Error::Write {
        path: folder.into(),
        source,
    })
}

/// `rows` as the bytes of a TOML in `folder`.
fn table(folder: &str, rows: &toml::Table) -> Result<Vec<u8>> {
    toml::to_string(rows)
        .map(String::into_bytes)
        .map_err(|source| Error::Write {
            path: folder.into(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bob_file_bakes_to_a_folder_named_after_it() {
        assert_eq!(bob_folder("DATA/BOBS/JOBS.BOB"), "bobs/jobs");
        assert_eq!(bob_folder("DATA/BOBS/CARRIER.BOB"), "bobs/carrier");
    }
}
