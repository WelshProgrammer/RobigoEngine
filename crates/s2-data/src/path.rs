//! Data paths: DOS-style, case-insensitive, `/` or `\` separated.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Splits a data path into its components, dropping empty and `.` parts.
pub(crate) fn split(path: &str) -> Result<Vec<&str>> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(invalid(path, "`..` is not allowed")),
            _ if part.contains(':') => return Err(invalid(path, "drive letters are not allowed")),
            _ => parts.push(part),
        }
    }
    Ok(parts)
}

/// The canonical form of a path, used in error messages and as a merge key.
pub(crate) fn display(parts: &[&str]) -> String {
    parts.join("/").to_ascii_uppercase()
}

/// Compares two names the way DOS does: ASCII letters ignore case.
pub(crate) fn cmp_ci(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|c| c.to_ascii_uppercase())
        .cmp(b.bytes().map(|c| c.to_ascii_uppercase()))
}

/// True when `prefix` is a leading run of `path`, ignoring ASCII case.
pub(crate) fn starts_with_ci<A: AsRef<str>, B: AsRef<str>>(path: &[A], prefix: &[B]) -> bool {
    prefix.len() <= path.len()
        && prefix
            .iter()
            .zip(path)
            .all(|(p, c)| p.as_ref().eq_ignore_ascii_case(c.as_ref()))
}

/// The folder holding `file`, as something `read_dir` accepts even for a bare file name.
pub(crate) fn folder_of(file: &Path) -> &Path {
    file.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Finds `parts` below `root` on disk, matching each component without regard to ASCII case.
///
/// An exact match wins, then the first case-insensitive match in byte order. The result does
/// not depend on the order the operating system lists the folder in.
pub(crate) fn find_on_disk<S: AsRef<str>>(root: &Path, parts: &[S]) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for part in parts {
        let part = part.as_ref();
        let exact = at.join(part);
        if exact.symlink_metadata().is_ok() {
            at = exact;
            continue;
        }
        let found = std::fs::read_dir(&at)
            .ok()?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.eq_ignore_ascii_case(part))
            .min()?;
        at.push(found);
    }
    Some(at)
}

fn invalid(path: &str, reason: &'static str) -> Error {
    Error::InvalidPath {
        path: path.to_owned(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_both_separators() {
        assert_eq!(
            split("DATA/TXT\\names.eng").unwrap(),
            ["DATA", "TXT", "names.eng"]
        );
        assert_eq!(split("\\DATA//./x").unwrap(), ["DATA", "x"]);
        let found = split("").unwrap();
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn rejects_escapes() {
        assert!(matches!(split("DATA/../x"), Err(Error::InvalidPath { .. })));
        assert!(matches!(split("C:\\S2\\S2.EXE"), Err(Error::InvalidPath { .. })));
    }

    #[test]
    fn finds_files_on_disk_without_case() {
        let dir = crate::testutil::TempDir::new();
        dir.write("Data/TXT/names.eng", b"x");
        let found = find_on_disk(dir.path(), &["DATA", "txt", "NAMES.ENG"]).unwrap();
        assert_eq!(std::fs::read(found).unwrap(), b"x");
        assert_eq!(find_on_disk(dir.path(), &["DATA", "missing"]), None);
        assert_eq!(
            find_on_disk(dir.path(), &[] as &[&str]),
            Some(dir.path().to_owned())
        );
    }

    #[test]
    fn compares_without_case() {
        assert_eq!(cmp_ci("data", "DATA"), Ordering::Equal);
        assert_eq!(cmp_ci("A", "b"), Ordering::Less);
        assert!(starts_with_ci(&["s2", "Root", "S2.EXE"], &["S2", "ROOT"]));
        assert!(!starts_with_ci(&["S2"], &["S2", "ROOT"]));
    }
}
