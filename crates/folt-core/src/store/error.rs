use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::layout::LayoutVersion;
use crate::object::ObjectError;

#[derive(Debug)]
pub enum StoreError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Object(ObjectError),
    NotRegularFile(PathBuf),
    NewerFormat {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    OlderFormat {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    UnsupportedAlgorithm(String),
    SourceChanged,
}
impl From<ObjectError> for StoreError {
    fn from(e: ObjectError) -> Self {
        Self::Object(e)
    }
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "object store I/O at {}: {source}", path.display())
            }
            Self::Object(e) => e.fmt(f),
            Self::NotRegularFile(path) => {
                write!(f, "object blob is not a regular file: {}", path.display())
            }
            Self::NewerFormat { found, supported } => write!(
                f,
                "Folio format {found} is newer than supported format {supported}"
            ),
            Self::OlderFormat { found, supported } => write!(
                f,
                "Folio format {found} is older than supported format {supported}"
            ),
            Self::UnsupportedAlgorithm(a) => write!(f, "unsupported object algorithm: {a}"),
            Self::SourceChanged => write!(f, "object source changed while being stored"),
        }
    }
}
impl Error for StoreError {}
