use super::errors::failure;
use super::{PlacementFs, temporary_path};
use crate::config::limits::STREAM_BUFFER_SIZE;
use crate::config::names::TEMP_TAG_COPY;
use crate::fsx::{copy_new, hard_links_unsupported, signature};
use crate::id::{FolioId, ObjectId};
use crate::object::{ObjectHasher, ObjectKind};
use crate::placement::{
    Observation, PlacementBase, PlacementError, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

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
        let temporary = temporary_path(parent, TEMP_TAG_COPY);
        write_copy(&temporary, objects, content)?;
        // Defer destination errors so a changed observation is reported as such.
        let prepare = if replace {
            match fs::symlink_metadata(&placement.path) {
                Ok(existing) if existing.is_file() => {
                    fs::set_permissions(&temporary, existing.permissions())
                        .map_err(|error| failure(&temporary, error))
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
                    let _ = fs::remove_file(&temporary);
                    return Err(failure(&placement.path, error));
                }
            } else {
                let _ = fs::remove_file(&temporary);
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
                    Ok(_) => {
                        let _ = fs::remove_file(&temporary);
                        return Err(PlacementError::PlacementChanged);
                    }
                    Err(error) => {
                        let _ = fs::remove_file(&temporary);
                        return Err(error);
                    }
                }
            }
            if let Err(error) = prepare {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
            fs::rename(&temporary, &placement.path)
        } else {
            match fs::hard_link(&temporary, &placement.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
                Err(error) if hard_links_unsupported(&error) => {
                    copy_new(&temporary, &placement.path)
                }
                Err(error) => Err(error),
            }
        };
        if let Err(error) = publish {
            let _ = fs::remove_file(&temporary);
            return Err(failure(&placement.path, error));
        }
        let _ = fs::remove_file(&temporary);
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

pub(super) fn write_copy(
    path: &Path,
    objects: &ObjectStore,
    content: ObjectId,
) -> Result<(), PlacementError> {
    let mut output = create_temporary_file(path)?;
    let result = (|| {
        objects
            .get_to(content, ObjectKind::Blob, &mut output)
            .map_err(PlacementError::Store)?;
        output.flush().map_err(|error| failure(path, error))?;
        output.sync_all().map_err(|error| failure(path, error))
    })();
    drop(output);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn create_temporary_file(path: &Path) -> Result<File, PlacementError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| failure(path, error))
}

pub(super) fn hash_file(path: &Path) -> Result<ObjectId, PlacementError> {
    let metadata = fs::metadata(path).map_err(|e| failure(path, e))?;
    let expected = metadata.len();
    let mut input = File::open(path).map_err(|e| failure(path, e))?;
    let mut hasher = ObjectHasher::new(ObjectKind::Blob, expected);
    let mut buffer = vec![0; STREAM_BUFFER_SIZE];
    let mut total = 0u64;
    loop {
        let count = input.read(&mut buffer).map_err(|e| failure(path, e))?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| failure(path, io::Error::other("content length overflow")))?;
        hasher.update(&buffer[..count]);
    }
    if total != expected {
        return Err(failure(
            path,
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "placement content changed while hashing",
            ),
        ));
    }
    Ok(hasher.finish())
}
