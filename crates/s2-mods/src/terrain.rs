//! Each landscape's terrain: its palette, terrain sheet and shading table, in a folder under
//! `terrain/` named after the landscape.
//!
//! `palette.toml` holds a row an index, 0 to 255, each `[red, green, blue]` in the 8-bit values
//! the palette file stores. `texture.png` is the 256 by 256 terrain sheet and `shading.png` the
//! shading table, a row a light level and a column a palette index, both indexed PNGs whose
//! pixels are the palette indices the game draws and whose colours are the landscape's palette.

use s2_assets::{Landscape, LandscapeAssets, LandscapeFiles, TEXTURE_SIDE, TEXTURE_SIZE};
use s2_data::DataSet;
use s2_formats::Palette;
use s2_formats::gouraud::{ShadingTable, TABLE_SIZE};
use s2_formats::iff;

use crate::sprites::{Sheets, row_error};
use crate::{Error, ModStack, Result};

/// Where the terrain goes in a mod.
pub const TERRAIN: &str = "terrain";

/// The files in a landscape's folder.
const PALETTE: &str = "palette.toml";
const TEXTURE: &str = "texture.png";
const SHADING: &str = "shading.png";

/// Colours in a palette.
const COLOURS: usize = 256;

/// Rows and columns in a shading table.
const SHADING_SIDE: u16 = 256;

/// The mod folder `landscape`'s terrain bakes to. Greenland's is `terrain/greenland`.
#[must_use]
pub fn terrain_folder(landscape: Landscape) -> String {
    format!("{TERRAIN}/{landscape}")
}

impl Sheets<'_> {
    /// `landscape`'s palette, terrain sheet and shading table, and the tables built from them.
    /// The object sprites are left out; they come from [`Sheets::sheet`].
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack has no such terrain, [`Error::Row`] for a baked file
    /// that does not decode, and [`Error::Data`] or [`Error::Assets`] for an original that cannot
    /// be read.
    pub fn landscape(&self, landscape: Landscape) -> Result<LandscapeAssets> {
        let folder = terrain_folder(landscape);
        if let Some(stack) = self.stack {
            return terrain(stack, landscape, &folder);
        }
        let palette = self.data.read(landscape.palette_path())?;
        let texture = self.data.read(landscape.texture_path())?;
        let shading = self.data.read(landscape.shading_path())?;
        let files = LandscapeFiles {
            palette: &palette,
            texture: &texture,
            shading: &shading,
            objects: None,
        };
        Ok(LandscapeAssets::from_files(landscape, &files)?)
    }
}

/// `landscape`'s terrain as the stack holds it in `folder`.
fn terrain(stack: &ModStack, landscape: Landscape, folder: &str) -> Result<LandscapeAssets> {
    let palette_path = format!("{folder}/{PALETTE}");
    if stack.file(&palette_path).is_none() {
        return Err(Error::NotInMods(palette_path));
    }
    let mut colours = [[0; 3]; COLOURS];
    let mut given = 0;
    for (key, value) in stack.table(&palette_path)? {
        let index: usize = key
            .parse()
            .ok()
            .filter(|&index| index < COLOURS)
            .ok_or_else(|| row_error(&palette_path, &key, "the key is a palette index from 0 to 255"))?;
        colours[index] = value.try_into().map_err(|_| {
            row_error(
                &palette_path,
                &key,
                "a colour is [red, green, blue], each 0 to 255",
            )
        })?;
        given += 1;
    }
    if given != COLOURS {
        return Err(row_error(&palette_path, "", "a palette has all 256 colours"));
    }
    let picture = |name: &str, side: u16| -> Result<Vec<u8>> {
        let path = format!("{folder}/{name}");
        let bytes = stack.read(&path)?.ok_or_else(|| Error::NotInMods(path.clone()))?;
        read_indexed(&bytes, side).map_err(|reason| row_error(&path, "", reason))
    };
    let texture = picture(TEXTURE, TEXTURE_SIDE)?
        .into_boxed_slice()
        .try_into()
        .map_err(|_| row_error(folder, TEXTURE, "the terrain sheet is 256 pixels square"))?;
    let shading = ShadingTable::parse(&picture(SHADING, SHADING_SIDE)?)
        .map_err(|_| row_error(folder, SHADING, "the shading table is 256 pixels square"))?;
    Ok(LandscapeAssets::from_parts(
        landscape,
        &Palette(colours),
        texture,
        shading,
    ))
}

/// The landscapes whose terrain the data set holds.
pub(crate) fn landscapes(data: &DataSet) -> Vec<Landscape> {
    Landscape::ALL
        .into_iter()
        .filter(|landscape| {
            [
                landscape.palette_path(),
                landscape.texture_path(),
                landscape.shading_path(),
            ]
            .into_iter()
            .all(|path| data.exists(path))
        })
        .collect()
}

