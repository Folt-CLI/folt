use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::config::names::REFS_DIR;
use crate::folio::format::{FolioFormat, FormatError, check_format};
use crate::folio::refs_dir::{
    atomic_write, check_directory, clean_directory, io_error, metadata, read_value,
    sync_directories, walk,
};
use crate::folio_dir::FolioDir;
use crate::fsx::{FileSystem, RealFileSystem};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ref_name::{LowerRefError, RefKind, RefName};
use crate::store::{ObjectStore, StoreError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expect {
    Absent,
    Is(ObjectId),
    Any,
}
impl Expect {
    fn check(self, actual: Option<ObjectId>) -> Result<(), RefError> {
        if match self {
            Self::Absent => actual.is_none(),
            Self::Is(old) => actual == Some(old),
            Self::Any => true,
        } {
            Ok(())
        } else {
            Err(RefError::Stale { actual })
        }
    }
}

/// Loose refs belonging to one Folio.
pub struct RefStore {
    fs: Box<dyn FileSystem>,
    root: PathBuf,
    objects: ObjectStore,
}
impl RefStore {
    pub fn open(folio: &FolioDir, format: &FolioFormat) -> Result<Self, RefError> {
        Self::open_with(folio, format, Box::new(RealFileSystem))
    }

    pub(crate) fn open_with(
        folio: &FolioDir,
        format: &FolioFormat,
        fs: Box<dyn FileSystem>,
    ) -> Result<Self, RefError> {
        check_format(format).map_err(RefError::Format)?;
        Ok(Self {
            fs,
            root: folio.path().to_path_buf(),
            objects: ObjectStore::open(folio, format).map_err(RefError::Store)?,
        })
    }
    fn path(&self, kind: RefKind, name: &RefName) -> Result<PathBuf, RefError> {
        kind.name(name.as_str())?;
        let mut path = self.root.clone();
        check_directory(&path)?;
        for component in [REFS_DIR, kind.directory()] {
            path.push(component);
            check_directory(&path)?;
        }
        let parts: Vec<_> = name.as_str().split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            path.push(part);
            match metadata(&path)? {
                Some(meta) if meta.is_dir() && index + 1 == parts.len() => {
                    return Err(RefError::Conflict { existing: path });
                }
                Some(meta) if !meta.file_type().is_file() && !meta.file_type().is_dir() => {
                    return Err(RefError::InvalidRef(path));
                }
                Some(meta) if meta.is_file() && index + 1 != parts.len() => {
                    return Err(RefError::Conflict { existing: path });
                }
                _ => {}
            }
        }
        Ok(path)
    }
    pub fn read(&self, kind: RefKind, name: &RefName) -> Result<Option<ObjectId>, RefError> {
        let path = self.path(kind, name)?;
        Ok(read_value(&path, kind, name)?)
    }
    /// Updates a ref with exclusive access to the home lock guard.
    pub fn update(
        &self,
        _lock: &mut HomeLock,
        kind: RefKind,
        name: &RefName,
        new: ObjectId,
        expect: Expect,
    ) -> Result<(), RefError> {
        let path = self.path(kind, name)?;
        expect.check(read_value(&path, kind, name)?)?;
        if kind == RefKind::Pin && name.pin_parts()?.1 != new {
            return Err(RefError::InvalidRef(path));
        }
        let actual = self.objects.kind_of(new).map_err(|error| match error {
            StoreError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
                RefError::MissingTarget(new)
            }
            error => RefError::Store(error),
        })?;
        if actual != ObjectKind::Commit && !(kind == RefKind::Tag && actual == ObjectKind::Tag) {
            return Err(RefError::WrongKind { id: new, actual });
        }
        Ok(atomic_write(&path, format!("{new}\n").as_bytes())?)
    }
    /// Deletes a ref with exclusive access to the home lock guard.
    pub fn delete(
        &self,
        _lock: &mut HomeLock,
        kind: RefKind,
        name: &RefName,
        expect: Expect,
    ) -> Result<(), RefError> {
        let path = self.path(kind, name)?;
        let actual = read_value(&path, kind, name)?;
        expect.check(actual)?;
        if actual.is_none() {
            return Ok(());
        }
        fs::remove_file(&path).map_err(|source| io_error(&path, source))?;
        let base = self.root.join(REFS_DIR).join(kind.directory());
        let mut parent = path.parent();
        while let Some(path) = parent {
            if path == base {
                break;
            }
            match fs::remove_dir(path) {
                Ok(()) => parent = path.parent(),
                Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => break,
                Err(source) => return Err(io_error(path, source).into()),
            }
        }
        Ok(())
    }
    pub fn list(&self, kind: RefKind) -> Result<Vec<(RefName, ObjectId)>, RefError> {
        check_directory(&self.root)?;
        check_directory(&self.root.join(REFS_DIR))?;
        let base = self.root.join(REFS_DIR).join(kind.directory());
        let mut result = Vec::new();
        walk(&base, &mut |path| {
            let relative = path
                .strip_prefix(&base)
                .map_err(|_| RefError::InvalidRef(path.into()))?;
            let value = relative
                .components()
                .map(|part| part.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| RefError::InvalidRef(path.into()))?
                .join("/");
            let name = kind.name(&value)?;
            if let Some(id) = read_value(path, kind, &name)? {
                result.push((name, id));
            }
            Ok::<(), RefError>(())
        })?;
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }
    /// Sync containing directories once after a batch.
    pub fn sync_refs(&self) -> Result<(), RefError> {
        Ok(sync_directories(
            self.fs.as_ref(),
            &self.root.join(REFS_DIR),
        )?)
    }
    /// Removes abandoned ref staging files with exclusive access to the home lock guard.
    pub fn clean_temporary_files(&self, _lock: &mut HomeLock) -> Result<(), RefError> {
        Ok(clean_directory(&self.root.join(REFS_DIR), true)?)
    }
}

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

#[cfg(test)]
mod tests;

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
