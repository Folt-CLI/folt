use super::copy::write_copy;
use super::errors::failure;
use super::errors::symlink_failure;
use super::require_single_link;
use super::{PlacementFs, temporary_path};
use crate::config::names::TEMP_TAG_CONVERT;
use crate::fsx::{FileSystem, RealFileSystem, signature};
use crate::id::{FolioId, ObjectId};
use crate::placement::{
    Observation, PlacementBase, PlacementError, PlacementKind, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;

impl PlacementFs<'_> {
    pub(super) fn convert_entry(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to_kind: PlacementKind,
        objects: &ObjectStore,
        content: ObjectId,
        expected: &Observation,
    ) -> Result<Written, PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        let temporary = temporary_path(parent, TEMP_TAG_CONVERT);
        let mut prepared = false;
        let result = (|| {
            match (placement.kind, to_kind, expected) {
                (PlacementKind::Link, PlacementKind::Copy, Observation::IntactLink) => {
                    write_copy(&temporary, objects, content)?;
                    prepared = true;
                    let output = OpenOptions::new()
                        .write(true)
                        .open(&temporary)
                        .map_err(|error| failure(&temporary, error))?;
                    let metadata = fs::metadata(&placement.path)
                        .map_err(|error| failure(&placement.path, error))?;
                    output
                        .set_permissions(metadata.permissions())
                        .map_err(|error| failure(&temporary, error))?;
                    output
                        .sync_all()
                        .map_err(|error| failure(&temporary, error))?;
                }
                (PlacementKind::Copy, PlacementKind::Link, Observation::File { .. }) => {
                    require_single_link(&placement.path)?;
                    let target = self.link_target(id);
                    fs::metadata(&target).map_err(|error| failure(&target, error))?;
                    RealFileSystem
                        .create_symlink(&target, &temporary)
                        .map_err(|error| symlink_failure(&placement.path, error))?;
                    prepared = true;
                    RealFileSystem
                        .sync_directory(parent)
                        .map_err(|error| failure(parent, error))?;
                }
                _ => {
                    return Err(failure(
                        &placement.path,
                        io::Error::other("invalid conversion"),
                    ));
                }
            }
            if self.read(id, placement)? != *expected {
                return Err(PlacementError::PlacementChanged);
            }
            if to_kind == PlacementKind::Link {
                require_single_link(&placement.path)?;
            }
            // Rename replaces the entry atomically (MoveFileExW with
            // MOVEFILE_REPLACE_EXISTING on Windows). Never unlink first: a
            // failed replacement must leave the original entry available for retry.
            fs::rename(&temporary, &placement.path)
                .map_err(|error| failure(&placement.path, error))?;
            RealFileSystem
                .sync_directory(parent)
                .map_err(|error| failure(parent, error))?;
            let base = if to_kind == PlacementKind::Copy {
                let metadata = fs::symlink_metadata(&placement.path)
                    .map_err(|error| failure(&placement.path, error))?;
                Some(PlacementBase {
                    hash: content,
                    signature: signature(&metadata),
                })
            } else {
                None
            };
            Ok(Written { base })
        })();
        if prepared {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}
