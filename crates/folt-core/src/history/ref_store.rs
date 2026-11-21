use std::io;
use std::path::PathBuf;

use crate::config::names::REFS_DIR;
use crate::folio::dir::FolioDir;
use crate::folio::format::{FolioFormat, check_format};
use crate::folio::refs_dir::{
    atomic_write, check_directory, clean_directory, io_error, metadata, read_value,
    sync_directories, walk,
};
use crate::fsx::{FileSystem, RealFileSystem};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ref_name::{RefKind, RefName};
use crate::store::{ObjectStore, StoreError};

use super::{Expect, RefError};

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
        crate::fsx::remove_file(&path).map_err(|source| io_error(&path, source))?;
        let base = self.root.join(REFS_DIR).join(kind.directory());
        let mut parent = path.parent();
        while let Some(path) = parent {
            if path == base {
                break;
            }
            match crate::fsx::remove_dir(path) {
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

#[cfg(test)]
mod tests;
