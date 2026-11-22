//! The sound fonts the General MIDI rendering plays the songs with: two Robigo offers, fetched once
//! from where Robigo keeps them and checked against their digests, or one of the player's own.

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Where Robigo keeps the sound fonts it offers: its GitHub releases, one per sound font.
const RELEASES: &str = "https://github.com/WelshProgrammer/RobigoEngine/releases/download";

/// A sound font Robigo offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offered {
    /// The name the settings give it.
    pub name: &'static str,
    /// The name a player reads.
    pub title: &'static str,
    /// The tag of the release that carries it.
    pub release: &'static str,
    /// Its file in the release and in the cache.
    pub file: &'static str,
    /// Its size in bytes.
    pub size: u64,
    /// Its SHA-256 digest, in hexadecimal.
    pub digest: &'static str,
}

/// GeneralUser GS 2.0.3 by S. Christian Collins, which Robigo plays with unless told otherwise.
pub const GENERALUSER_GS: Offered = Offered {
    name: "generaluser-gs",
    title: "GeneralUser GS",
    release: "generaluser-gs-2.0.3",
    file: "GeneralUser-GS.sf2",
    size: 32_319_396,
    digest: "9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe",
};

/// MuseScore General 0.2, the MIT-licensed descendant of Frank Wen's FluidR3.
pub const MUSESCORE_GENERAL: Offered = Offered {
    name: "musescore-general",
    title: "MuseScore General",
    release: "musescore-general-0.2",
    file: "MuseScore_General.sf2",
    size: 215_614_036,
    digest: "ee51d2c4b1525e70f19a45909c4fd7a2e26d91d115fa89dbf5a6bc413d8b9bf3",
};

/// The sound fonts Robigo offers.
pub const OFFERED: [Offered; 2] = [GENERALUSER_GS, MUSESCORE_GENERAL];

impl Offered {
    /// Its size in megabytes of a million bytes, to the nearest.
    #[must_use]
    pub const fn megabytes(&self) -> u64 {
        (self.size + MEGABYTE / 2) / MEGABYTE
    }
}

/// A megabyte, as a download's size is given.
const MEGABYTE: u64 = 1_000_000;

/// The sound font the settings pick: one Robigo offers, or a file of the player's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bank {
    /// One Robigo offers.
    Offered(Offered),
    /// The player's own SF2 file.
    File(PathBuf),
}

impl Bank {
    /// The bank `setting` names: an offered sound font by its name, or else a path to a file.
    #[must_use]
    pub fn from_setting(setting: &str) -> Self {
        OFFERED
            .into_iter()
            .find(|offered| offered.name.eq_ignore_ascii_case(setting))
            .map_or_else(|| Self::File(PathBuf::from(setting)), Self::Offered)
    }

    /// What tells one rendering's sound font from another's without reading it: an offered
    /// sound font's digest, or a file's path, size and time of change. `None` for a file that
    /// is not there.
    #[must_use]
    pub fn identity(&self) -> Option<String> {
        match self {
            Self::Offered(offered) => Some(offered.digest.to_owned()),
            Self::File(path) => {
                let meta = std::fs::metadata(path).ok()?;
                let changed = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |since| since.as_secs());
                Some(format!("{} {} {changed}", path.display(), meta.len()))
            }
        }
    }

    /// Whether the sound font is there without a fetch: the player's file exists, or an offered
    /// one sits in `cache` at its size. The digest is checked only when the font is used.
    #[must_use]
    pub fn installed(&self, cache: &Path) -> bool {
        match self {
            Self::File(path) => path.is_file(),
            Self::Offered(offered) => {
                std::fs::metadata(cache.join(offered.file)).is_ok_and(|meta| meta.len() == offered.size)
            }
        }
    }

    /// The sound font's file: the player's own, or an offered one from the cache in `cache`,
    /// fetched first if it is not there or no longer matches its digest. A fetch tells
    /// `fetched` how many bytes have arrived as they do.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] for a file that cannot be read or written, and [`Error::SoundFont`] for a
    /// fetch that fails or a file that does not match its digest.
    pub fn fetch(&self, cache: &Path, fetched: &mut dyn FnMut(u64)) -> Result<PathBuf> {
        match self {
            Self::File(path) => {
                std::fs::metadata(path).map_err(Error::io(path))?;
                Ok(path.clone())
            }
            Self::Offered(offered) => {
                let path = cache.join(offered.file);
                if path.exists() && digest_of(&path)? == offered.digest {
                    return Ok(path);
                }
                download(offered, cache, fetched)?;
                Ok(path)
            }
        }
    }
}

