//! Placement path types and `RegistryError` live together because they need each other.
//! This module imports neither `registry` nor `home`.

use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::env::Environment;
use crate::config::names::MARKER_FILE;
use crate::folio::name::FolioName;
use crate::id::FolioId;
use crate::placement::PlacementError;

/// A path whose filesystem spelling has been resolved before registry access.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlacementPath(pub(crate) Vec<u8>);

impl PlacementPath {
    /// Resolves a path to an absolute key without following its final component.
    /// A Folio directory resolves to its marker record.
    pub fn resolve(path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        resolve(
            path.as_ref(),
            ResolveMode::Resolve {
                append_marker: true,
            },
        )
        .map(|path| Self(path_bytes(&path)))
    }

    /// Resolves a directory itself, without appending a Folio record name.
    pub fn resolve_directory(path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        resolve(
            path.as_ref(),
            ResolveMode::Resolve {
                append_marker: false,
            },
        )
        .map(|path| Self(path_bytes(&path)))
    }

    pub(crate) fn replace_prefix(&self, old: &Self, new: &Self) -> Self {
        let mut path = new.0.clone();
        path.extend_from_slice(&self.0[old.0.len()..]);
        Self(path)
    }

    pub(crate) fn from_stored(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(self.0.clone()).into()
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let wide: Vec<_> = self
                .0
                .as_chunks::<2>()
                .0
                .iter()
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                .collect();
            std::ffi::OsString::from_wide(&wide).into()
        }
    }
}

/// A resolved absolute directory prefix used to query or move placements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlacementPrefix(pub(crate) Vec<u8>);

impl From<&PlacementPath> for PlacementPrefix {
    fn from(path: &PlacementPath) -> Self {
        Self(path.0.clone())
    }
}

/// Makes an input absolute without resolving its filesystem spelling.
pub(crate) fn absolute(input: &Path) -> std::io::Result<PathBuf> {
    if input.is_absolute() {
        Ok(input.to_path_buf())
    } else {
        Ok(Environment::current_dir()?.join(input))
    }
}

enum ResolveMode {
    Resolve { append_marker: bool },
    ResolveExistingAncestor,
}

/// Resolves existing ancestors even when a former home has been moved or removed.
pub(crate) fn resolve_existing_ancestor(input: &Path) -> Result<PathBuf, PlacementError> {
    resolve(input, ResolveMode::ResolveExistingAncestor).map_err(|error| {
        PlacementError::OperationFailed {
            path: input.to_path_buf(),
            source: match error {
                RegistryError::Io(source) => source,
                error => std::io::Error::other(error),
            },
        }
    })
}

fn resolve(input: &Path, mode: ResolveMode) -> Result<PathBuf, RegistryError> {
    let absolute = absolute(input)?;
    let append_marker = match mode {
        ResolveMode::Resolve { append_marker } => append_marker,
        ResolveMode::ResolveExistingAncestor => {
            let mut ancestor = absolute.as_path();
            let mut missing = Vec::new();
            let mut missing_error = None;
            loop {
                match fs::canonicalize(ancestor) {
                    Ok(mut resolved) => {
                        // Windows can report NotFound beneath a regular file.
                        // Only a directory can contain the missing names; keep
                        // the original filesystem error when it cannot.
                        if let Some(error) = missing_error
                            && !fs::metadata(&resolved)?.is_dir()
                        {
                            return Err(RegistryError::Io(error));
                        }
                        for name in missing.iter().rev() {
                            resolved.push(name);
                        }
                        return Ok(resolved);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        let Some(name) = ancestor.file_name() else {
                            return Err(error.into());
                        };
                        missing.push(name);
                        let Some(parent) = ancestor.parent() else {
                            return Err(error.into());
                        };
                        ancestor = parent;
                        missing_error.get_or_insert(error);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
    };
    let metadata = fs::symlink_metadata(&absolute);
    let candidate = match metadata {
        Ok(metadata) if metadata.is_dir() && append_marker => absolute.join(MARKER_FILE),
        _ => absolute,
    };
    let parent = candidate
        .parent()
        .ok_or_else(|| RegistryError::InvalidPath(candidate.clone()))?;
    let name = candidate
        .file_name()
        .ok_or_else(|| RegistryError::InvalidPath(candidate.clone()))?;
    let canonical_parent = fs::canonicalize(parent)?;
    let resolved_name = on_disk_spelling(&canonical_parent, name)?;
    let resolved = canonical_parent.join(resolved_name);
    Ok(resolved)
}

pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
}
fn on_disk_spelling(parent: &Path, name: &OsStr) -> Result<PathBuf, RegistryError> {
    let name_path = Path::new(name);
    let Some(text) = name.to_str() else {
        return Ok(name_path.to_path_buf());
    };
    let has_letter = text.chars().any(char::is_alphabetic);
    if !has_letter || !case_insensitive_probe(parent, text) {
        return Ok(name_path.to_path_buf());
    }
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(text)
        {
            return Ok(entry.file_name().into());
        }
    }
    Ok(name_path.to_path_buf())
}
fn case_insensitive_probe(parent: &Path, name: &str) -> bool {
    let flipped: String = name
        .chars()
        .map(|character| {
            if character.is_lowercase() {
                character.to_uppercase().next().unwrap_or(character)
            } else if character.is_uppercase() {
                character.to_lowercase().next().unwrap_or(character)
            } else {
                character
            }
        })
        .collect();
    flipped != name && fs::symlink_metadata(parent.join(flipped)).is_ok()
}

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

#[cfg(test)]
mod tests;
