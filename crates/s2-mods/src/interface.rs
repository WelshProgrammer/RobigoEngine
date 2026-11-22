//! The interface's named items and its fonts (`docs/design/mods.md`, "What each original file
//! becomes").
//!
//! Each `.IDX`/`.DAT` pair bakes to a folder under `interface/` named after the pair, holding one
//! PNG a picture named after its entry, and a TOML beside it with a row a picture keyed by that
//! name, in the index's order. Each font in `DATA/RESOURCE` bakes to a folder under `fonts/`,
//! holding one PNG a glyph named after its character code, and a TOML beside it with the font's
//! `line_height` and a row a glyph. The PNGs follow the sprite archives' rules.

use std::sync::Arc;

use s2_assets::Sprite;
use s2_data::DataSet;
use s2_formats::idx::Index;
use s2_formats::lst::Item;

use crate::sprites::{
    Baked, BakedArchive, Kind, Row, Sheets, kind_of, row_error, row_of, sprites_in, view_palette,
};
use crate::{Error, ModStack, Result};

/// Where the named interface items go in a mod.
pub const INTERFACE: &str = "interface";

/// Where the fonts go in a mod.
pub const FONTS: &str = "fonts";

/// The named pairs of the original data, by data path without the extension.
const PAIRS: [&str; 4] = ["DATA/IO/IO", "DATA/RESOURCE", "DATA/IO/EDITIO", "DATA/EDITRES"];

/// The pair the game's fonts come from.
const FONT_PAIR: &str = "DATA/RESOURCE";

/// The key in a font's TOML that holds the distance from one line of text to the next.
const LINE_HEIGHT: &str = "line_height";

/// The mod folder a pair bakes to: its file name, lower case. `DATA/IO/IO` is `interface/io`.
#[must_use]
pub fn interface_folder(stem: &str) -> String {
    let name = stem.rsplit_once('/').map_or(stem, |(_, name)| name);
    format!("{INTERFACE}/{}", name.to_ascii_lowercase())
}

/// The mod folder the font `name` bakes to.
#[must_use]
pub fn font_folder(name: &str) -> String {
    format!("{FONTS}/{name}")
}

/// A font's glyphs, by character code, and how far apart its lines are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontGlyphs {
    /// Pixels from one line's top to the next's.
    pub line_height: u8,
    /// Each glyph the font draws, by character code, lowest first.
    pub glyphs: Vec<(u8, Sprite)>,
}

impl Sheets<'_> {
    /// The pictures of the pair at `stem`, such as `DATA/IO/IO`, by entry name in the index's
    /// order, with any a mod adds after them. An entry that is not a picture, or whose name is
    /// not plain ASCII, is left out.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack has no such pair, the stack's errors for one that does
    /// not load, and [`Error::Data`] or [`Error::Index`] for an original pair that cannot be read.
    pub fn named(&self, stem: &str) -> Result<Vec<(String, Sprite)>> {
        let folder = interface_folder(stem);
        if let Some(stack) = self.baked(&folder)? {
            return named(stack, &folder);
        }
        Ok(pictures(&index(self.data, stem)?)
            .map(|(name, sprite, _)| (name.to_owned(), sprite))
            .collect())
    }

    /// The font `name` of `DATA/RESOURCE`, such as `font09`.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack has no such font, the stack's errors for one that does
    /// not load, [`Error::Data`] or
    /// [`Error::Index`] for an original pair that cannot be read, and [`Error::NoItem`] when the
    /// pair holds no such font.
    pub fn font(&self, name: &str) -> Result<FontGlyphs> {
        let folder = font_folder(name);
        if let Some(stack) = self.baked(&folder)? {
            return font(stack, &folder);
        }
        fonts(&index(self.data, FONT_PAIR)?)
            .find(|(found, _)| *found == name)
            .map(|(_, glyphs)| glyphs)
            .ok_or_else(|| Error::NoItem {
                path: FONT_PAIR.into(),
                name: name.into(),
            })
    }
}

/// The pictures the stack holds in `folder`, in its table's order.
fn named(stack: &ModStack, folder: &str) -> Result<Vec<(String, Sprite)>> {
    let table_path = format!("{folder}.toml");
    let rows = stack
        .table(&table_path)?
        .into_iter()
        .map(|(key, value)| Ok((key.clone(), row_of(&table_path, &key, value)?)))
        .collect::<Result<Vec<_>>>()?;
    sprites_in(stack, folder, rows)
}

/// The font the stack holds in `folder`.
fn font(stack: &ModStack, folder: &str) -> Result<FontGlyphs> {
    let table_path = format!("{folder}.toml");
    let mut table = stack.table(&table_path)?;
    let line_height = table
        .remove(LINE_HEIGHT)
        .and_then(|value| value.as_integer())
        .and_then(|height| u8::try_from(height).ok())
        .ok_or_else(|| row_error(&table_path, LINE_HEIGHT, "not a height from 0 to 255"))?;
    let mut rows = Vec::with_capacity(table.len());
    let mut codes = Vec::with_capacity(table.len());
    for (key, value) in table {
        let code: u8 = key
            .parse()
            .map_err(|_| row_error(&table_path, &key, "the key is not a character code"))?;
        let row = row_of(&table_path, &key, value)?;
        if row.kind != Kind::Player {
            return Err(row_error(&table_path, &key, "a glyph's kind is player"));
        }
        codes.push(code);
        rows.push((key, row));
    }
    let mut glyphs: Vec<(u8, Sprite)> = codes
        .into_iter()
        .zip(sprites_in(stack, folder, rows)?)
        .map(|(code, (_, sprite))| (code, sprite))
        .collect();
    glyphs.sort_by_key(|&(code, _)| code);
    Ok(FontGlyphs { line_height, glyphs })
}

