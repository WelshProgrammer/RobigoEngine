//! The sprite archives: every drawable item of an original `.LST` as one indexed PNG, and each
//! archive's origins and kinds as one TOML (`docs/design/mods.md`, "What each original file
//! becomes").
//!
//! A PNG carries the game's palette, so it shows the item's own colours in any image editor,
//! and its pixel indices are the palette indices the game draws. One index the item does not use
//! is marked transparent. A player-colour item draws its shades as indices 128 to 131, player
//! 0's ramp, which the game then moves to the owner's ramp; a shadow item is a mask.

use std::sync::Arc;

use rayon::prelude::*;
use s2_assets::{Ink, Sprite, SpriteSheet};
use s2_data::DataSet;
use s2_formats::Palette;
use s2_formats::lst::{Archive, Item, Origin};

use crate::{Error, ModStack, Result};

/// Where sprites go in a mod.
pub const SPRITES: &str = "sprites";

/// The folders of the original data that hold sprite archives.
const FOLDERS: [&str; 3] = ["DATA", "DATA/MBOB", "DATA/CBOB"];

/// Archives in those folders that hold no sprites.
const NOT_SPRITES: [&str; 1] = ["SOUND.LST"];

/// The first index of player 0's colour ramp, where a player-colour item's shades are drawn.
pub const PLAYER_RAMP: u8 = 128;

/// How many shades a player's ramp holds.
pub const PLAYER_SHADES: u8 = 4;

/// The palettes a sprite's PNG carries for an editor to show it with. The game draws with the
/// landscape's own palette whatever the PNG carries.
const GREENLAND_PALETTE: &str = "GFX/PALETTE/PAL5.BBM";
const WASTELAND_PALETTE: &str = "GFX/PALETTE/PAL6.BBM";
const WINTER_PALETTE: &str = "GFX/PALETTE/PAL7.BBM";

/// The archive of the credits' portraits, and the palette six of the base game's were drawn
/// for: its indices 192 to 223 are one ramp of 32 greys. The credits show in the greenland
/// palette, whose same indices are two ramps of brown, so Settlers 2 shows those six in
/// blotches of sepia. The Gold Edition's archive holds them drawn again in the greenland
/// palette's greys.
const PORTRAITS: &str = "DATA/CREDITS.LST";
const GREY_PALETTE: &str = "GFX/PALETTE/PALETTI8.BBM";
const GREY_RAMP: std::ops::RangeInclusive<u8> = 192..=223;

/// Full opacity and none, in a PNG's transparency chunk.
const OPAQUE: u8 = u8::MAX;
const CLEAR: u8 = 0;

/// How a sprite's pixels are drawn, as its archive's TOML names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Palette indices.
    Colour,
    /// Palette indices, with indices 128 to 131 the owner's colour shades.
    Player,
    /// A mask of the pixels the shadow table darkens.
    Shadow,
}

/// One row of an archive's TOML: where the item's origin is and how it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    /// How it draws.
    pub kind: Kind,
    /// The point the game places at the item's position, from its top-left corner.
    pub origin: [i16; 2],
}

/// The mod folder an original archive bakes to: its data path below `DATA`, lower case, without
/// the extension. `DATA/MBOB/ROM_Z.LST` is `sprites/mbob/rom_z`.
#[must_use]
pub fn sprite_folder(path: &str) -> String {
    let below = path.strip_prefix("DATA/").unwrap_or(path);
    let stem = below.rsplit_once('.').map_or(below, |(stem, _)| stem);
    format!("{SPRITES}/{}", stem.to_ascii_lowercase())
}

/// Where a sheet comes from: the mod stack when the game has one, otherwise the original files
/// in the data set. With a stack, a file no loaded mod holds is missing even when the data has it,
/// so leaving a mod out takes its files away.
#[derive(Debug, Clone, Copy)]
pub struct Sheets<'a> {
    /// The mods, when the game has them.
    pub stack: Option<&'a ModStack>,
    /// The player's data.
    pub data: &'a DataSet,
}

