//! Telling the retail CD, the v1.51 Mission CD build and the Gold Edition apart.

use std::fmt;

use crate::error::{Error, Result};

const BANNERS: [&[u8]; 2] = [b"The Settlers II", b"Die Siedler II"];
/// What a v1.51 build's banner names: the patched game and GOG's copy the Mission CD, the Gold
/// Edition CD's executable the Gold Edition.
const V151_BUILDS: [&[u8]; 2] = [b"Mission CD", b"Gold Edition"];
const GOLD_MARKER: &[u8] = b"africa.wld";
const BANNER_LIMIT: usize = 120;

/// Which build of the game a data set holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Release {
    /// The original retail CD. Its README says 1.02 but `S2.EXE` reports V1.03.
    Original,
    /// The v1.51 "Mission CD" build without the World Campaign (the official patch applied to
    /// the retail CD).
    V151,
    /// The Gold Edition: v1.51 plus the World Campaign (`DATA/MAPS2/AFRICA.WLD` and friends).
    Gold,
}

/// The detected edition and the version string the executable reports.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edition {
    release: Release,
    version: String,
}

impl Edition {
    /// The build this is.
    #[must_use]
    pub fn release(&self) -> Release {
        self.release
    }

    /// The version as printed in `S2.EXE`'s banner, without the leading `V` (e.g. `1.51`).
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl fmt::Display for Edition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.release {
            Release::Original => "original CD",
            Release::V151 => "Mission CD",
            Release::Gold => "Gold Edition",
        };
        write!(f, "The Settlers II v{} ({name})", self.version)
    }
}

/// Classifies `S2.EXE`; `world_campaign` says whether `DATA/MAPS2/AFRICA.WLD` is present.
///
/// Gold needs both the World Campaign data and an executable that knows about it.
pub(crate) fn classify(exe: &[u8], world_campaign: bool) -> Result<Edition> {
    let banner = BANNERS
        .iter()
        .flat_map(|b| positions(exe, b))
        .map(|at| {
            let rest = &exe[at..exe.len().min(at + BANNER_LIMIT)];
            &rest[..rest.iter().position(|&c| c == 0).unwrap_or(rest.len())]
        })
        .find(|banner| version(banner).is_some())
        .ok_or(Error::UnknownEdition)?;
    let version = version(banner).ok_or(Error::UnknownEdition)?;
    let release = if V151_BUILDS.iter().all(|build| find(banner, build).is_none()) {
        Release::Original
    } else if world_campaign && find(exe, GOLD_MARKER).is_some() {
        Release::Gold
    } else {
        Release::V151
    };
    Ok(Edition { release, version })
}

/// Pulls `1.51` out of `... V1.51. Copyright ...`.
fn version(banner: &[u8]) -> Option<String> {
    banner.windows(2).enumerate().find_map(|(i, w)| {
        if w[0] != b'V' || !w[1].is_ascii_digit() {
            return None;
        }
        let digits = &banner[i + 1..];
        let end = digits
            .iter()
            .position(|&c| !(c.is_ascii_digit() || c == b'.'))
            .unwrap_or(digits.len());
        let text = std::str::from_utf8(&digits[..end]).ok()?.trim_end_matches('.');
        text.contains('.').then(|| text.to_owned())
    })
}

fn positions<'a>(haystack: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(move |(_, w)| *w == needle)
        .map(|(i, _)| i)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    positions(haystack, needle).next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(parts: &[&[u8]]) -> Vec<u8> {
        let mut out = b"MZ\0\0junk\0".to_vec();
        for p in parts {
            out.extend_from_slice(p);
            out.push(0);
        }
        out
    }

    #[test]
    fn recognises_the_original_cd() {
        let e = classify(
            &exe(&[b"The Settlers II. V1.03. Copyright BlueByte 1995-96."]),
            false,
        )
        .unwrap();
        assert_eq!((e.release(), e.version()), (Release::Original, "1.03"));
        assert_eq!(e.to_string(), "The Settlers II v1.03 (original CD)");
    }

    #[test]
    fn needs_both_exe_and_data_for_gold() {
        let banner: &[u8] = b"The Settlers II Mission CD. V1.51. Copyright BlueByte 1996.";
        let plain = exe(&[banner, b"WORLD_V1.0"]);
        let gold = exe(&[banner, b"africa.wld"]);
        assert_eq!(classify(&plain, false).unwrap().release(), Release::V151);
        assert_eq!(classify(&plain, true).unwrap().release(), Release::V151);
        assert_eq!(classify(&gold, false).unwrap().release(), Release::V151);
        let e = classify(&gold, true).unwrap();
        assert_eq!((e.release(), e.version()), (Release::Gold, "1.51"));
    }

    #[test]
    fn the_gold_edition_cds_banner_is_v151() {
        let banner: &[u8] = b"The Settlers II Gold Edition. V1.51. Copyright BlueByte 1997.";
        let e = classify(&exe(&[banner, b"africa.wld"]), true).unwrap();
        assert_eq!((e.release(), e.version()), (Release::Gold, "1.51"));
        let e = classify(&exe(&[banner, b"africa.wld"]), false).unwrap();
        assert_eq!(
            e.release(),
            Release::V151,
            "an install missing the World Campaign"
        );
    }

    #[test]
    fn falls_back_to_the_german_banner() {
        let e = classify(
            &exe(&[b"Die Siedler II Mission CD V1.51. Copyright BlueByte 1996."]),
            false,
        )
        .unwrap();
        assert_eq!((e.release(), e.version()), (Release::V151, "1.51"));
    }

    #[test]
    fn skips_banners_without_a_version() {
        let e = classify(
            &exe(&[b"The Settlers II", b"The Settlers II. V1.03. Copyright"]),
            false,
        )
        .unwrap();
        assert_eq!(e.version(), "1.03");
    }

    #[test]
    fn rejects_other_programs() {
        assert!(matches!(classify(b"MZ hello", false), Err(Error::UnknownEdition)));
        assert!(matches!(
            classify(&exe(&[b"The Settlers II"]), false),
            Err(Error::UnknownEdition)
        ));
    }
}
