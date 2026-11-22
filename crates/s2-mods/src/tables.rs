//! The tables the renderer and the interface draw and sound by, as TOML files under `tables/`.
//! None of them changes the game's state, so none is part of a replay's tables hash.
//!
//! | File | Holds |
//! |---|---|
//! | `tables/terrain/types.toml` | a row a terrain type, by number |
//! | `tables/terrain/textures.toml` | a row a texture entry, by number |
//! | `tables/terrain/borders.toml` | each landscape's border table |
//! | `tables/buildings/<nation>.toml` | a row a building type: its door and what it draws over itself |
//! | `tables/yards.toml` | a row a nation: where its yards put their animals |
//! | `tables/objects.toml` | where a flag holds its wares and a ship hangs its pennant |
//! | `tables/messages.toml` | a row a message id, by number |
//! | `tables/styles.toml` | a row a window style, by number |
//! | `tables/pictures.toml` | the interface item each picture draws, by the picture's name |

use serde::{Deserialize, Serialize};

use s2_assets::{Landscape, Nation};
use s2_render::terrain::{
    BORDER_CLASSES, BorderTable, Point, TERRAIN_SETS, TerrainTables, TextureEntry, TypeInfo,
};
use s2_render::{Door, Doors, Extra, Extras, FlagSlots, ObjectTables, Offset, ShipFlags, Yard};
use s2_sim::{BuildingKind, FLAG_CAPACITY, MESSAGE_IDS, MessageId, MessageType, MessageTypes};
use s2_ui::picture;
use s2_ui::style::COUNT as STYLE_COUNT;
use s2_ui::{FontId, Style, Styles};

use crate::rows::{by_name, numbered, rows, value, whole, write};
use crate::sprites::{Sheets, row_error};
use crate::{ModStack, Result};

/// The table files.
const TYPES: &str = "tables/terrain/types.toml";
const TEXTURES: &str = "tables/terrain/textures.toml";
const BORDERS: &str = "tables/terrain/borders.toml";
const YARDS: &str = "tables/yards.toml";
const OBJECTS: &str = "tables/objects.toml";
const MESSAGES: &str = "tables/messages.toml";
const STYLES: &str = "tables/styles.toml";
const PICTURES: &str = "tables/pictures.toml";

/// The fonts' names.
const FONTS: [(&str, FontId); 3] = [
    ("small", FontId::Small),
    ("medium", FontId::Medium),
    ("large", FontId::Large),
];

/// How a border table row marks a neighbour class that gets a border, and one that does not.
const BORDER: char = 'x';
const NO_BORDER: char = '.';

/// The directions a ship can face.
const DIRECTIONS: usize = 6;

/// Points of a texture entry: three corners of each of its two triangles.
const CORNERS: usize = 6;

/// A pixel offset, rightwards then downwards.
type Pair = [i32; 2];

/// One terrain type's row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypeRow {
    class: u8,
    texture: u8,
    strip: [u8; TERRAIN_SETS],
}

/// One texture entry's row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureRow {
    uv: [[u16; 2]; CORNERS],
    flat: bool,
    colour: u8,
}

/// One building type's row for one nation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingRow {
    door: Pair,
    flags: Pair,
    stopped: Pair,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work: Option<Pair>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smoke: Option<Pair>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    banner: Option<Pair>,
}

/// One nation's yards.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct YardRow {
    pigs: [Pair; 5],
    donkeys: [Pair; 3],
    harbour: [Pair; 3],
}

/// The objects file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectsFile {
    flag_slots: [Pair; FLAG_CAPACITY as usize],
    ship_pennants: [Pair; DIRECTIONS],
}

/// One message id's row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageRow {
    category: u8,
    effect: u16,
}

/// One window style's row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleRow {
    background: String,
    alternate: String,
    frame: String,
    font: String,
    colour: u8,
    other_colour: u8,
}

/// Where a nation's building rows go.
fn buildings_path(nation: Nation) -> String {
    format!("tables/buildings/{nation}.toml")
}

