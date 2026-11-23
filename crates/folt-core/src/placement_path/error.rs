use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use super::PlacementPath;
use crate::folio::name::FolioName;
use crate::id::FolioId;

/// Errors returned by registry operations.
#[derive(Debug)]
pub enum RegistryError {
    References(String),
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    UnsupportedSchemaVersion {
        found: i64,
        supported: i64,
    },
    OutdatedSchema {
        found: i64,
        required: i64,
        path: PathBuf,
    },
    ForeignDatabase(PathBuf),
    InvalidPath(PathBuf),
    InvalidId,
    InvalidName(String),
    InvalidPendingRename(FolioId),
    NameTaken(FolioName),
    FolioMissing(FolioId),
    NotNamed(FolioId),
    InvalidLayout {
        id: FolioId,
        text: String,
    },
    MissingDatabase(PathBuf),
    NotRegularFile(PathBuf),
    FolioHasPlacements(FolioId),
    SizeOutOfRange(u64),
    InvalidPlacementKind(String),
    InvalidCommitId,
    InvalidBaseHash,
    InvalidBaseSignature,
    InvalidStaged,
    InvalidConflictState,
    PlacementChanged(PlacementPath),
}
impl From<rusqlite::Error> for RegistryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}
impl From<std::io::Error> for RegistryError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "registry database error: {e}"),
            Self::Io(e) => write!(f, "registry filesystem error: {e}"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "registry schema version {found} is newer than supported version {supported}"
            ),
            Self::OutdatedSchema {
                found,
                required,
                path,
            } => write!(
                f,
                "registry {} uses pre-release schema version {found}; required version is {required}; delete this registry before continuing",
                path.display()
            ),
            Self::ForeignDatabase(path) => {
                write!(f, "not a Folt registry database: {}", path.display())
            }
            Self::InvalidPath(path) => write!(f, "invalid placement path: {}", path.display()),
            Self::InvalidPendingRename(id) => write!(
                f,
                "registry contains an invalid pending rename for Folio {id}"
            ),
            Self::InvalidName(text) => {
                write!(f, "registry contains an invalid Folio name: {text:?}")
            }
            Self::NameTaken(name) => write!(f, "Folio name is already taken: {name}"),
            Self::FolioMissing(id) => write!(f, "Folio does not exist: {id}"),
            Self::References(message) => f.write_str(message),
            Self::NotNamed(id) => write!(f, "Folio has no name to rename: {id}"),
            Self::InvalidId => write!(f, "registry contains an invalid Folio id"),
            Self::InvalidLayout { id, text } => {
                write!(
                    f,
                    "registry contains invalid layout version '{text}' for Folio {id}"
                )
            }
            Self::MissingDatabase(path) => {
                write!(f, "registry database does not exist: {}", path.display())
            }
            Self::NotRegularFile(path) => {
                write!(
                    f,
                    "registry database is not a regular file: {}",
                    path.display()
                )
            }
            Self::FolioHasPlacements(_) => {
                write!(f, "cannot remove a Folio while placements refer to it")
            }
            Self::SizeOutOfRange(size) => {
                write!(f, "placement size {size} exceeds SQLite integer range")
            }
            Self::InvalidPlacementKind(kind) => {
                write!(f, "registry contains an invalid placement kind: {kind}")
            }
            Self::InvalidCommitId => write!(f, "registry contains an invalid placement commit id"),
            Self::InvalidBaseHash => write!(f, "registry contains an invalid placement base hash"),
            Self::InvalidConflictState => write!(f, "registry contains an invalid conflict state"),
            Self::InvalidStaged => write!(f, "registry contains an invalid staged blob"),
            Self::PlacementChanged(path) => write!(
                f,
                "registry placement changed: {}",
                path.to_path_buf().display()
            ),
            Self::InvalidBaseSignature => {
                write!(f, "registry contains an invalid placement signature")
            }
        }
    }
}
impl Error for RegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
