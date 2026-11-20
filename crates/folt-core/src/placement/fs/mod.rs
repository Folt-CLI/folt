mod convert;
mod copy;
mod errors;
mod link;

use crate::folio::dir::path::content_path;
use crate::fsx::{FileSystem, RealFileSystem, signature, temp_name};
use crate::home::Home;
use crate::id::{FolioId, ObjectId};
use crate::placement::{
    Observation, PlacementError, PlacementKind, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;
use copy::hash_file;
use errors::{failure, move_failure};
pub(crate) use link::{require_single_link, resolved_path};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Filesystem implementation of link and copy placement operations.
pub struct PlacementFs<'a> {
    home: &'a Home,
}

impl<'a> PlacementFs<'a> {
    pub fn new(home: &'a Home) -> Self {
        Self { home }
    }

    fn relocate_with_rename(
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
            PlacementKind::Link => self.create_link(&target, &placement.path),
            PlacementKind::Copy | PlacementKind::Internal => {
                self.copy_content(id, placement, objects, content, false, None)
            }
        }
    }
    fn read(&self, id: FolioId, placement: &PlacementSpec) -> Result<Observation, PlacementError> {
        let target = self.link_target(id);
        match fs::symlink_metadata(&placement.path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Observation::Missing),
            Err(error) => Err(failure(&placement.path, error)),
            Ok(metadata) if metadata.file_type().is_symlink() => self.read_link(placement, &target),
            Ok(metadata) if metadata.is_file() => Ok(Observation::File {
                signature: signature(&metadata),
            }),
            Ok(metadata) if metadata.is_dir() => Ok(Observation::Directory),
            Ok(_) => Ok(Observation::NotAFile),
        }
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
        self.normalize_link_entry(id, placement)
    }
    fn sync_parent(&self, placement: &PlacementSpec) -> Result<(), PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        RealFileSystem
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
            RealFileSystem.rename_no_replace(from, to)
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
                fs::remove_file(&placement.path).map_err(|error| failure(&placement.path, error))
            }
            _ => Err(PlacementError::PlacementChanged),
        }
    }
    fn hash_placement(&self, placement: &PlacementSpec) -> Result<ObjectId, PlacementError> {
        hash_file(&placement.path)
    }
}

fn require_absent(path: &Path) -> Result<(), PlacementError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(failure(
            path,
            io::Error::new(io::ErrorKind::AlreadyExists, "destination exists"),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failure(path, error)),
    }
}

fn temporary_path(parent: &Path, tag: &str) -> PathBuf {
    parent.join(temp_name(&format!(".{tag}-"), '-'))
}

#[cfg(test)]
mod tests;
