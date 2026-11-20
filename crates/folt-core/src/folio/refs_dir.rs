use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

use crate::config::limits::REF_FILE_MAX_BYTES;
use crate::config::names::{HEADS_DIR, PINS_DIR, REFS_DIR, TAGS_DIR, TEMP_PREFIX_REF};
use crate::fsx::{FileSystem, RealFileSystem};
use crate::fsx::{TempFile, TempMode};
use crate::id::ObjectId;
use crate::ref_name::{LowerRefError, RefKind, RefName};

pub(crate) fn metadata(path: &Path) -> Result<Option<fs::Metadata>, LowerRefError> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(path, source)),
    }
}
pub(crate) fn check_directory(path: &Path) -> Result<(), LowerRefError> {
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
pub(crate) fn read_value(
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
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), LowerRefError> {
    let parent = path
        .parent()
        .ok_or_else(|| LowerRefError::InvalidRef(path.into()))?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let mut temporary = TempFile::create(parent, TEMP_PREFIX_REF, '.', TempMode::Default)
        .map_err(|(path, source)| io_error(&path, source))?;
    temporary
        .file_mut()
        .write_all(bytes)
        .map_err(|source| io_error(temporary.path(), source))?;
    temporary
        .sync()
        .map_err(|source| io_error(temporary.path(), source))?;
    temporary
        .publish(path)
        .map_err(|source| io_error(path, source))
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
pub(crate) fn walk<E: From<LowerRefError>>(
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
pub(crate) fn clean_directory(path: &Path, recursive: bool) -> Result<(), LowerRefError> {
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
    RealFileSystem
        .sync_directory(path)
        .map_err(|source| io_error(path, source))
}
pub(crate) fn sync_directories(path: &Path) -> Result<(), LowerRefError> {
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
pub(crate) fn io_error(path: &Path, source: io::Error) -> LowerRefError {
    LowerRefError::Io {
        path: path.into(),
        source,
    }
}