impl Sheets<'_> {
    /// The tables the terrain is drawn by.
    ///
    /// # Errors
    ///
    /// [`crate::Error::NotInMods`] when the stack lacks a table, [`crate::Error::Row`] for a row
    /// that does not decode or tables that do not fit together, and the stack's errors for a
    /// table that does not load.
    pub fn terrain_tables(&self) -> Result<TerrainTables> {
        self.stack.map_or(Ok(TerrainTables::ORIGINAL), terrain)
    }

    /// The tables the object pass draws with.
    ///
    /// # Errors
    ///
    /// As [`Sheets::terrain_tables`].
    pub fn object_tables(&self) -> Result<ObjectTables> {
        self.stack.map_or(Ok(ObjectTables::ORIGINAL), objects)
    }

    /// The list each kind of message goes on and the effect it plays.
    ///
    /// # Errors
    ///
    /// As [`Sheets::terrain_tables`].
    pub fn message_types(&self) -> Result<MessageTypes> {
        self.stack.map_or(Ok(MessageTypes::ORIGINAL), messages)
    }

    /// The window styles.
    ///
    /// # Errors
    ///
    /// As [`Sheets::terrain_tables`].
    pub fn styles(&self) -> Result<Styles> {
        self.stack.map_or(Ok(Styles::ORIGINAL), styles)
    }

    /// The interface item each picture draws, by [`s2_ui::Picture::index`].
    ///
    /// # Errors
    ///
    /// As [`Sheets::terrain_tables`].
    pub fn pictures(&self) -> Result<Vec<String>> {
        let Some(stack) = self.stack else {
            return Ok(picture::ORIGINAL.iter().map(|&name| name.to_owned()).collect());
        };
        by_name(PICTURES, "", &picture::NAMES, rows::<String>(stack, PICTURES)?)
    }
}

/// The files the original tables bake to, by path inside the mod.
pub(crate) fn table_files() -> Result<Vec<(String, String)>> {
    let terrain = TerrainTables::ORIGINAL;
    let objects = ObjectTables::ORIGINAL;
    let mut files = vec![
        (TYPES.to_owned(), write(TYPES, &types_table(&terrain)?)?),
        (TEXTURES.to_owned(), write(TEXTURES, &textures_table(&terrain)?)?),
        (
            BORDERS.to_owned(),
            write(BORDERS, &borders_table(terrain.borders()))?,
        ),
        (YARDS.to_owned(), write(YARDS, &yards_table(&objects.yards)?)?),
        (OBJECTS.to_owned(), write(OBJECTS, &objects_file(&objects))?),
        (
            MESSAGES.to_owned(),
            write(MESSAGES, &messages_table(&MessageTypes::ORIGINAL)?)?,
        ),
        (
            STYLES.to_owned(),
            write(STYLES, &styles_table(&Styles::ORIGINAL)?)?,
        ),
        (PICTURES.to_owned(), write(PICTURES, &pictures_table())?),
    ];
    for (index, nation) in Nation::ALL.into_iter().enumerate() {
        let path = buildings_path(nation);
        let table = buildings_table(&path, &objects.doors[index], &objects.extras[index])?;
        let text = write(&path, &table)?;
        files.push((path, text));
    }
    Ok(files)
}

/// `pair` as an offset, refused for row `key` of the file at `path` when it does not fit.
fn offset(path: &str, key: &str, [x, y]: Pair) -> Result<Offset> {
    let fit =
        |value: i32| i16::try_from(value).map_err(|_| row_error(path, key, "an offset is -32768 to 32767"));
    Ok(Offset {
        x: fit(x)?,
        y: fit(y)?,
    })
}

/// `offset` as a pair.
fn offset_pair(offset: Offset) -> Pair {
    [offset.x.into(), offset.y.into()]
}

/// `point` as a pair.
const fn point_pair(point: Point) -> Pair {
    [point.x, point.y]
}

/// `pair` as a point.
const fn point([x, y]: Pair) -> Point {
    Point::new(x, y)
}

/// `rows` keyed by their numbers from 0.
fn numbered_table<T: Serialize>(path: &str, rows: impl IntoIterator<Item = T>) -> Result<toml::Table> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| Ok((index.to_string(), value(path, row)?)))
        .collect()
}