impl Sheets<'_> {
    /// The sprites of the archive at data path `path`.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack has no such archive, the stack's errors for one that
    /// does not load, and [`Error::Data`] or [`Error::Archive`] for an original that cannot be
    /// read.
    pub fn sheet(&self, path: &str) -> Result<SpriteSheet> {
        if let Some(stack) = self.baked(&sprite_folder(path))? {
            return sheet(stack, &sprite_folder(path));
        }
        let bytes = self.data.read(path)?;
        let archive = Archive::parse(&bytes).map_err(|source| Error::Archive {
            path: path.into(),
            source,
        })?;
        Ok(SpriteSheet::from_archive(&archive))
    }

    /// Whether the archive at `path` is there: in the mods when the game has them, otherwise in
    /// the data.
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.stack.map_or_else(
            || self.data.exists(path),
            |stack| stack.file(&format!("{}.toml", sprite_folder(path))).is_some(),
        )
    }

    /// The stack to read `folder` from, `None` when the game reads the original files.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the game has a stack and no mod in it holds `folder`.
    pub(crate) fn baked(&self, folder: &str) -> Result<Option<&ModStack>> {
        let Some(stack) = self.stack else {
            return Ok(None);
        };
        let table = format!("{folder}.toml");
        if stack.file(&table).is_none() {
            return Err(Error::NotInMods(table));
        }
        Ok(Some(stack))
    }
}

/// The sheet the stack holds in `folder`, its rows merged over every mod and each item's PNG
/// from the last mod that has it.
///
/// # Errors
///
/// [`Error::Row`] for a row with no PNG or a PNG that is not indexed, and the stack's errors.
pub fn sheet(stack: &ModStack, folder: &str) -> Result<SpriteSheet> {
    let table_path = format!("{folder}.toml");
    let rows = stack
        .table(&table_path)?
        .into_iter()
        .map(|(key, value)| {
            let slot: usize = key
                .parse()
                .map_err(|_| row_error(&table_path, &key, "the key is not an item number"))?;
            let row = row_of(&table_path, &key, value)?;
            Ok((slot, (key, row)))
        })
        .collect::<Result<Vec<_>>>()?;
    let (slots, rows): (Vec<usize>, Vec<_>) = rows.into_iter().unzip();
    let sprites = sprites_in(stack, folder, rows)?;
    let mut sheet: Vec<Option<Sprite>> = Vec::new();
    for (slot, (_, sprite)) in slots.into_iter().zip(sprites) {
        if sheet.len() <= slot {
            sheet.resize(slot + 1, None);
        }
        sheet[slot] = Some(sprite);
    }
    Ok(SpriteSheet::from_slots(sheet))
}

/// The row `value` of the table at `table_path`, under `key`.
pub(crate) fn row_of(table_path: &str, key: &str, value: toml::Value) -> Result<Row> {
    value
        .try_into()
        .map_err(|error: toml::de::Error| row_error(table_path, key, error.message()))
}

/// A row of the table at `table_path` that the game cannot use.
pub(crate) fn row_error(table_path: &str, key: &str, reason: impl Into<String>) -> Error {
    Error::Row {
        table: table_path.into(),
        row: key.into(),
        reason: reason.into(),
    }
}

/// The sprite of each row, from `<folder>/<key>.png` in the last mod that has it, decoded on
/// several threads and given back in the rows' order.
pub(crate) fn sprites_in(
    stack: &ModStack,
    folder: &str,
    rows: Vec<(String, Row)>,
) -> Result<Vec<(String, Sprite)>> {
    let table_path = format!("{folder}.toml");
    rows.into_par_iter()
        .map(|(key, row)| {
            if !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err(row_error(&table_path, &key, "the key is not a plain file name"));
            }
            let png_path = format!("{folder}/{key}.png");
            let bytes = stack
                .read(&png_path)?
                .ok_or_else(|| row_error(&table_path, &key, format!("no {png_path}")))?;
            let sprite = decode(&bytes, row).map_err(|reason| row_error(&table_path, &key, reason))?;
            Ok((key, sprite))
        })
        .collect()
}

