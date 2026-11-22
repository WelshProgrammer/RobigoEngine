//! The four peoples a player can lead, and the sprite archive each one's buildings, flags and
//! border stones come from.

use std::fmt;
use std::str::FromStr;

use crate::Landscape;

/// A player's people. The numbers are the nation bytes `S2.EXE` keeps per player, in the order
/// the executable names the archives: Romans, Vikings, Japanese, Africans. The per-nation
/// building table at `0x1f7d78` confirms the order by placing each headquarters' flag on that
/// nation's roof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Nation {
    /// `ROM`.
    #[default]
    Romans,
    /// `VIK`.
    Vikings,
    /// `JAP`.
    Japanese,
    /// `AFR`, the Nubians.
    Africans,
}

impl Nation {
    /// Every nation, by number.
    pub const ALL: [Self; 4] = [Self::Romans, Self::Vikings, Self::Japanese, Self::Africans];

    /// The nation byte.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The name used on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Romans => "romans",
            Self::Vikings => "vikings",
            Self::Japanese => "japanese",
            Self::Africans => "africans",
        }
    }

    /// The nation's sprite archive for maps of `landscape`.
    ///
    /// Inferred: winter maps use the `W` archives (snow on the roofs). The executable names both
    /// sets. The base game ships only the plain ones and has no winter maps. Which set a winter map
    /// loads was not traced.
    #[must_use]
    pub const fn sprites_path(self, landscape: Landscape) -> &'static str {
        let winter = matches!(landscape, Landscape::Winter);
        match (self, winter) {
            (Self::Romans, false) => "DATA/MBOB/ROM_Z.LST",
            (Self::Romans, true) => "DATA/MBOB/WROM_Z.LST",
            (Self::Vikings, false) => "DATA/MBOB/VIK_Z.LST",
            (Self::Vikings, true) => "DATA/MBOB/WVIK_Z.LST",
            (Self::Japanese, false) => "DATA/MBOB/JAP_Z.LST",
            (Self::Japanese, true) => "DATA/MBOB/WJAP_Z.LST",
            (Self::Africans, false) => "DATA/MBOB/AFR_Z.LST",
            (Self::Africans, true) => "DATA/MBOB/WAFR_Z.LST",
        }
    }

    /// The nation's archive of building icons, which the action menu's type buttons draw
    /// from by graphics group.
    #[must_use]
    pub const fn icons_path(self) -> &'static str {
        match self {
            Self::Romans => "DATA/MBOB/ROM_ICON.LST",
            Self::Vikings => "DATA/MBOB/VIK_ICON.LST",
            Self::Japanese => "DATA/MBOB/JAP_ICON.LST",
            Self::Africans => "DATA/MBOB/AFR_ICON.LST",
        }
    }
}

impl fmt::Display for Nation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The text was not a nation name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown nation '{0}': expected romans, vikings, japanese or africans")]
pub struct UnknownNation(String);

impl FromStr for Nation {
    type Err = UnknownNation;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|n| n.name().eq_ignore_ascii_case(s) || n.name()[..3].eq_ignore_ascii_case(s))
            .ok_or_else(|| UnknownNation(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_paths() {
        assert_eq!("Vik".parse(), Ok(Nation::Vikings));
        assert_eq!("africans".parse(), Ok(Nation::Africans));
        assert!("nubians".parse::<Nation>().is_err());
        assert_eq!(Nation::Japanese.index(), 2);
        assert_eq!(
            Nation::Romans.sprites_path(Landscape::Winter),
            "DATA/MBOB/WROM_Z.LST"
        );
        assert_eq!(
            Nation::Africans.sprites_path(Landscape::Wasteland),
            "DATA/MBOB/AFR_Z.LST"
        );
        assert_eq!(Nation::Vikings.icons_path(), "DATA/MBOB/VIK_ICON.LST");
    }
}