fn types_table(tables: &TerrainTables) -> Result<toml::Table> {
    numbered_table(
        TYPES,
        tables.types().iter().map(|info| TypeRow {
            class: info.class,
            texture: info.texture,
            strip: info.strip,
        }),
    )
}

fn textures_table(tables: &TerrainTables) -> Result<toml::Table> {
    numbered_table(
        TEXTURES,
        tables.textures().iter().map(|entry| TextureRow {
            uv: entry.uv.map(|(u, v)| [u, v]),
            flat: entry.flat,
            colour: entry.colour,
        }),
    )
}

fn borders_table(borders: &BorderTable) -> toml::Table {
    Landscape::ALL
        .into_iter()
        .map(|landscape| {
            let rows = borders[landscape.set()]
                .iter()
                .map(|row| {
                    let text = row.iter().map(|&drawn| if drawn { BORDER } else { NO_BORDER });
                    toml::Value::String(text.collect())
                })
                .collect();
            (landscape.to_string(), toml::Value::Array(rows))
        })
        .collect()
}

fn terrain(stack: &ModStack) -> Result<TerrainTables> {
    let types = numbered(TYPES, rows::<TypeRow>(stack, TYPES)?)?
        .into_iter()
        .map(|row| TypeInfo::new(row.class, row.texture, row.strip))
        .collect();
    let textures = numbered(TEXTURES, rows::<TextureRow>(stack, TEXTURES)?)?
        .into_iter()
        .map(|row| TextureEntry::new(row.uv.map(|[u, v]| (u, v)), row.flat, row.colour))
        .collect();
    let names: Vec<String> = Landscape::ALL.iter().map(ToString::to_string).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let sets = by_name(BORDERS, "", &names, rows::<Vec<String>>(stack, BORDERS)?)?;
    let mut borders = [[[false; BORDER_CLASSES]; BORDER_CLASSES]; TERRAIN_SETS];
    for ((landscape, rows), table) in Landscape::ALL.into_iter().zip(sets).zip(&mut borders) {
        let key = landscape.to_string();
        let shape = || {
            row_error(
                BORDERS,
                &key,
                format!("{BORDER_CLASSES} rows of {BORDER_CLASSES} x or ."),
            )
        };
        if rows.len() != BORDER_CLASSES {
            return Err(shape());
        }
        for (text, row) in rows.iter().zip(table.iter_mut()) {
            if text.chars().count() != BORDER_CLASSES {
                return Err(shape());
            }
            for (mark, drawn) in text.chars().zip(row.iter_mut()) {
                *drawn = match mark {
                    BORDER => true,
                    NO_BORDER => false,
                    _ => return Err(shape()),
                };
            }
        }
    }
    TerrainTables::new(types, textures, borders).map_err(|error| row_error(TYPES, "", error.to_string()))
}

fn buildings_table(path: &str, doors: &Doors, extras: &Extras) -> Result<toml::Table> {
    BuildingKind::NAMES
        .iter()
        .zip(0..)
        .map(|(&name, kind)| {
            let kind = BuildingKind(kind);
            let door = doors.get(kind);
            let extra = extras.get(kind);
            let row = BuildingRow {
                door: [door.x.into(), door.y.into()],
                flags: offset_pair(extra.flags),
                stopped: offset_pair(extra.stopped),
                work: extra.work.map(offset_pair),
                smoke: extra.smoke.map(offset_pair),
                banner: extra.banner.map(offset_pair),
            };
            Ok((name.to_owned(), value(path, row)?))
        })
        .collect()
}

/// One nation's doors and extras from its building rows.
fn nation_buildings(stack: &ModStack, nation: Nation) -> Result<(Doors, Extras)> {
    let path = buildings_path(nation);
    let rows = rows::<BuildingRow>(stack, &path)?;
    let rows = by_name(&path, "", &BuildingKind::NAMES, rows)?;
    let mut doors = Vec::new();
    let mut extras = Vec::new();
    for (row, name) in rows.into_iter().zip(BuildingKind::NAMES) {
        let door = offset(&path, name, row.door)?;
        doors.push(Door { x: door.x, y: door.y });
        let optional = |pair: Option<Pair>| pair.map(|pair| offset(&path, name, pair)).transpose();
        extras.push(Extra {
            work: optional(row.work)?,
            flags: offset(&path, name, row.flags)?,
            smoke: optional(row.smoke)?,
            stopped: offset(&path, name, row.stopped)?,
            banner: optional(row.banner)?,
        });
    }
    let doors = Doors::new(doors).map_err(|error| row_error(&path, "", error.to_string()))?;
    let extras = Extras::new(extras).map_err(|error| row_error(&path, "", error.to_string()))?;
    Ok((doors, extras))
}