/// How a decoded item draws, or `None` for one that is not a picture.
pub(crate) const fn kind_of(item: &Item) -> Option<Kind> {
    match item {
        Item::Bitmap(_) | Item::RawBitmap(_) => Some(Kind::Colour),
        Item::PlayerBitmap(_) => Some(Kind::Player),
        Item::Shadow(_) => Some(Kind::Shadow),
        _ => None,
    }
}

/// Every sprite archive the data set holds, by data path.
pub(crate) fn archives(data: &DataSet) -> Vec<String> {
    let mut found = Vec::new();
    for folder in FOLDERS {
        let Ok(entries) = data.list_dir(folder) else {
            continue;
        };
        for entry in entries {
            let name = entry.name.to_ascii_uppercase();
            let archive = std::path::Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lst"));
            if archive && !NOT_SPRITES.contains(&name.as_str()) {
                found.push(format!("{folder}/{name}"));
            }
        }
    }
    found.sort();
    found
}

/// One item to bake: its sprite, how it draws and the palette its PNG carries.
pub(crate) struct Baked {
    /// Where the PNG goes in the mod.
    pub path: String,
    sprite: Sprite,
    kind: Kind,
    palette: Arc<Palette>,
}

impl Baked {
    /// `sprite`, drawn as `kind`, to go at `path` in a PNG carrying `palette`.
    pub(crate) const fn new(path: String, sprite: Sprite, kind: Kind, palette: Arc<Palette>) -> Self {
        Self {
            path,
            sprite,
            kind,
            palette,
        }
    }

    /// The item's PNG.
    pub(crate) fn png(&self) -> Result<Vec<u8>> {
        encode(&self.sprite, self.kind, &self.palette).map_err(|reason| Error::Row {
            table: self.path.clone(),
            row: String::new(),
            reason: reason.into(),
        })
    }
}

/// An archive ready to bake: its items, and its TOML with the path it goes to.
pub(crate) struct BakedArchive {
    /// The items.
    pub items: Vec<Baked>,
    /// Where the TOML goes, and its bytes.
    pub table: (String, Vec<u8>),
}

/// The items of the archive at `path` to bake, and its TOML.
pub(crate) fn bake_archive(data: &DataSet, path: &str) -> Result<BakedArchive> {
    let bytes = data.read(path)?;
    let archive = Archive::parse(&bytes).map_err(|source| Error::Archive {
        path: path.into(),
        source,
    })?;
    let palette = Arc::new(view_palette(data, path)?);
    let greys = if path == PORTRAITS && data.exists(GREY_PALETTE) {
        Some(grey_table(&read_palette(data, GREY_PALETTE)?, &palette))
    } else {
        None
    };
    let folder = sprite_folder(path);
    let mut rows = toml::Table::new();
    let mut items = Vec::new();
    for (slot, item) in archive.iter().enumerate() {
        let Some(kind) = kind_of(item) else {
            continue;
        };
        let Some(sprite) = Sprite::from_item(item) else {
            continue;
        };
        let sprite = match &greys {
            Some(table) if sprite.colours().all(|index| GREY_RAMP.contains(&index)) => {
                sprite.recoloured(table)
            }
            _ => sprite,
        };
        let origin = sprite.origin();
        let row = Row {
            kind,
            origin: [origin.x, origin.y],
        };
        let value = toml::Value::try_from(row).map_err(|source| Error::Write {
            path: folder.clone(),
            source,
        })?;
        rows.insert(slot.to_string(), value);
        items.push(Baked::new(
            format!("{folder}/{slot}.png"),
            sprite,
            kind,
            Arc::clone(&palette),
        ));
    }
    let table = toml::to_string(&rows).map_err(|source| Error::Write {
        path: folder.clone(),
        source,
    })?;
    Ok(BakedArchive {
        items,
        table: (format!("{folder}.toml"), table.into_bytes()),
    })
}

