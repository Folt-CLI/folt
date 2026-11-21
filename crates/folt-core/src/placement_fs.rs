use std::fs;
use std::path::{Path, PathBuf};

mod convert;
mod copy;
mod helpers;
mod link;
mod observe;

use crate::config::names::{FOLIOS_DIR, TEMP_TAG_LINK_PROBE};
use crate::folio::dir::path::content_path;
use crate::fsx::{FileSystem, RealFileSystem, TempFile};
use crate::home::Home;
use crate::id::FolioId;
use crate::id::ObjectId;
use crate::placement::{
    Observation, PlacementError, PlacementKind, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;
use helpers::{failure, hash_file, symlink_failure, temporary_path};

/// Filesystem implementation of link and copy placement operations.
pub struct PlacementFs<'a> {
    home: &'a Home,
    fs: &'a dyn FileSystem,
}

impl<'a> PlacementFs<'a> {
    pub fn new(home: &'a Home) -> Self {
        Self::with_fs(home, &RealFileSystem)
    }

    pub(crate) fn with_fs(home: &'a Home, fs: &'a dyn FileSystem) -> Self {
        Self { home, fs }
    }

    /// Checks whether the path's filesystem permits creating a symbolic link.
    /// Runs only when a link is explicitly requested; a copy never calls it.
    pub fn probe_link(&self, path: &Path) -> Result<(), PlacementError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let probe_path = temporary_path(parent, TEMP_TAG_LINK_PROBE);
        let probe =
            TempFile::sibling_at(&probe_path).map_err(|source| symlink_failure(path, source))?;
        self.fs
            .create_symlink(&self.home.path().join(FOLIOS_DIR), probe.path())
            .map_err(|source| symlink_failure(path, source))?;
        probe
            .remove()
            .map_err(|source| failure(&probe_path, source))
    }
}

impl PlacementOps for PlacementFs<'_> {
    fn link_target(&self, id: FolioId) -> PathBuf {
        content_path(self.home, id)
    }

    fn create(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
    ) -> Result<Written, PlacementError> {
        let target = self.link_target(id);
        match placement.kind {
            PlacementKind::Link => {
                fs::metadata(&target).map_err(|e| failure(&target, e))?;
                self.fs
                    .create_symlink(&target, &placement.path)
                    .map_err(|e| failure(&placement.path, e))?;
                Ok(Written { base: None })
            }
            PlacementKind::Copy | PlacementKind::Internal => {
                self.copy_content(id, placement, objects, content, false, None)
            }
        }
    }

    fn read(&self, id: FolioId, placement: &PlacementSpec) -> Result<Observation, PlacementError> {
        self.observe(id, placement)
    }

    fn refresh(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        match placement.kind {
            PlacementKind::Copy | PlacementKind::Internal => {
                self.copy_content(id, placement, objects, content, true, expected)
            }
            PlacementKind::Link => self.refresh_link(id, placement, expected),
        }
    }

    fn normalize_link(&self, id: FolioId, placement: &PlacementSpec) -> Result<(), PlacementError> {
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

    fn sync_parent(&self, placement: &PlacementSpec) -> Result<(), PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        self.fs
            .sync_directory(parent)
            .map_err(|error| failure(parent, error))
    }

    fn convert(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to_kind: PlacementKind,
        objects: &ObjectStore,
        content: ObjectId,
        expected: &Observation,
    ) -> Result<Written, PlacementError> {
        self.convert_entry(id, placement, to_kind, objects, content, expected)
    }

    fn relocate(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to: &Path,
        expected: &Observation,
    ) -> Result<Written, PlacementError> {
        self.relocate_with_rename(id, placement, to, expected, |from, to| {
            self.fs.rename_no_replace(from, to)
        })
    }

    fn unlink(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        expected: &Observation,
    ) -> Result<(), PlacementError> {
        if self.read(id, placement)? != *expected {
            return Err(PlacementError::PlacementChanged);
        }
        match expected {
            Observation::IntactLink | Observation::OtherLink { .. } | Observation::File { .. } => {
                crate::fsx::remove_file(&placement.path)
                    .map_err(|error| failure(&placement.path, error))
            }
            _ => Err(PlacementError::PlacementChanged),
        }
    }

    fn hash_placement(&self, placement: &PlacementSpec) -> Result<ObjectId, PlacementError> {
        hash_file(&placement.path)
    }
}

pub(crate) fn require_single_link(path: &Path) -> Result<(), PlacementError> {
    crate::fsx::require_single_link(path).map_err(|error| failure(path, error))
}

#[cfg(test)]
mod tests;