fn yards_table(yards: &[Yard]) -> Result<toml::Table> {
    Nation::ALL
        .into_iter()
        .zip(yards)
        .map(|(nation, yard)| {
            let row = YardRow {
                pigs: yard.pigs.map(offset_pair),
                donkeys: yard.donkeys.map(offset_pair),
                harbour: yard.harbour.map(offset_pair),
            };
            Ok((nation.to_string(), value(YARDS, row)?))
        })
        .collect()
}

fn objects_file(objects: &ObjectTables) -> ObjectsFile {
    ObjectsFile {
        flag_slots: std::array::from_fn(|slot| {
            point_pair(objects.flag_slots.get(u8::try_from(slot).unwrap_or(u8::MAX)))
        }),
        ship_pennants: std::array::from_fn(|direction| point_pair(objects.ship_flags.get(direction))),
    }
}

fn objects(stack: &ModStack) -> Result<ObjectTables> {
    let mut tables = ObjectTables::ORIGINAL;
    for (index, nation) in Nation::ALL.into_iter().enumerate() {
        (tables.doors[index], tables.extras[index]) = nation_buildings(stack, nation)?;
    }
    let names: Vec<String> = Nation::ALL.iter().map(ToString::to_string).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let yards = by_name(YARDS, "", &names, rows::<YardRow>(stack, YARDS)?)?;
    for ((yard, row), name) in tables.yards.iter_mut().zip(yards).zip(&names) {
        let offsets = |pairs: &[Pair]| -> Result<Vec<Offset>> {
            pairs.iter().map(|&pair| offset(YARDS, name, pair)).collect()
        };
        let fill = |into: &mut [Offset], pairs: &[Pair]| -> Result<()> {
            into.copy_from_slice(&offsets(pairs)?);
            Ok(())
        };
        fill(&mut yard.pigs, &row.pigs)?;
        fill(&mut yard.donkeys, &row.donkeys)?;
        fill(&mut yard.harbour, &row.harbour)?;
    }
    let file: ObjectsFile = whole(stack, OBJECTS)?;
    tables.flag_slots = FlagSlots::new(file.flag_slots.map(point));
    tables.ship_flags = ShipFlags::new(file.ship_pennants.map(point));
    Ok(tables)
}

fn messages_table(types: &MessageTypes) -> Result<toml::Table> {
    numbered_table(
        MESSAGES,
        (0..MESSAGE_IDS).map(|id| {
            let row = types.get(MessageId(u8::try_from(id).unwrap_or(u8::MAX)));
            MessageRow {
                category: row.category,
                effect: row.effect,
            }
        }),
    )
}

fn messages(stack: &ModStack) -> Result<MessageTypes> {
    let rows = numbered(MESSAGES, rows::<MessageRow>(stack, MESSAGES)?)?;
    if rows.len() != MESSAGE_IDS {
        return Err(row_error(
            MESSAGES,
            "",
            format!("a row for each id from 0 to {}", MESSAGE_IDS - 1),
        ));
    }
    let blank = MessageType {
        category: 0,
        effect: 0,
    };
    let mut types = [blank; MESSAGE_IDS];
    for (slot, row) in types.iter_mut().zip(rows) {
        *slot = MessageType {
            category: row.category,
            effect: row.effect,
        };
    }
    Ok(MessageTypes::new(types))
}