/// The palette an archive's PNGs carry: the landscape's for a landscape's own archives, the
/// greenland palette for the rest.
pub(crate) fn view_palette(data: &DataSet, path: &str) -> Result<Palette> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let palette = if name.starts_with("MAP_1") {
        WASTELAND_PALETTE
    } else if name.starts_with("MAP_2") || (path.starts_with("DATA/MBOB/") && name.starts_with('W')) {
        WINTER_PALETTE
    } else {
        GREENLAND_PALETTE
    };
    read_palette(data, palette)
}

/// The palette in the picture at data path `path`.
fn read_palette(data: &DataSet, path: &str) -> Result<Palette> {
    let bytes = data.read(path)?;
    s2_formats::iff::read_palette(&bytes).map_err(|_| Error::Row {
        table: path.into(),
        row: String::new(),
        reason: "the palette does not decode".into(),
    })
}

/// What each palette index of a portrait drawn for `greys` becomes to show in `shown`: an index
/// of the grey ramp becomes the colour of `shown` nearest its grey, which is the choice the Gold
/// Edition's portraits make for all 32, and any other index stays.
fn grey_table(greys: &Palette, shown: &Palette) -> [u8; 256] {
    let mut table = [0; 256];
    for (index, slot) in (0..=u8::MAX).zip(&mut table) {
        *slot = if GREY_RAMP.contains(&index) {
            shown.nearest(greys.colour(index))
        } else {
            index
        };
    }
    table
}

/// `sprite` as an indexed PNG.
fn encode(sprite: &Sprite, kind: Kind, palette: &Palette) -> Result<Vec<u8>, &'static str> {
    let (width, height) = (usize::from(sprite.width()), usize::from(sprite.height()));
    let mut pixels: Vec<Option<(Ink, u8)>> = vec![None; width * height];
    let mut used = [false; 256];
    for y in 0..sprite.height() {
        for run in sprite.row(y) {
            let start = usize::from(y) * width + usize::from(run.x);
            let values = sprite.values(run);
            for step in 0..usize::from(run.len) {
                let value = values.get(step).copied().unwrap_or_default();
                pixels[start + step] = Some((run.ink, value));
                match run.ink {
                    Ink::Colour if kind == Kind::Player && is_ramp(value) => {
                        return Err("a fixed colour inside the player ramp, 128 to 131");
                    }
                    Ink::Colour => used[usize::from(value)] = true,
                    Ink::Player if value >= PLAYER_SHADES => return Err("a player shade past 3"),
                    Ink::Player => used[usize::from(PLAYER_RAMP + value)] = true,
                    Ink::Shadow => {}
                }
            }
        }
    }
    let (indices, colours, alphas): (Vec<u8>, Vec<u8>, Vec<u8>) = if kind == Kind::Shadow {
        let indices = pixels.iter().map(|pixel| u8::from(pixel.is_some())).collect();
        (indices, vec![0; 6], vec![CLEAR, OPAQUE])
    } else {
        let clear = used
            .iter()
            .position(|&taken| !taken)
            .ok_or("every palette index is used")?;
        let clear = u8::try_from(clear).unwrap_or_default();
        let indices = pixels
            .iter()
            .map(|pixel| match *pixel {
                None | Some((Ink::Shadow, _)) => clear,
                Some((Ink::Colour, value)) => value,
                Some((Ink::Player, shade)) => PLAYER_RAMP + shade,
            })
            .collect();
        let mut alphas = vec![OPAQUE; 256];
        alphas[usize::from(clear)] = CLEAR;
        (indices, palette.0.iter().flatten().copied().collect(), alphas)
    };
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, u32::from(sprite.width()), u32::from(sprite.height()));
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(colours);
    encoder.set_trns(alphas);
    let mut writer = encoder.write_header().map_err(|_| "the PNG header")?;
    writer.write_image_data(&indices).map_err(|_| "the PNG pixels")?;
    writer.finish().map_err(|_| "the PNG end")?;
    Ok(out)
}

