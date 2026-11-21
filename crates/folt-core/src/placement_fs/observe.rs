use std::fs;
use std::io;
use std::path::Path;

use super::PlacementFs;
use super::helpers::failure;
use crate::fsx::signature;
use crate::id::FolioId;
use crate::placement::{
    Observation, PlacementError, PlacementOps, PlacementSpec, resolve_existing_ancestor,
};

impl PlacementFs<'_> {
    pub(super) fn observe(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
    ) -> Result<Observation, PlacementError> {
        let target = self.link_target(id);
        let metadata = match fs::symlink_metadata(&placement.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Observation::Missing);
            }
            Err(error) => return Err(failure(&placement.path, error)),
        };
        if metadata.file_type().is_symlink() {
            return match fs::read_link(&placement.path) {
                Ok(found) => {
                    let found_path = placement
                        .path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(&found);
                    // Resolving a detached target can fail (for example, a file
                    // used as a directory). The link itself is still observable.
                    let intact = match (
                        resolve_existing_ancestor(&found_path),
                        resolve_existing_ancestor(&target),
                    ) {
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
            };
        }
        if metadata.is_file() {
            return Ok(Observation::File {
                signature: signature(&metadata),
            });
        }
        Ok(if metadata.is_dir() {
            Observation::Directory
        } else {
            Observation::NotAFile
        })
    }
}
