use std::fs;
use std::io;
use std::path::Path;

use super::PlacementFs;
use super::helpers::{create_temporary_file, failure, write_copy};
use crate::config::names::TEMP_TAG_COPY;
use crate::fsx::{copy_new, hard_links_unsupported, signature};
use crate::id::{FolioId, ObjectId};
use crate::placement::{
    Observation, PlacementBase, PlacementError, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;

impl PlacementFs<'_> {
    pub(super) fn copy_content(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
        replace: bool,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary = create_temporary_file(parent, TEMP_TAG_COPY)?;
        write_copy(&mut temporary, objects, content)?;
        // Defer destination errors so a changed observation is reported as such.
        let prepare = if replace {
            match fs::symlink_metadata(&placement.path) {
                Ok(existing) if existing.is_file() => {
                    crate::fsx::set_permissions(temporary.path(), existing.permissions())
                        .map_err(|error| failure(temporary.path(), error))
                }
                Ok(_) => Err(failure(
                    &placement.path,
                    io::Error::other("cannot refresh a copy placement over a non-regular file"),
                )),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(failure(&placement.path, error)),
            }
        } else {
            Ok(())
        };
        if !replace {
            if let Err(error) = fs::symlink_metadata(&placement.path) {
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(failure(&placement.path, error));
                }
            } else {
                return Err(failure(
                    &placement.path,
                    io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "placement path already exists",
                    ),
                ));
            }
        }
        let publish = if replace {
            if let Some(expected) = expected {
                match self.read(id, placement) {
                    Ok(observation) if &observation == expected => {}
                    Ok(_) => return Err(PlacementError::PlacementChanged),
                    Err(error) => return Err(error),
                }
            }
            prepare?;
            temporary.publish(&placement.path)
        } else {
            let linked = match crate::fsx::hard_link(temporary.path(), &placement.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
                Err(error) if hard_links_unsupported(&error) => {
                    copy_new(temporary.path(), &placement.path)
                }
                Err(error) => Err(error),
            };
            // The temporary name is gone before the placement is read, so its link count is one.
            drop(temporary);
            linked
        };
        if let Err(error) = publish {
            return Err(failure(&placement.path, error));
        }
        let metadata = fs::symlink_metadata(&placement.path)
            .map_err(|error| failure(&placement.path, error))?;
        Ok(Written {
            base: Some(PlacementBase {
                hash: content,
                signature: signature(&metadata),
            }),
        })
    }
}