/// The sprite an indexed PNG holds, drawn as `row` says.
fn decode(bytes: &[u8], row: Row) -> Result<Sprite, &'static str> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|_| "not a PNG")?;
    let info = reader.info();
    if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
        return Err("not an 8-bit indexed PNG on the game's palette");
    }
    let (width, height) = (info.width, info.height);
    let alphas = info.trns.as_deref().map(<[u8]>::to_vec).unwrap_or_default();
    let mut indices = vec![0; reader.output_buffer_size().ok_or("too large")?];
    reader
        .next_frame(&mut indices)
        .map_err(|_| "the pixels do not decode")?;
    let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
        return Err("larger than 65535 pixels");
    };
    let clear = |index: u8| alphas.get(usize::from(index)) == Some(&CLEAR);
    let pixels = indices.iter().map(|&index| {
        if clear(index) {
            return None;
        }
        Some(match row.kind {
            Kind::Shadow => (Ink::Shadow, 0),
            Kind::Player if is_ramp(index) => (Ink::Player, index - PLAYER_RAMP),
            Kind::Colour | Kind::Player => (Ink::Colour, index),
        })
    });
    let origin = Origin {
        x: row.origin[0],
        y: row.origin[1],
    };
    Ok(Sprite::build(origin, width, height, pixels))
}

/// Whether `index` is one of player 0's shades.
const fn is_ramp(index: u8) -> bool {
    index >= PLAYER_RAMP && index < PLAYER_RAMP + PLAYER_SHADES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(kind: Kind) -> Sprite {
        let pixels = [
            None,
            Some((Ink::Colour, 7)),
            Some((Ink::Player, 2)),
            Some((Ink::Colour, 0)),
        ];
        let pixels = pixels.map(|pixel| match (kind, pixel) {
            (Kind::Shadow, Some(_)) => Some((Ink::Shadow, 0)),
            (Kind::Colour, Some((Ink::Player, _))) => Some((Ink::Colour, 200)),
            (_, pixel) => pixel,
        });
        Sprite::build(Origin { x: 1, y: -2 }, 2, 2, pixels)
    }

    #[test]
    fn every_kind_comes_back_as_it_went_in() {
        let palette = Palette([[1, 2, 3]; 256]);
        for kind in [Kind::Colour, Kind::Player, Kind::Shadow] {
            let original = sprite(kind);
            let png = encode(&original, kind, &palette).unwrap();
            let row = Row {
                kind,
                origin: [1, -2],
            };
            assert_eq!(decode(&png, row).unwrap(), original, "{kind:?}");
        }
    }

    #[test]
    fn a_fixed_colour_in_the_player_ramp_is_refused() {
        let pixels = [Some((Ink::Colour, PLAYER_RAMP + 1)), Some((Ink::Player, 0))];
        let sprite = Sprite::build(Origin { x: 0, y: 0 }, 2, 1, pixels);
        assert_eq!(
            encode(&sprite, Kind::Player, &Palette([[0; 3]; 256])),
            Err("a fixed colour inside the player ramp, 128 to 131")
        );
    }

    #[test]
    fn a_grey_of_the_ramp_takes_the_nearest_colour_shown() {
        let mut greys = Palette([[0; 3]; 256]);
        greys.0[200] = [90, 90, 90];
        let mut shown = Palette([[0; 3]; 256]);
        shown.0[200] = [120, 80, 40];
        shown.0[17] = [88, 92, 90];
        shown.0[30] = [88, 92, 90];
        let table = grey_table(&greys, &shown);
        assert_eq!(table[200], 17, "the nearer colour, and the lower of two as near");
        assert_eq!(table[100], 100, "an index off the ramp stays");
        let portrait = Sprite::build(
            Origin { x: 0, y: 0 },
            2,
            1,
            [Some((Ink::Colour, 200)), Some((Ink::Colour, 100))],
        );
        assert_eq!(
            portrait.recoloured(&table).colours().collect::<Vec<_>>(),
            [17, 100]
        );
    }

    #[test]
    fn an_archive_bakes_to_its_path_below_data() {
        assert_eq!(sprite_folder("DATA/MBOB/ROM_Z.LST"), "sprites/mbob/rom_z");
        assert_eq!(sprite_folder("DATA/MAP_0_Z.LST"), "sprites/map_0_z");
    }
}
