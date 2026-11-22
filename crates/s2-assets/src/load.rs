//! Everything one landscape needs, read from a data set or from bytes.

use s2_data::DataSet;
use s2_formats::gouraud::ShadingTable;
use s2_formats::{Palette, iff, lst};

use crate::palette::dac_palette;
use crate::{Error, Landscape, Remap, Result, SpriteSheet, Tint};

/// Texels in a terrain texture sheet: 256 rows of 256.
pub const TEXTURE_SIZE: usize = 256 * 256;

/// Width and height of a terrain texture sheet.
pub const TEXTURE_SIDE: u16 = 256;

/// The raw bytes of a landscape's files, for building [`LandscapeAssets`] without a data set.
#[derive(Debug, Clone, Copy)]
pub struct LandscapeFiles<'a> {
    /// The palette file, [`Landscape::palette_path`].
    pub palette: &'a [u8],
    /// The terrain sheet, [`Landscape::texture_path`].
    pub texture: &'a [u8],
    /// The shading table, [`Landscape::shading_path`].
    pub shading: &'a [u8],
    /// The object archive, [`Landscape::objects_path`], if the data has one.
    pub objects: Option<&'a [u8]>,
}

/// One landscape's palette, shading table, terrain texture sheet, shadow tables and object
/// sprites.
#[derive(Debug, Clone)]
pub struct LandscapeAssets {
    landscape: Landscape,
    palette: Palette,
    shading: ShadingTable,
    texture: Box<[u8; TEXTURE_SIZE]>,
    shadow: Remap,
    grey_shadow: Remap,
    work: Remap,
    paused: Remap,
    highlight: Remap,
    disabled: Remap,
    objects: Option<SpriteSheet>,
}

impl LandscapeAssets {
    /// Reads `landscape`'s files from `data`. A missing object archive is not an error: it leaves
    /// [`Self::objects`] empty. The base game has no winter set.
    ///
    /// # Errors
    ///
    /// [`Error::Data`] if a required file cannot be read, otherwise as [`Self::from_files`].
    pub fn load(data: &DataSet, landscape: Landscape) -> Result<Self> {
        let palette = data.read(landscape.palette_path())?;
        let texture = data.read(landscape.texture_path())?;
        let shading = data.read(landscape.shading_path())?;
        let objects = data
            .exists(landscape.objects_path())
            .then(|| data.read(landscape.objects_path()))
            .transpose()?;
        Self::from_files(
            landscape,
            &LandscapeFiles {
                palette: &palette,
                texture: &texture,
                shading: &shading,
                objects: objects.as_deref(),
            },
        )
    }

    /// Decodes `landscape`'s files from bytes.
    ///
    /// # Errors
    ///
    /// [`Error::Iff`], [`Error::Shading`] or [`Error::Archive`] when a file does not decode, and
    /// [`Error::SheetSize`] when the terrain sheet is not 256 pixels square.
    pub fn from_files(landscape: Landscape, files: &LandscapeFiles<'_>) -> Result<Self> {
        let palette = iff::read_palette(files.palette).map_err(|source| Error::Iff {
            path: landscape.palette_path(),
            source,
        })?;
        let texture_path = landscape.texture_path();
        let picture = iff::parse(files.texture).map_err(|source| Error::Iff {
            path: texture_path,
            source,
        })?;
        let (width, height) = (picture.image.width(), picture.image.height());
        let texture = Box::<[u8]>::from(picture.image.pixels())
            .try_into()
            .ok()
            .filter(|_| (width, height) == (TEXTURE_SIDE, TEXTURE_SIDE))
            .ok_or(Error::SheetSize {
                path: texture_path,
                width,
                height,
            })?;
        let shading = ShadingTable::parse(files.shading).map_err(|source| Error::Shading {
            path: landscape.shading_path(),
            source,
        })?;
        let objects = files
            .objects
            .map(|bytes| {
                lst::Archive::parse(bytes).map_err(|source| Error::Archive {
                    path: landscape.objects_path(),
                    source,
                })
            })
            .transpose()?
            .map(|archive| SpriteSheet::from_archive(&archive));
        let mut assets = Self::from_parts(landscape, &palette, texture, shading);
        assets.objects = objects;
        Ok(assets)
    }

    /// `landscape`'s assets from its decoded parts: `palette` as the palette file holds it, in
    /// 8-bit colours, the terrain sheet and the shading table. The object sprites are left out.
    #[must_use]
    pub fn from_parts(
        landscape: Landscape,
        palette: &Palette,
        texture: Box<[u8; TEXTURE_SIZE]>,
        shading: ShadingTable,
    ) -> Self {
        let shadow = Remap::nearest(palette, Tint::SHADOW);
        let lighter = Remap::nearest(palette, Tint::HIGHLIGHT);
        let twice = lighter.then(&lighter);
        Self {
            landscape,
            paused: shadow.then(&shadow),
            highlight: twice.then(&twice),
            disabled: shadow
                .then(&shadow)
                .then(&Remap::nearest(palette, Tint::DISABLED)),
            shadow,
            grey_shadow: Remap::nearest(palette, Tint::GREY_SHADOW),
            work: Remap::nearest(palette, Tint::WORK),
            palette: dac_palette(palette),
            shading,
            texture,
            objects: None,
        }
    }

