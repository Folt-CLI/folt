use std::fs;
use std::io;
use std::path::Path;

use super::PlacementFs;
use super::helpers::{failure, move_failure, require_absent, temporary_path};
use crate::fsx::TempFile;
use crate::id::FolioId;
use crate::placement::{Observation, PlacementError, PlacementOps, PlacementSpec, Written};

impl PlacementFs<'_> {
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

    pub(super) fn rewrite_link(
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
        let link_path = temporary_path(
            placement.path.parent().unwrap_or_else(|| Path::new(".")),
            "folt-link",
        );
        let temporary = TempFile::sibling_at(&link_path).map_err(|e| failure(&link_path, e))?;
        self.fs
            .create_symlink(&target, temporary.path())
            .map_err(|e| failure(&link_path, e))?;
        if let Some(expected) = expected
            && self.read(id, placement)? != *expected
        {
            return Err(PlacementError::PlacementChanged);
        }
        temporary
            .publish(&placement.path)
            .map_err(|e| failure(&placement.path, e))?;
        Ok(Written { base: None })
    }

    pub(super) fn relocate_with_rename(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to: &Path,
        expected: &Observation,
        rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<Written, PlacementError> {
        if self.read(id, placement)? != *expected {
            return Err(PlacementError::PlacementChanged);
        }
        require_absent(to)?;
        rename(&placement.path, to).map_err(|error| move_failure(to, error))?;
        Ok(Written {
            base: placement.base.clone(),
        })
    }
}
