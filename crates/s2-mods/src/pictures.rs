//! The full-screen pictures: the title, the backdrops of the screens before a game, the credits
//! and the missions' pictures. Each `.LBM` file of the original `GFX/PICS`, `GFX/PICS2` and
//! `GFX/PICS/MISSION` folders bakes to an 8-bit indexed PNG under `pictures/`, at its path below
//! `GFX` in lower case, whose pixels are the palette indices the game draws and whose colours are
//! the picture's own palette. `GFX/PICS/SETUP010.LBM` is `pictures/pics/setup010.png`.

use s2_data::DataSet;
use s2_formats::{Palette, iff};

use crate::sprites::{Sheets, row_error};
use crate::{Error, Result};

/// Where the pictures go in a mod.
pub const PICTURES: &str = "pictures";

/// The colour table the World Campaign's world screen recolours closed continents through, in
/// the data and in a mod: 4 bytes of header, then an entry per palette index.
pub const REMAP: &str = "DATA/REMAP.DAT";
/// See [`REMAP`].
pub const REMAP_BAKED: &str = "pictures/remap.dat";

/// The folders of the original data that hold the pictures.
const FOLDERS: [&str; 3] = ["GFX/PICS", "GFX/PICS2", "GFX/PICS/MISSION"];

/// The folder every picture folder sits in.
const ROOT: &str = "GFX/";

/// The ending of a picture file in the original data.
const ENDING: &str = ".LBM";

/// Colours in a palette.
const COLOURS: usize = 256;

/// A full-screen picture: its size, a palette index a pixel, and the palette it was drawn for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backdrop {
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// Its rows, a palette index a pixel.
    pub pixels: Vec<u8>,
    /// The colours it was drawn for.
    pub palette: Palette,
}

/// Where the picture at data path `path` goes in a mod, or `None` for a path outside the
/// picture folders.
#[must_use]
pub fn picture_path(path: &str) -> Option<String> {
    let upper = path.replace('\\', "/").to_ascii_uppercase();
    let (folder, name) = upper.rsplit_once('/')?;
    let stem = name.strip_suffix(ENDING)?;
    let below = folder.strip_prefix(ROOT)?;
    FOLDERS.contains(&folder).then(|| {
        format!(
            "{PICTURES}/{}/{}.png",
            below.to_ascii_lowercase(),
            stem.to_ascii_lowercase()
        )
    })
}

impl Sheets<'_> {
    /// The picture at data path `path`, such as `GFX/PICS/SETUP010.LBM`: the loaded mods' copy
    /// when the game has mods, otherwise the file in the data.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when no loaded mod holds the picture, [`Error::Row`] for a baked file
    /// that does not decode, and [`Error::Data`] or [`Error::Picture`] for an original that cannot
    /// be read.
    pub fn backdrop(&self, path: &str) -> Result<Backdrop> {
        match (self.stack, picture_path(path)) {
            (Some(stack), Some(baked)) => {
                let bytes = stack
                    .read(&baked)?
                    .ok_or_else(|| Error::NotInMods(baked.clone()))?;
                read_indexed(&bytes).map_err(|reason| row_error(&baked, "", reason))
            }
            _ => original(self.data, path),
        }
    }
}

impl Sheets<'_> {
    /// Whether [`Self::backdrop`] would find `path`.
    #[must_use]
    pub fn has_backdrop(&self, path: &str) -> bool {
        match (self.stack, picture_path(path)) {
            (Some(stack), Some(baked)) => stack.file(&baked).is_some(),
            _ => self.data.exists(path),
        }
    }

    /// The bytes of [`REMAP`]: the loaded mods' copy when the game has mods, otherwise the file
    /// in the data.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when no loaded mod holds the table, [`Error::Io`] when its copy
    /// cannot be read, and [`Error::Data`] for a file the data cannot give.
    pub fn remap_table(&self) -> Result<Vec<u8>> {
        match self.stack {
            Some(stack) => stack
                .read(REMAP_BAKED)?
                .ok_or_else(|| Error::NotInMods(REMAP_BAKED.to_owned())),
            None => Ok(self.data.read(REMAP)?),
        }
    }
}

