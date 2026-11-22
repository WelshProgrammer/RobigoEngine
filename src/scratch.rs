//! A test's own folder under the system temp folder, removed once the test is done with it,
//! whether it passed or failed.

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A test's own empty folder, removed on drop.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// The folder `robigo-<name>-<process id>`, emptied.
    pub(crate) fn new(name: &str) -> Self {
        let folder = std::env::temp_dir().join(format!("robigo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        Self(folder)
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