/// The SHA-256 digest of the file at `path`, in hexadecimal.
fn digest_of(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(Error::io(path))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer).map_err(Error::io(path))?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Fetches `offered` into `cache`, through a partial file that takes its place only once its
/// digest matches.
fn download(offered: &Offered, cache: &Path, fetched: &mut dyn FnMut(u64)) -> Result<()> {
    std::fs::create_dir_all(cache).map_err(Error::io(cache))?;
    let url = format!("{RELEASES}/{}/{}", offered.release, offered.file);
    log::info!("fetching the sound font {} from {url}", offered.name);
    let partial = cache.join(format!("{}.part", offered.file));
    if let Err(error) = fetch_into(&url, offered, &partial, fetched) {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    let path = cache.join(offered.file);
    std::fs::rename(&partial, &path).map_err(Error::io(&path))?;
    Ok(())
}

/// Fetches `url` into `partial`, refusing a body longer than `offered`'s size and one whose
/// digest is not `offered`'s. The reader's limit is a byte over the size, since a body that
/// reaches the limit exactly is refused before its end is seen.
fn fetch_into(url: &str, offered: &Offered, partial: &Path, fetched: &mut dyn FnMut(u64)) -> Result<()> {
    let failed = |reason: String| Error::SoundFont {
        name: offered.name.to_owned(),
        reason,
    };
    let response = ureq::get(url).call().map_err(|error| failed(error.to_string()))?;
    let mut reader = response
        .into_body()
        .into_with_config()
        .limit(offered.size + 1)
        .reader();
    let mut file = std::fs::File::create(partial).map_err(Error::io(partial))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    let mut arrived = 0;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| failed(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        std::io::Write::write_all(&mut file, &buffer[..read]).map_err(Error::io(partial))?;
        arrived += read as u64;
        fetched(arrived);
    }
    let got = hex(&hasher.finalize());
    if got == offered.digest {
        Ok(())
    } else {
        Err(failed(format!(
            "the download's digest is {got}, not {}",
            offered.digest
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_names_an_offered_font_or_a_file() {
        assert_eq!(
            Bank::from_setting("GeneralUser-GS"),
            Bank::Offered(GENERALUSER_GS)
        );
        assert_eq!(
            Bank::from_setting("musescore-general"),
            Bank::Offered(MUSESCORE_GENERAL)
        );
        assert_eq!(
            Bank::from_setting("/fonts/sc55.sf2"),
            Bank::File(PathBuf::from("/fonts/sc55.sf2"))
        );
        assert_eq!(Bank::File(PathBuf::from("/no/such.sf2")).identity(), None);
        assert!(!Bank::File(PathBuf::from("/no/such.sf2")).installed(Path::new("/")));
        assert_eq!(MUSESCORE_GENERAL.megabytes(), 216);
        assert_eq!(GENERALUSER_GS.megabytes(), 32);
    }

    #[test]
    fn a_cached_font_that_does_not_match_its_digest_is_not_used() {
        let cache = std::env::temp_dir().join(format!("s2-mods-sound-font-{}", std::process::id()));
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join(GENERALUSER_GS.file), b"not a sound font").unwrap();
        assert!(!Bank::Offered(GENERALUSER_GS).installed(&cache));
        assert_ne!(
            digest_of(&cache.join(GENERALUSER_GS.file)).unwrap(),
            GENERALUSER_GS.digest
        );
        let _ = std::fs::remove_dir_all(&cache);
    }
}