/// The picture at data path `path` as the original file holds it.
fn original(data: &DataSet, path: &str) -> Result<Backdrop> {
    let picture = iff::parse(&data.read(path)?).map_err(|source| Error::Picture {
        path: path.to_owned(),
        source,
    })?;
    Ok(Backdrop {
        width: u32::from(picture.image.width()),
        height: u32::from(picture.image.height()),
        pixels: picture.image.pixels().to_vec(),
        palette: picture.palette.unwrap_or(Palette([[0; 3]; COLOURS])),
    })
}

/// Every picture in `data`, by data path in upper case, sorted.
pub(crate) fn picture_files(data: &DataSet) -> Vec<String> {
    let mut found: Vec<String> = FOLDERS
        .iter()
        .flat_map(|folder| {
            data.list_dir(folder)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry.kind == s2_data::EntryKind::File)
                .map(move |entry| format!("{folder}/{}", entry.name.to_ascii_uppercase()))
        })
        .filter(|path| path.ends_with(ENDING))
        .collect();
    found.sort();
    found
}

/// The baked file of the picture at data path `path`, by path inside the mod.
pub(crate) fn bake_picture(data: &DataSet, path: &str) -> Result<(String, Vec<u8>)> {
    let baked = picture_path(path).unwrap_or_default();
    let picture = original(data, path)?;
    let bytes = write_indexed(&picture).map_err(|reason| row_error(&baked, "", reason))?;
    Ok((baked, bytes))
}

/// `picture` as an 8-bit indexed PNG carrying its palette.
fn write_indexed(picture: &Backdrop) -> std::result::Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, picture.width, picture.height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(picture.palette.0.iter().flatten().copied().collect::<Vec<_>>());
    let mut writer = encoder.write_header().map_err(|_| "the PNG header")?;
    writer
        .write_image_data(&picture.pixels)
        .map_err(|_| "the PNG pixels")?;
    writer.finish().map_err(|_| "the PNG end")?;
    Ok(out)
}

/// The picture an 8-bit indexed PNG holds, with the colours of its palette. A palette shorter
/// than 256 colours leaves the rest black.
fn read_indexed(bytes: &[u8]) -> std::result::Result<Backdrop, &'static str> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|_| "not a PNG")?;
    let info = reader.info();
    if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
        return Err("not an 8-bit indexed PNG");
    }
    let (width, height) = (info.width, info.height);
    let mut palette = Palette([[0; 3]; COLOURS]);
    let colours = info.palette.as_deref().ok_or("no palette")?;
    for (slot, colour) in palette.0.iter_mut().zip(colours.as_chunks::<3>().0) {
        *slot = *colour;
    }
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("too large")?];
    reader
        .next_frame(&mut pixels)
        .map_err(|_| "the pixels do not decode")?;
    Ok(Backdrop {
        width,
        height,
        pixels,
        palette,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_keep_their_folder_and_name_in_lower_case() {
        assert_eq!(
            picture_path("GFX/PICS/SETUP010.LBM").as_deref(),
            Some("pictures/pics/setup010.png")
        );
        assert_eq!(
            picture_path("gfx\\pics2\\credit00.lbm").as_deref(),
            Some("pictures/pics2/credit00.png")
        );
        assert_eq!(
            picture_path("GFX/PICS/MISSION/AFRICA.LBM").as_deref(),
            Some("pictures/pics/mission/africa.png")
        );
        assert_eq!(picture_path("GFX/TEXTURES/TEX5.LBM"), None);
        assert_eq!(picture_path("GFX/PICS/README.TXT"), None);
    }

    #[test]
    fn a_picture_comes_back_as_it_went_in() {
        let mut palette = Palette([[0; 3]; COLOURS]);
        for (index, colour) in palette.0.iter_mut().enumerate() {
            let shade = u8::try_from(index).unwrap();
            *colour = [shade, shade / 2, u8::MAX - shade];
        }
        let picture = Backdrop {
            width: 7,
            height: 3,
            pixels: (0..21).collect(),
            palette,
        };
        let bytes = write_indexed(&picture).unwrap();
        assert_eq!(read_indexed(&bytes).unwrap(), picture);
    }
}