fn styles_table(styles: &Styles) -> Result<toml::Table> {
    numbered_table(
        STYLES,
        styles.all().iter().map(|style| StyleRow {
            background: style.background.to_string(),
            alternate: style.alternate.to_string(),
            frame: style.frame.to_string(),
            font: FONTS
                .iter()
                .find(|(_, font)| *font == style.font)
                .map_or_else(String::new, |(name, _)| (*name).to_owned()),
            colour: style.colour,
            other_colour: style.other_colour,
        }),
    )
}

fn styles(stack: &ModStack) -> Result<Styles> {
    let rows = numbered(STYLES, rows::<StyleRow>(stack, STYLES)?)?;
    let count = rows.len();
    let styles = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let font = FONTS
                .iter()
                .find(|(name, _)| *name == row.font)
                .map(|&(_, font)| font)
                .ok_or_else(|| row_error(STYLES, &index.to_string(), format!("no font {}", row.font)))?;
            Ok(Style {
                background: row.background.into(),
                alternate: row.alternate.into(),
                frame: row.frame.into(),
                font,
                colour: row.colour,
                other_colour: row.other_colour,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let styles: [Style; STYLE_COUNT] = styles.try_into().map_err(|_| {
        row_error(
            STYLES,
            "",
            format!("{STYLE_COUNT} styles, numbered from 0, not {count}"),
        )
    })?;
    Ok(Styles::new(styles))
}

fn pictures_table() -> toml::Table {
    picture::NAMES
        .iter()
        .zip(picture::ORIGINAL)
        .map(|(&name, item)| (name.to_owned(), toml::Value::String(item.to_owned())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::rows::testing::stack;

    #[test]
    fn the_original_tables_come_back_from_their_files() {
        let (root, stack) = stack("looks", &table_files().unwrap(), None);
        assert_eq!(terrain(&stack).unwrap(), TerrainTables::ORIGINAL);
        assert_eq!(objects(&stack).unwrap(), ObjectTables::ORIGINAL);
        assert_eq!(messages(&stack).unwrap(), MessageTypes::ORIGINAL);
        assert_eq!(styles(&stack).unwrap(), Styles::ORIGINAL);
        let sheets_pictures = by_name(
            PICTURES,
            "",
            &picture::NAMES,
            rows::<String>(&stack, PICTURES).unwrap(),
        )
        .unwrap();
        assert_eq!(sheets_pictures, picture::ORIGINAL.map(str::to_owned).to_vec());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_mod_moves_one_chimney() {
        let path = buildings_path(Nation::Vikings);
        let (root, stack) = stack(
            "chimney",
            &table_files().unwrap(),
            Some((&path, "[bakery]\nsmoke = [1, 2]\n")),
        );
        let tables = objects(&stack).unwrap();
        let vikings = Nation::Vikings.index();
        let bakery = tables.extras[vikings].get(BuildingKind::BAKERY);
        assert_eq!(bakery.smoke, Some(Offset { x: 1, y: 2 }));
        assert_eq!(
            bakery.stopped,
            ObjectTables::ORIGINAL.extras[vikings]
                .get(BuildingKind::BAKERY)
                .stopped
        );
        assert_eq!(
            tables.extras[0],
            ObjectTables::ORIGINAL.extras[0],
            "the other nations keep theirs"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn broken_rows_are_refused() {
        let refused = |name: &str, path: &str, text: &str| {
            let (root, stack) = stack(name, &table_files().unwrap(), Some((path, text)));
            let error = terrain(&stack)
                .and(objects(&stack).map(|_| ()))
                .and(messages(&stack).map(|_| ()));
            std::fs::remove_dir_all(root).unwrap();
            error.unwrap_err()
        };
        assert!(matches!(
            refused("border", BORDERS, "greenland = [\"xx\"]\n"),
            Error::Row { row, .. } if row == "greenland"
        ));
        assert!(matches!(
            refused("class", TYPES, "[0]\nclass = 99\n"),
            Error::Row { .. }
        ));
        assert!(matches!(
            refused("message", MESSAGES, "[30]\ncategory = 1\neffect = 1\n"),
            Error::Row { row, .. } if row == "30"
        ));
        assert!(matches!(
            refused("yard", YARDS, "[romans]\npigs = [[1, 2]]\n"),
            Error::Row { row, .. } if row == "romans"
        ));
    }
}