    /// Which landscape these are.
    #[must_use]
    pub fn landscape(&self) -> Landscape {
        self.landscape
    }

    /// The palette the whole frame resolves through, as the VGA DAC shows it (see
    /// [`dac_palette`]), before any colour cycling.
    #[must_use]
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The Gouraud shading table.
    #[must_use]
    pub fn shading(&self) -> &ShadingTable {
        &self.shading
    }

    /// The terrain texture sheet: 256 rows of 256 palette indices. The sheet's own palette is
    /// not used, as in the game.
    #[must_use]
    pub fn texture(&self) -> &[u8; TEXTURE_SIZE] {
        &self.texture
    }

    /// The normal shadow table ([`Tint::SHADOW`]), built from the palette file's 8-bit colours.
    #[must_use]
    pub fn shadow(&self) -> &Remap {
        &self.shadow
    }

    /// The grey shadow table ([`Tint::GREY_SHADOW`]).
    #[must_use]
    pub fn grey_shadow(&self) -> &Remap {
        &self.grey_shadow
    }

    /// The table a working building's glow goes through ([`Tint::WORK`]).
    #[must_use]
    pub fn work(&self) -> &Remap {
        &self.work
    }

    /// The table a paused view draws through: the normal shadow table applied twice.
    #[must_use]
    pub fn paused(&self) -> &Remap {
        &self.paused
    }

    /// The table a marked control's background goes through: [`Tint::HIGHLIGHT`] applied four
    /// times.
    #[must_use]
    pub fn highlight(&self) -> &Remap {
        &self.highlight
    }

    /// The table a disabled control goes through once it is drawn: the normal shadow table
    /// twice, then [`Tint::DISABLED`] (`0x4d8c0`).
    #[must_use]
    pub fn disabled(&self) -> &Remap {
        &self.disabled
    }

    /// The map object sprites, if the data has this landscape's archive.
    #[must_use]
    pub fn objects(&self) -> Option<&SpriteSheet> {
        self.objects.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use s2_formats::gouraud;

    use super::*;

    fn iff_chunk(id: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend(u32::try_from(body.len()).unwrap().to_be_bytes());
        out.extend(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    /// An uncompressed `FORM PBM` of the given size filled with `index`, with a grey CMAP.
    fn pbm(width: u16, height: u16, index: u8) -> Vec<u8> {
        let mut bmhd = Vec::new();
        bmhd.extend(width.to_be_bytes());
        bmhd.extend(height.to_be_bytes());
        bmhd.extend([0; 4]);
        bmhd.extend([8, 0, 0, 0]);
        bmhd.extend([0, 0, 1, 1]);
        bmhd.extend(width.to_be_bytes());
        bmhd.extend(height.to_be_bytes());
        let cmap: Vec<u8> = (0..=255u8).flat_map(|v| [v, v, v]).collect();
        let body = vec![index; usize::from(width) * usize::from(height)];
        let mut form = b"PBM ".to_vec();
        form.extend(iff_chunk(*b"BMHD", &bmhd));
        form.extend(iff_chunk(*b"CMAP", &cmap));
        form.extend(iff_chunk(*b"BODY", &body));
        iff_chunk(*b"FORM", &form)
    }

    fn shading() -> Vec<u8> {
        (0..gouraud::TABLE_SIZE)
            .map(|i| u8::try_from((i + i / 256) % 256).unwrap())
            .collect()
    }

    #[test]
    fn builds_from_synthetic_files() {
        let palette = pbm(1, 1, 0);
        let texture = pbm(256, 256, 42);
        let shading = shading();
        let assets = LandscapeAssets::from_files(
            Landscape::Wasteland,
            &LandscapeFiles {
                palette: &palette,
                texture: &texture,
                shading: &shading,
                objects: None,
            },
        )
        .unwrap();
        assert_eq!(assets.landscape(), Landscape::Wasteland);
        assert_eq!(
            assets.palette().colour(200),
            [203, 203, 203],
            "as the DAC shows it"
        );
        assert_eq!(assets.palette().colour(255), [255, 255, 255]);
        assert!(assets.texture().iter().all(|&t| t == 42));
        assert_eq!(assets.shading().shade(1, 255), 0, "row 1, column 255");
        assert!(assets.objects().is_none());
    }

    #[test]
    fn a_sheet_of_the_wrong_size_is_rejected() {
        let palette = pbm(1, 1, 0);
        let texture = pbm(64, 64, 0);
        let shading = shading();
        let err = LandscapeAssets::from_files(
            Landscape::Greenland,
            &LandscapeFiles {
                palette: &palette,
                texture: &texture,
                shading: &shading,
                objects: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            Error::SheetSize {
                width: 64,
                height: 64,
                ..
            }
        ));
    }
}
