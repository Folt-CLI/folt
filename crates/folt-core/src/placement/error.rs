use crate::store::StoreError;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Errors returned by placement operations.
#[derive(Debug)]
pub enum PlacementError {
    HashRequired,
    PlacementChanged,
    Store(StoreError),
    OperationFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    LinkUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for PlacementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => error.fmt(formatter),
            Self::PlacementChanged => write!(formatter, "placement changed before refresh"),
            Self::HashRequired => write!(formatter, "placement content hash is required"),
            Self::OperationFailed { path, source } => {
                write!(
                    formatter,
                    "placement operation failed for {}: {source}",
                    path.display()
                )
            }
            Self::LinkUnavailable { path, source } => write!(
                formatter,
                "cannot create a symbolic link at {}: {source}. On Windows, enable Developer Mode or grant the process the Create symbolic links right; otherwise use a filesystem that supports symbolic links or choose a copy placement",
                path.display()
            ),
        }
    }
}

impl Error for PlacementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::HashRequired | Self::PlacementChanged => None,
            Self::Store(error) => Some(error),
            Self::OperationFailed { source, .. } => Some(source),
            Self::LinkUnavailable { source, .. } => Some(source),
        }
    }
}
