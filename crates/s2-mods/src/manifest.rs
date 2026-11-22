use std::path::Path;

use crate::{Error, MANIFEST, Result};

/// A mod's `mod.toml`: who it is and what it builds on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The id other mods and the settings name it by, which is also its folder's name.
    pub id: String,
    /// The name the player sees.
    pub name: String,
    /// What it adds or changes, in a sentence the player sees beside the name.
    #[serde(default)]
    pub description: String,
    /// Its own version.
    pub version: String,
    /// The version of Robigo it was written for, such as `0.9` for any 0.9 release or `0.9.3` for
    /// that one alone. A mod written for another version still loads, with a warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robigo: Option<String>,
    /// The mods it builds on, which load before it. The base mod always loads first.
    #[serde(default)]
    pub depends: Vec<String>,
}

impl Manifest {
    /// Whether the mod was written for `running`, the Robigo version playing it: each part of the
    /// version it names matches `running`'s. A mod that names none fits every version.
    #[must_use]
    pub fn fits(&self, running: &str) -> bool {
        self.robigo.as_deref().is_none_or(|wanted| {
            let mut have = running.split('.');
            wanted.split('.').all(|part| have.next() == Some(part))
        })
    }

    /// The manifest in the mod folder `dir`.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when it cannot be read, [`Error::Toml`] when it does not parse.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).map_err(Error::io(&path))?;
        toml::from_str(&text).map_err(|source| Error::Toml {
            path,
            source: Box::new(source),
        })
    }

    /// The manifest as TOML.
    ///
    /// # Errors
    ///
    /// [`Error::Write`] when it cannot be written, which a manifest of strings never meets.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self).map_err(|source| Error::Write {
            path: MANIFEST.into(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written_for(version: Option<&str>) -> Manifest {
        Manifest {
            id: "some-mod".into(),
            name: "Some mod".into(),
            description: String::new(),
            version: "1".into(),
            robigo: version.map(Into::into),
            depends: Vec::new(),
        }
    }

    #[test]
    fn a_mod_fits_the_releases_its_version_names() {
        assert!(written_for(Some("0.9")).fits("0.9.3"));
        assert!(written_for(Some("0.9.3")).fits("0.9.3"));
        assert!(!written_for(Some("0.9.3")).fits("0.9.4"));
        assert!(!written_for(Some("0.9")).fits("0.10.0"));
        assert!(!written_for(Some("1")).fits("0.9.3"));
        assert!(written_for(None).fits("0.9.3"));
    }

    #[test]
    fn a_manifest_without_the_newer_fields_still_reads() {
        let manifest: Manifest = toml::from_str("id = \"old\"\nname = \"Old\"\nversion = \"1\"\n").unwrap();
        assert_eq!(manifest.robigo, None);
        assert_eq!(manifest.description, "");
    }
}
