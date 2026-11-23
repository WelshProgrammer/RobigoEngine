//! The three landscapes and the files each one draws from.

use std::fmt;
use std::str::FromStr;

use s2_formats::map::Terrain;

/// A landscape: the set of palette, shading table, terrain sheet and object sprites a map uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Landscape {
    /// Temperate grass and forest.
    Greenland,
    /// Lava and wasteland.
    Wasteland,
    /// Snow and ice.
    Winter,
}

impl Landscape {
    /// Every landscape, in header byte order.
    pub const ALL: [Self; 3] = [Self::Greenland, Self::Wasteland, Self::Winter];

    /// The landscape a map header names, or `None` for a set the game does not ship.
    #[must_use]
    pub fn from_terrain(terrain: Terrain) -> Option<Self> {
        match terrain {
            Terrain::Greenland => Some(Self::Greenland),
            Terrain::Wasteland => Some(Self::Wasteland),
            Terrain::Winter => Some(Self::Winter),
            Terrain::Other(_) => None,
        }
    }

    /// The terrain set number the game indexes its per-landscape tables with: 0 greenland,
    /// 1 wasteland, 2 winter.
    #[must_use]
    pub const fn set(self) -> usize {
        match self {
            Self::Greenland => 0,
            Self::Wasteland => 1,
            Self::Winter => 2,
        }
    }

    /// The name used on the command line and in logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Greenland => "greenland",
            Self::Wasteland => "wasteland",
            Self::Winter => "winter",
        }
    }

    /// The palette the whole frame is drawn with.
    #[must_use]
    pub const fn palette_path(self) -> &'static str {
        match self {
            Self::Greenland => "GFX/PALETTE/PAL5.BBM",
            Self::Wasteland => "GFX/PALETTE/PAL6.BBM",
            Self::Winter => "GFX/PALETTE/PAL7.BBM",
        }
    }

    /// The terrain texture sheet. Its own `CMAP` is ignored in favour of [`Self::palette_path`].
    #[must_use]
    pub const fn texture_path(self) -> &'static str {
        match self {
            Self::Greenland => "GFX/TEXTURES/TEX5.LBM",
            Self::Wasteland => "GFX/TEXTURES/TEX6.LBM",
            Self::Winter => "GFX/TEXTURES/TEX7.LBM",
        }
    }

    /// The Gouraud shading table.
    #[must_use]
    pub const fn shading_path(self) -> &'static str {
        match self {
            Self::Greenland => "DATA/TEXTURES/GOU5.DAT",
            Self::Wasteland => "DATA/TEXTURES/GOU6.DAT",
            Self::Winter => "DATA/TEXTURES/GOU7.DAT",
        }
    }

    /// The archive of map object sprites: trees, stones and decorations with their shadows.
    /// The base game ships no winter set.
    #[must_use]
    pub const fn objects_path(self) -> &'static str {
        match self {
            Self::Greenland => "DATA/MAP_0_Z.LST",
            Self::Wasteland => "DATA/MAP_1_Z.LST",
            Self::Winter => "DATA/MAP_2_Z.LST",
        }
    }

    /// The archive the interface draws the wares and the people from (`0x200c46`).
    #[must_use]
    pub const fn icons_path(self) -> &'static str {
        match self {
            Self::Greenland => "DATA/MAP_0_Y.LST",
            Self::Wasteland => "DATA/MAP_1_Y.LST",
            Self::Winter => "DATA/MAP_2_Y.LST",
        }
    }

    /// The mission set a free-play game on this landscape loads: wasteland maps get set 1, the
    /// others none (`0x5ea48`). Decorations that come from a mission set draw nothing without one.
    #[must_use]
    pub const fn free_play_mission_set(self) -> Option<&'static str> {
        match self {
            Self::Wasteland => Some("DATA/MIS1BOBS.LST"),
            Self::Greenland | Self::Winter => None,
        }
    }
}

impl fmt::Display for Landscape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The text was not a landscape name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown landscape '{0}': expected greenland, wasteland or winter")]
pub struct UnknownLandscape(String);

impl FromStr for Landscape {
    type Err = UnknownLandscape;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|l| l.name().eq_ignore_ascii_case(s))
            .ok_or_else(|| UnknownLandscape(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_terrain_picks_the_landscape() {
        assert_eq!(
            Landscape::from_terrain(Terrain::Wasteland),
            Some(Landscape::Wasteland)
        );
        assert_eq!(Landscape::from_terrain(Terrain::Other(3)), None);
    }

    #[test]
    fn files_share_the_landscape_number() {
        assert_eq!(Landscape::Winter.palette_path(), "GFX/PALETTE/PAL7.BBM");
        assert_eq!(Landscape::Winter.texture_path(), "GFX/TEXTURES/TEX7.LBM");
        assert_eq!(Landscape::Winter.shading_path(), "DATA/TEXTURES/GOU7.DAT");
        assert_eq!(Landscape::Winter.objects_path(), "DATA/MAP_2_Z.LST");
        assert_eq!(Landscape::Greenland.icons_path(), "DATA/MAP_0_Y.LST");
    }

    #[test]
    fn names_round_trip() {
        for landscape in Landscape::ALL {
            assert_eq!(landscape.name().parse(), Ok(landscape));
        }
        assert_eq!("Winter".parse(), Ok(Landscape::Winter));
        assert!("lava".parse::<Landscape>().is_err());
    }
}
