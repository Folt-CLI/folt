use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::config::limits::STREAM_BUFFER_SIZE;
use crate::fsx::{TempFile, TempMode, temp_name};
use crate::id::ObjectId;
use crate::object::{ObjectHasher, ObjectKind};
use crate::placement::PlacementError;
use crate::store::ObjectStore;

pub(super) fn move_failure(path: &Path, source: io::Error) -> PlacementError {
    if source.kind() == io::ErrorKind::CrossesDevices {
        failure(
            path,
            io::Error::new(
                io::ErrorKind::CrossesDevices,
                "moves across filesystems are not supported",
            ),
        )
    } else {
        failure(path, source)
    }
}

pub(super) fn require_absent(path: &Path) -> Result<(), PlacementError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(failure(
            path,
            io::Error::new(io::ErrorKind::AlreadyExists, "destination exists"),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failure(path, error)),
    }
}

pub(super) fn create_temporary_file(parent: &Path, tag: &str) -> Result<TempFile, PlacementError> {
    let path = temporary_path(parent, tag);
    TempFile::create_at(&path, TempMode::Default).map_err(|error| failure(&path, error))
}

/// Writes the object into the temporary file and closes it; the guard removes it on failure.
pub(super) fn write_copy(
    temporary: &mut TempFile,
    objects: &ObjectStore,
    content: ObjectId,
) -> Result<(), PlacementError> {
    let path = temporary.path().to_path_buf();
    let output = temporary.file_mut();
    objects
        .get_to(content, ObjectKind::Blob, &mut *output)
        .map_err(PlacementError::Store)?;
    output.flush().map_err(|error| failure(&path, error))?;
    crate::fsx::sync_file(output).map_err(|error| failure(&path, error))?;
    temporary.close();
    Ok(())
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

pub(super) fn temporary_path(parent: &Path, tag: &str) -> PathBuf {
    parent.join(temp_name(&format!(".{tag}-"), '-'))
}

/// Windows privilege-not-held error code (dimensionless).
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

pub(super) fn symlink_failure(path: &Path, source: io::Error) -> PlacementError {
    let unavailable = source.kind() == io::ErrorKind::Unsupported
        || cfg!(windows)
            && (source.kind() == io::ErrorKind::PermissionDenied
                || source.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD));
    if unavailable {
        PlacementError::LinkUnavailable {
            path: path.to_path_buf(),
            source,
        }
    } else {
        failure(path, source)
    }
}

pub(super) fn failure(path: &Path, source: io::Error) -> PlacementError {
    PlacementError::OperationFailed {
        path: path.to_path_buf(),
        source,
    }
}
