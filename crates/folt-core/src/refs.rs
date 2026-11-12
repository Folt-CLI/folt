use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::limits::REF_FILE_MAX_BYTES;
use crate::config::names::{HEADS_DIR, PINS_DIR, REFS_DIR, TAGS_DIR, TEMP_PREFIX_REF};
use crate::folio_dir::{self, FolioDir, FormatError};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ref_name::{LowerRefError, RefKind, RefName};
use crate::registry::FolioFormat;
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
    root: PathBuf,
    objects: ObjectStore,
}
impl RefStore {
    pub fn open(folio: &FolioDir, format: &FolioFormat) -> Result<Self, RefError> {
        folio_dir::check_format(format).map_err(RefError::Format)?;
        Ok(Self {
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
        Ok(sync_directories(&self.root.join(REFS_DIR))?)
    }
    /// Removes abandoned ref staging files with exclusive access to the home lock guard.
    pub fn clean_temporary_files(&self, _lock: &mut HomeLock) -> Result<(), RefError> {
        Ok(clean_directory(&self.root.join(REFS_DIR), true)?)
    }
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>, LowerRefError> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(path, source)),
    }
}
fn check_directory(path: &Path) -> Result<(), LowerRefError> {
    if let Some(meta) = metadata(path)?
        && !meta.file_type().is_dir()
    {
        return Err(LowerRefError::InvalidRef(path.into()));
    }
    Ok(())
}
fn read_bytes(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, LowerRefError> {
    let Some(meta) = metadata(path)? else {
        return Ok(None);
    };
    if !meta.file_type().is_file() {
        return Err(LowerRefError::InvalidRef(path.into()));
    }
    let file = File::open(path).map_err(|source| io_error(path, source))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| io_error(path, source))?;
    if bytes.len() as u64 > limit {
        return Err(LowerRefError::InvalidRef(path.into()));
    }
    Ok(Some(bytes))
}
fn read_value(
    path: &Path,
    kind: RefKind,
    name: &RefName,
) -> Result<Option<ObjectId>, LowerRefError> {
    let Some(bytes) = read_bytes(path, REF_FILE_MAX_BYTES as u64)? else {
        return Ok(None);
    };
    let id = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.strip_suffix('\n'))
        .and_then(|text| text.parse::<ObjectId>().ok())
        .ok_or_else(|| LowerRefError::InvalidRef(path.into()))?;
    if kind == RefKind::Pin && name.pin_parts()?.1 != id {
        return Err(LowerRefError::InvalidRef(path.into()));
    }
    Ok(Some(id))
}
fn temporary_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(TEMP_PREFIX_REF) else {
        return false;
    };
    let Some((pid, counter)) = rest.split_once('.') else {
        return false;
    };
    !pid.is_empty()
        && !counter.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && counter.bytes().all(|b| b.is_ascii_digit())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), LowerRefError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| LowerRefError::InvalidRef(path.into()))?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let (temporary, mut file) = loop {
        let temporary = parent.join(format!(
            "{TEMP_PREFIX_REF}{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error(&temporary, source)),
        }
    };
    let result = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| io_error(&temporary, source))?;
        drop(file);
        fs::rename(&temporary, path).map_err(|source| io_error(path, source))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn entries(path: &Path) -> Result<Vec<fs::DirEntry>, LowerRefError> {
    check_directory(path)?;
    match fs::read_dir(path) {
        Ok(entries) => entries
            .map(|entry| entry.map_err(|source| io_error(path, source)))
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(io_error(path, source)),
    }
}
fn walk<E: From<LowerRefError>>(
    path: &Path,
    visit: &mut impl FnMut(&Path) -> Result<(), E>,
) -> Result<(), E> {
    for entry in entries(path)? {
        let path = entry.path();
        let ty = entry
            .file_type()
            .map_err(|source| io_error(&path, source))?;
        if ty.is_dir() {
            walk(&path, visit)?;
        } else if ty.is_file() && entry.file_name().to_str().is_some_and(temporary_name) {
            continue;
        } else if ty.is_file() {
            visit(&path)?;
        } else {
            return Err(LowerRefError::InvalidRef(path).into());
        }
    }
    Ok(())
}
fn clean_directory(path: &Path, recursive: bool) -> Result<(), LowerRefError> {
    for entry in entries(path)? {
        let child = entry.path();
        let ty = entry
            .file_type()
            .map_err(|source| io_error(&child, source))?;
        if ty.is_file() && entry.file_name().to_str().is_some_and(temporary_name) {
            fs::remove_file(&child).map_err(|source| io_error(&child, source))?;
        } else if recursive && ty.is_dir() {
            clean_directory(&child, true)?;
        } else if recursive && !ty.is_file() {
            return Err(LowerRefError::InvalidRef(child));
        }
    }
    Ok(())
}
fn sync_directory(path: &Path) -> Result<(), LowerRefError> {
    crate::fs::sync_directory(path).map_err(|source| io_error(path, source))
}
fn sync_directories(path: &Path) -> Result<(), LowerRefError> {
    for entry in entries(path)? {
        let child = entry.path();
        if entry
            .file_type()
            .map_err(|source| io_error(&child, source))?
            .is_dir()
        {
            sync_directories(&child)?;
        }
    }
    if metadata(path)?.is_some() {
        sync_directory(path)?;
    }
    Ok(())
}

pub(crate) fn inspect_refs(root: &Path) -> Result<(), LowerRefError> {
    let refs = root.join(REFS_DIR);
    for entry in entries(&refs)? {
        let path = entry.path();
        let kind = match entry.file_name().to_str() {
            Some(HEADS_DIR) => RefKind::Branch,
            Some(TAGS_DIR) => RefKind::Tag,
            Some(PINS_DIR) => RefKind::Pin,
            _ => return Err(LowerRefError::InvalidRef(path)),
        };
        check_directory(&path)?;
        walk(&path, &mut |file| {
            let relative = file
                .strip_prefix(&path)
                .map_err(|_| LowerRefError::InvalidRef(file.into()))?;
            let value = relative
                .components()
                .map(|part| part.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| LowerRefError::InvalidRef(file.into()))?
                .join("/");
            let name = kind.name(&value)?;
            read_value(file, kind, &name)?;
            Ok::<(), LowerRefError>(())
        })?;
    }
    Ok(())
}
fn io_error(path: &Path, source: io::Error) -> LowerRefError {
    LowerRefError::Io {
        path: path.into(),
        source,
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
