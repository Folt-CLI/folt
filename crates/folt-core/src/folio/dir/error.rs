use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::layout::LayoutVersion;

pub(super) fn io_error(path: &Path, source: io::Error) -> FolioDirError {
    FolioDirError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Errors returned by Folio directory operations.
#[derive(Debug)]
pub enum FolioDirError {
    AlreadyExists(PathBuf),
    InvalidDirectory(PathBuf),
    Newer {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}
impl fmt::Display for FolioDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists(path) => write!(f, "Folio already exists: {}", path.display()),
            Self::InvalidDirectory(path) => {
                write!(f, "invalid Folio directory: {}", path.display())
            }
            Self::Newer { found, supported } => write!(
                f,
                "Folio format {found} is newer than supported format {supported}"
            ),
            Self::Io { path, source } => write!(
                f,
                "Folio directory operation failed at {}: {source}",
                path.display()
            ),
        }
    }
}
impl Error for FolioDirError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
