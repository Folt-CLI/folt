use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;

use super::PlacementFs;
use super::helpers::{create_temporary_file, failure, symlink_failure, temporary_path, write_copy};
use crate::config::names::TEMP_TAG_CONVERT;
use crate::fsx::{TempFile, signature};
use crate::id::{FolioId, ObjectId};
use crate::placement::{
    Observation, PlacementBase, PlacementError, PlacementKind, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;

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
        let temporary_path = temporary_path(parent, TEMP_TAG_CONVERT);
        (|| {
            let temporary = match (placement.kind, to_kind, expected) {
                (PlacementKind::Link, PlacementKind::Copy, Observation::IntactLink) => {
                    let mut temporary = create_temporary_file(parent, TEMP_TAG_CONVERT)?;
                    write_copy(&mut temporary, objects, content)?;
                    let output = OpenOptions::new()
                        .write(true)
                        .open(temporary.path())
                        .map_err(|error| failure(temporary.path(), error))?;
                    let metadata = fs::metadata(&placement.path)
                        .map_err(|error| failure(&placement.path, error))?;
                    crate::fsx::set_file_permissions(&output, metadata.permissions())
                        .map_err(|error| failure(temporary.path(), error))?;
                    crate::fsx::sync_file(&output)
                        .map_err(|error| failure(temporary.path(), error))?;
                    temporary
                }
                (PlacementKind::Copy, PlacementKind::Link, Observation::File { .. }) => {
                    self.fs
                        .link_count(&placement.path)
                        .and_then(crate::fsx::ensure_single)
                        .map_err(|error| failure(&placement.path, error))?;
                    let target = self.link_target(id);
                    fs::metadata(&target).map_err(|error| failure(&target, error))?;
                    let temporary = TempFile::sibling_at(&temporary_path)
                        .map_err(|error| symlink_failure(&placement.path, error))?;
                    self.fs
                        .create_symlink(&target, temporary.path())
                        .map_err(|error| symlink_failure(&placement.path, error))?;
                    self.fs
                        .sync_directory(parent)
                        .map_err(|error| failure(parent, error))?;
                    temporary
                }
                _ => {
                    return Err(failure(
                        &placement.path,
                        io::Error::other("invalid conversion"),
                    ));
                }
            };
            if self.read(id, placement)? != *expected {
                return Err(PlacementError::PlacementChanged);
            }
            if to_kind == PlacementKind::Link {
                self.fs
                    .link_count(&placement.path)
                    .and_then(crate::fsx::ensure_single)
                    .map_err(|error| failure(&placement.path, error))?;
            }
            // Rename replaces the entry atomically (MoveFileExW with
            // MOVEFILE_REPLACE_EXISTING on Windows). Never unlink first: a
            // failed replacement must leave the original entry available for retry.
            temporary
                .publish(&placement.path)
                .map_err(|error| failure(&placement.path, error))?;
            self.fs
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
        })()
    }
}
