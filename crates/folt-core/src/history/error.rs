use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::folio::format::FormatError;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ref_name::LowerRefError;
use crate::store::StoreError;

#[derive(Debug)]
pub enum RefError {
    InvalidName(String),
    InvalidRef(PathBuf),
    Stale { actual: Option<ObjectId> },
    Conflict { existing: PathBuf },
    MissingTarget(ObjectId),
    WrongKind { id: ObjectId, actual: ObjectKind },
    Format(FormatError),
    Store(StoreError),
    Io { path: PathBuf, source: io::Error },
}
impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "invalid ref name: {name}"),
            Self::InvalidRef(path) => write!(f, "invalid ref at {}", path.display()),
            Self::Stale { actual } => write!(f, "stale ref; actual value: {actual:?}"),
            Self::Conflict { existing } => write!(f, "ref conflicts with {}", existing.display()),
            Self::MissingTarget(id) => write!(f, "missing ref target: {id}"),
            Self::WrongKind { id, actual } => write!(f, "invalid ref target {id}: {actual:?}"),
            Self::Format(error) => error.fmt(f),
            Self::Store(error) => error.fmt(f),
            Self::Io { path, source } => write!(f, "ref I/O at {}: {source}", path.display()),
        }
    }
}
impl Error for RefError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<LowerRefError> for RefError {
    fn from(error: LowerRefError) -> Self {
        match error {
            LowerRefError::InvalidName(name) => Self::InvalidName(name),
            LowerRefError::InvalidRef(path) => Self::InvalidRef(path),
            LowerRefError::Conflict { existing } => Self::Conflict { existing },
            LowerRefError::Io { path, source } => Self::Io { path, source },
        }
    }
}
