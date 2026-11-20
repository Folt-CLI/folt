use super::errors::{failure, symlink_failure};
use super::{PlacementFs, temporary_path};
use crate::config::names::{FOLIOS_DIR, TEMP_TAG_LINK_PROBE};
use crate::fsx::{FileSystem, RealFileSystem};
use crate::id::FolioId;
use crate::placement::{Observation, PlacementError, PlacementOps, PlacementSpec, Written};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

impl PlacementFs<'_> {
    pub(super) fn create_link(
        &self,
        target: &Path,
        path: &Path,
    ) -> Result<Written, PlacementError> {
        fs::metadata(target).map_err(|e| failure(target, e))?;
        RealFileSystem
            .create_symlink(target, path)
            .map_err(|e| failure(path, e))?;
        Ok(Written { base: None })
    }

    /// Checks whether the path's filesystem permits creating a symbolic link.
    /// Runs only when a link is explicitly requested; a copy never calls it.
    pub fn probe_link(&self, path: &Path) -> Result<(), PlacementError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let probe = temporary_path(parent, TEMP_TAG_LINK_PROBE);
        RealFileSystem
            .create_symlink(&self.home.path().join(FOLIOS_DIR), &probe)
            .map_err(|source| symlink_failure(path, source))?;
        fs::remove_file(&probe).map_err(|source| failure(&probe, source))
    }

    pub(super) fn refresh_link(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        let target = self.link_target(id);
        fs::metadata(&target).map_err(|e| failure(&target, e))?;
        self.rewrite_link(id, placement, expected)
    }

    fn rewrite_link(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        let target = self.link_target(id);
        match fs::symlink_metadata(&placement.path) {
            Ok(existing) if existing.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(failure(
                    &placement.path,
                    io::Error::other("cannot refresh a link placement over a non-symlink"),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure(&placement.path, error)),
        }
        let temporary = temporary_path(
            placement.path.parent().unwrap_or_else(|| Path::new(".")),
            "folt-link",
        );
        RealFileSystem
            .create_symlink(&target, &temporary)
            .map_err(|e| failure(&temporary, e))?;
        if let Some(expected) = expected
            && self.read(id, placement)? != *expected
        {
            let _ = fs::remove_file(&temporary);
            return Err(PlacementError::PlacementChanged);
        }
        fs::rename(&temporary, &placement.path).map_err(|e| failure(&placement.path, e))?;
        Ok(Written { base: None })
    }

    pub(super) fn normalize_link_entry(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
    ) -> Result<(), PlacementError> {
        if self.read(id, placement)? != Observation::IntactLink {
            return Ok(());
        }
        let target =
            fs::read_link(&placement.path).map_err(|error| failure(&placement.path, error))?;
        if !target.is_absolute() {
            self.rewrite_link(id, placement, Some(&Observation::IntactLink))?;
        }
        Ok(())
    }

    pub(super) fn read_link(
        &self,
        placement: &PlacementSpec,
        target: &Path,
    ) -> Result<Observation, PlacementError> {
        match fs::read_link(&placement.path) {
            Ok(found) => {
                let found_path = placement
                    .path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(&found);
                // Resolving a detached target can fail (for example, a file
                // used as a directory). The link itself is still observable.
                let intact = match (resolved_path(&found_path), resolved_path(target)) {
                    (Ok(found), Ok(expected)) => found == expected,
                    _ => false,
                };
                Ok(if intact {
                    Observation::IntactLink
                } else {
                    Observation::OtherLink { target: found }
                })
            }
            Err(error) => Err(failure(&placement.path, error)),
        }
    }
}

/// Resolve existing ancestors even when a former home has been moved or removed.
pub(crate) fn resolved_path(path: &Path) -> Result<PathBuf, PlacementError> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut resolved) => {
                for name in missing.iter().rev() {
                    resolved.push(name);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = ancestor.file_name() else {
                    return Err(failure(path, error));
                };
                missing.push(name);
                ancestor = ancestor.parent().ok_or_else(|| failure(path, error))?;
            }
            Err(error) => return Err(failure(path, error)),
        }
    }
}

pub(crate) fn require_single_link(path: &Path) -> Result<(), PlacementError> {
    crate::fsx::require_single_link(path).map_err(|error| failure(path, error))
}