/// Every named pair the data set holds, by data path without the extension.
pub(crate) fn pairs(data: &DataSet) -> Vec<&'static str> {
    PAIRS
        .into_iter()
        .filter(|stem| data.exists(&format!("{stem}.IDX")) && data.exists(&format!("{stem}.DAT")))
        .collect()
}

/// The pictures of the pair at `stem` whose names `keep` takes, to bake, and their TOML, or
/// `None` when it takes none.
pub(crate) fn bake_pair(
    data: &DataSet,
    stem: &str,
    keep: impl Fn(&str) -> bool,
) -> Result<Option<BakedArchive>> {
    let index = index(data, stem)?;
    let palette = Arc::new(view_palette(data, &format!("{stem}.IDX"))?);
    let folder = interface_folder(stem);
    let mut rows = toml::Table::new();
    let mut items = Vec::new();
    for (name, sprite, kind) in pictures(&index).filter(|(name, _, _)| keep(name)) {
        let origin = sprite.origin();
        let row = Row {
            kind,
            origin: [origin.x, origin.y],
        };
        rows.insert(name.to_owned(), value(&folder, row)?);
        items.push(Baked::new(
            format!("{folder}/{name}.png"),
            sprite,
            kind,
            Arc::clone(&palette),
        ));
    }
    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(BakedArchive {
        items,
        table: (format!("{folder}.toml"), table(&folder, &rows)?),
    }))
}

/// Each font of `DATA/RESOURCE` to bake, and its TOML.
pub(crate) fn bake_fonts(data: &DataSet) -> Result<Vec<BakedArchive>> {
    if !pairs(data).contains(&FONT_PAIR) {
        return Ok(Vec::new());
    }
    let palette = Arc::new(view_palette(data, &format!("{FONT_PAIR}.IDX"))?);
    fonts(&index(data, FONT_PAIR)?)
        .map(|(name, font)| {
            let folder = font_folder(name);
            let mut rows = toml::Table::new();
            rows.insert(LINE_HEIGHT.into(), toml::Value::Integer(font.line_height.into()));
            let mut items = Vec::with_capacity(font.glyphs.len());
            for (code, sprite) in font.glyphs {
                let origin = sprite.origin();
                let row = Row {
                    kind: Kind::Player,
                    origin: [origin.x, origin.y],
                };
                rows.insert(code.to_string(), value(&folder, row)?);
                items.push(Baked::new(
                    format!("{folder}/{code}.png"),
                    sprite,
                    Kind::Player,
                    Arc::clone(&palette),
                ));
            }
            Ok(BakedArchive {
                items,
                table: (format!("{folder}.toml"), table(&folder, &rows)?),
            })
        })
        .collect()
}

/// The pair at `stem`, decoded.
fn index(data: &DataSet, stem: &str) -> Result<Index> {
    let idx = data.read(&format!("{stem}.IDX"))?;
    let dat = data.read(&format!("{stem}.DAT"))?;
    Index::parse(&idx, &dat).map_err(|source| Error::Index {
        path: stem.into(),
        source,
    })
}

/// Each picture of `index` with a plain ASCII name, in the index's order.
fn pictures(index: &Index) -> impl Iterator<Item = (&str, Sprite, Kind)> {
    index.entries().iter().filter_map(|entry| {
        let name = entry.name()?;
        let kind = kind_of(entry.item())?;
        Some((name, Sprite::from_item(entry.item())?, kind))
    })
}

/// Each font of `index` with a plain ASCII name, in the index's order.
fn fonts(index: &Index) -> impl Iterator<Item = (&str, FontGlyphs)> {
    index.entries().iter().filter_map(|entry| {
        let Item::Font(font) = entry.item() else {
            return None;
        };
        let glyphs = font
            .glyphs()
            .map(|(code, glyph)| (code, Sprite::from_player_bitmap(glyph)))
            .collect();
        Some((
            entry.name()?,
            FontGlyphs {
                line_height: font.dy(),
                glyphs,
            },
        ))
    })
}

/// `row` as a TOML value of the table in `folder`.
fn value(folder: &str, row: Row) -> Result<toml::Value> {
    toml::Value::try_from(row).map_err(|source| Error::Write {
        path: folder.into(),
        source,
    })
}

/// `rows` as the bytes of the TOML beside `folder`.
fn table(folder: &str, rows: &toml::Table) -> Result<Vec<u8>> {
    toml::to_string(rows)
        .map(String::into_bytes)
        .map_err(|source| Error::Write {
            path: format!("{folder}.toml"),
            source,
        })
}