/// The source files of every landscape's terrain, by data path.
pub(crate) fn terrain_files(data: &DataSet) -> Vec<String> {
    landscapes(data)
        .into_iter()
        .flat_map(|landscape| {
            [
                landscape.palette_path(),
                landscape.texture_path(),
                landscape.shading_path(),
            ]
        })
        .map(str::to_owned)
        .collect()
}

/// `landscape`'s terrain files to bake, by path inside the mod.
pub(crate) fn bake_terrain(data: &DataSet, landscape: Landscape) -> Result<Vec<(String, Vec<u8>)>> {
    let folder = terrain_folder(landscape);
    let palette =
        iff::read_palette(&data.read(landscape.palette_path())?).map_err(|source| s2_assets::Error::Iff {
            path: landscape.palette_path(),
            source,
        })?;
    let texture =
        iff::parse(&data.read(landscape.texture_path())?).map_err(|source| s2_assets::Error::Iff {
            path: landscape.texture_path(),
            source,
        })?;
    let shading = ShadingTable::parse(&data.read(landscape.shading_path())?).map_err(|source| {
        s2_assets::Error::Shading {
            path: landscape.shading_path(),
            source,
        }
    })?;
    let mut rows = toml::Table::new();
    for (index, colour) in palette.0.iter().enumerate() {
        let colour = toml::Value::Array(colour.iter().map(|&c| toml::Value::Integer(c.into())).collect());
        rows.insert(index.to_string(), colour);
    }
    let palette_file = toml::to_string(&rows).map_err(|source| Error::Write {
        path: format!("{folder}/{PALETTE}"),
        source,
    })?;
    let pixels = texture.image.pixels();
    if pixels.len() != TEXTURE_SIZE {
        return Err(s2_assets::Error::SheetSize {
            path: landscape.texture_path(),
            width: texture.image.width(),
            height: texture.image.height(),
        }
        .into());
    }
    let table: Vec<u8> = (0..=u8::MAX).flat_map(|level| *shading.row(level)).collect();
    debug_assert_eq!(table.len(), TABLE_SIZE);
    let picture = |name: &str, side: u16, pixels: &[u8]| {
        write_indexed(side, pixels, &palette)
            .map(|bytes| (format!("{folder}/{name}"), bytes))
            .map_err(|reason| row_error(&folder, name, reason))
    };
    Ok(vec![
        (format!("{folder}/{PALETTE}"), palette_file.into_bytes()),
        picture(TEXTURE, TEXTURE_SIDE, pixels)?,
        picture(SHADING, SHADING_SIDE, &table)?,
    ])
}

/// `pixels`, `side` rows of `side`, as an 8-bit indexed PNG carrying `palette`.
fn write_indexed(side: u16, pixels: &[u8], palette: &Palette) -> std::result::Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, u32::from(side), u32::from(side));
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette.0.iter().flatten().copied().collect::<Vec<_>>());
    let mut writer = encoder.write_header().map_err(|_| "the PNG header")?;
    writer.write_image_data(pixels).map_err(|_| "the PNG pixels")?;
    writer.finish().map_err(|_| "the PNG end")?;
    Ok(out)
}

/// The pixels of an 8-bit indexed PNG `side` pixels square, row by row.
fn read_indexed(bytes: &[u8], side: u16) -> std::result::Result<Vec<u8>, &'static str> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|_| "not a PNG")?;
    let info = reader.info();
    if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
        return Err("not an 8-bit indexed PNG");
    }
    if (info.width, info.height) != (u32::from(side), u32::from(side)) {
        return Err("the wrong size");
    }
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("too large")?];
    reader
        .next_frame(&mut pixels)
        .map_err(|_| "the pixels do not decode")?;
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_indexed_picture_comes_back_as_it_went_in() {
        let pixels: Vec<u8> = (0..u32::from(SHADING_SIDE) * u32::from(SHADING_SIDE))
            .map(|n| u8::try_from(n % 251).unwrap())
            .collect();
        let palette = Palette([[1, 2, 3]; COLOURS]);
        let file = write_indexed(SHADING_SIDE, &pixels, &palette).unwrap();
        assert_eq!(read_indexed(&file, SHADING_SIDE).unwrap(), pixels);
        assert_eq!(read_indexed(&file, 255), Err("the wrong size"));
    }

    #[test]
    fn a_landscape_bakes_to_a_folder_named_after_it() {
        assert_eq!(terrain_folder(Landscape::Winter), "terrain/winter");
    }
}
