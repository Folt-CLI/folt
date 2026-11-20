use crate::placement::PlacementError;
use std::io;
use std::path::Path;

/// Windows privilege-not-held error code (dimensionless).
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

pub(super) fn failure(path: &Path, source: io::Error) -> PlacementError {
    PlacementError::OperationFailed {
        path: path.to_path_buf(),
        source,
    }
}

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
