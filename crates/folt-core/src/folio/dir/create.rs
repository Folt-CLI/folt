use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::config::names::{BLOBS_DIR, CONTENT_FILE, FOLIOS_DIR, NEW_DIR, REFS_DIR};
use crate::fsx::{FileSystem, RealFileSystem};
use crate::fsx::{TempFile, TempMode, private_directory_builder};
use crate::home::HomeLock;
use crate::id::FolioId;

use super::error::io_error;
use super::{Durability, FolioDir, FolioDirError, FolioSpec};

/// Creates a Folio by staging it under `folios/.new/` and renaming it into place.
pub fn create_folio(
    home: &Path,
    lock: &HomeLock,
    spec: FolioSpec,
    durability: Durability,
) -> Result<FolioDir, FolioDirError> {
    create_folio_with(&RealFileSystem, home, lock, spec, durability)
}

pub(crate) fn create_folio_with(
    fs: &dyn FileSystem,
    home: &Path,
    _lock: &HomeLock,
    spec: FolioSpec,
    durability: Durability,
) -> Result<FolioDir, FolioDirError> {
    let folios = home.join(FOLIOS_DIR);
    ensure_directory(&folios)?;
    let staging = folios.join(NEW_DIR);
    ensure_directory(&staging)?;
    let id = FolioId::generate();
    let temporary = staging.join(id.to_string());
    private_directory_builder()
        .create(&temporary)
        .map_err(|source| io_error(&temporary, source))?;
    let result = build_and_publish(fs, &folios, &temporary, id, spec, durability);
    if result.is_err() {
        let _ = crate::fsx::remove_dir_all(&temporary);
    }
    result
}

fn build_and_publish(
    fs: &dyn FileSystem,
    folios: &Path,
    temporary: &Path,
    id: FolioId,
    spec: FolioSpec,
    durability: Durability,
) -> Result<FolioDir, FolioDirError> {
    let content_path = temporary.join(CONTENT_FILE);
    let mut file = TempFile::create_at(&content_path, TempMode::Private)
        .map_err(|source| io_error(&content_path, source))?;
    file.file_mut()
        .write_all(&spec.content)
        .map_err(|source| io_error(&content_path, source))?;
    if durability == Durability::DataAndDirectories {
        file.sync()
            .map_err(|source| io_error(&content_path, source))?;
    }
    ensure_directory(&temporary.join(BLOBS_DIR))?;
    ensure_directory(&temporary.join(REFS_DIR))?;
    if durability == Durability::DataAndDirectories {
        fs.sync_directory(temporary)
            .map_err(|source| io_error(temporary, source))?;
    }
    let destination = folios.join(id.to_string());
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(FolioDirError::AlreadyExists(destination));
    }
    // Windows cannot rename the staging directory while its content file is open.
    file.keep();
    crate::fsx::rename(temporary, &destination).map_err(|source| io_error(&destination, source))?;
    if durability == Durability::DataAndDirectories
        && let Err(source) = fs.sync_directory(folios)
    {
        // The rename already published this call's directory. The caller has
        // no FolioDir to roll back when creation returns an error.
        crate::fsx::remove_dir_all(&destination)
            .map_err(|source| io_error(&destination, source))?;
        return Err(io_error(folios, source));
    }
    Ok(FolioDir {
        id,
        path: destination,
    })
}

/// Removes all crash leftovers beneath the staging directory with exclusive
/// access to the home lock guard.
pub fn sweep_new(home: &Path, _lock: &mut HomeLock) -> Result<(), FolioDirError> {
    let staging = home.join(FOLIOS_DIR).join(NEW_DIR);
    match fs::read_dir(&staging) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|source| io_error(&staging, source))?;
                let path = entry.path();
                let ty = entry
                    .file_type()
                    .map_err(|source| io_error(&path, source))?;
                if ty.is_dir() {
                    crate::fsx::remove_dir_all(&path)
                } else {
                    crate::fsx::remove_file(&path)
                }
                .map_err(|source| io_error(&path, source))?;
            }
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&staging, source)),
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), FolioDirError> {
    private_directory_builder()
        .recursive(true)
        .create(path)
        .map_err(|source| io_error(path, source))
}
