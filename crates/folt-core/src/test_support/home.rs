//! Unique temporary directories and Folt homes, removed on drop.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::home::Home;
#[cfg(test)]
use crate::placement::{Placement, PlacementKind};

/// Owns a unique temporary directory and removes it on drop.
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new() -> io::Result<Self> {
        loop {
            let path = std::env::temp_dir().join(format!("folt-test-{}", uuid::Uuid::new_v4()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    let mut directory = Self { path };
                    directory.path = fs::canonicalize(&directory.path)?;
                    return Ok(directory);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Owns a Folt home beneath a unique temporary root.
pub(crate) struct TempHome {
    pub(crate) root: PathBuf,
    pub(crate) home: Home,
    pub(crate) _directory: TempDir,
}

impl TempHome {
    pub(crate) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory = TempDir::new()?;
        let root = directory.path().to_path_buf();
        let home = Home::open_or_create(root.join("home"))?;
        Ok(Self {
            root,
            home,
            _directory: directory,
        })
    }
}

#[cfg(test)]
impl TempHome {
    pub(crate) fn source(&self, directory: bool) -> Placement {
        let path = self.root.join("source");
        if directory {
            fs::create_dir(&path).unwrap();
        } else {
            fs::write(&path, b"source").unwrap();
        }
        Placement {
            path,
            kind: PlacementKind::Copy,
            head: "main".into(),
            commit_id: None,
            base: None,
        }
    }
    pub(crate) fn folio_count(&self) -> usize {
        fs::read_dir(self.home.path().join("folios"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() != ".new")
            .count()
    }
    pub(crate) fn dir(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir(&path).unwrap();
        path
    }
}
