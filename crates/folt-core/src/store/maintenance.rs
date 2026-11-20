use std::fs;

use crate::config::names::{TEMP_PREFIX_INPUT, TEMP_PREFIX_OBJECT};
use crate::home::HomeLock;

use super::{ObjectStore, StoreError};

impl ObjectStore {
    pub fn sync_blobs(&self) -> Result<(), StoreError> {
        self.fs
            .sync_directory(&self.root)
            .map_err(|source| StoreError::Io {
                path: self.root.clone(),
                source,
            })
    }

    /// Removes abandoned staging files with exclusive access to the home lock guard.
    /// Call once before a batch of puts.
    pub fn clean_temporary_files(&self, _lock: &mut HomeLock) -> Result<(), StoreError> {
        for entry in fs::read_dir(&self.root).map_err(|source| StoreError::Io {
            path: self.root.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| StoreError::Io {
                path: self.root.clone(),
                source,
            })?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(TEMP_PREFIX_INPUT) || name.starts_with(TEMP_PREFIX_OBJECT) {
                crate::fsx::remove_file(entry.path()).map_err(|source| StoreError::Io {
                    path: entry.path(),
                    source,
                })?;
            }
        }
        Ok(())
    }
}
