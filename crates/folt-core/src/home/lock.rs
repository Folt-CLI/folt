use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use crate::config::names::{LOCK_FILE, TMP_DIR};
use crate::fsx::{TempFile, TempMode};
use crate::fsx::{private_directory_builder, private_file_options};

use super::{Home, HomeError};

impl Home {
    pub(super) fn acquire_lock(&self, nonblocking: bool) -> Result<HomeLock, HomeError> {
        let lock_path = self.path.join(LOCK_FILE);
        let file = open_lock_file(&lock_path)?;
        take_lock(&file, &self.path, lock_path, nonblocking)?;
        let temporary_directory = self.path.join(TMP_DIR);
        ensure_tmp_directory(&temporary_directory)?;
        clear_tmp_directory(&temporary_directory)?;
        Ok(HomeLock {
            file,
            temporary_directory,
        })
    }
}

/// Opens the lock file, creating it when absent, and checks it is a regular file.
fn open_lock_file(lock_path: &Path) -> Result<File, HomeError> {
    match fs::metadata(lock_path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(HomeError::NotRegularFile {
                path: lock_path.to_path_buf(),
            });
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(HomeError::LockIo {
                path: lock_path.to_path_buf(),
                source,
            });
        }
    }
    let file = private_file_options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|source| HomeError::LockIo {
            path: lock_path.to_path_buf(),
            source,
        })?;
    if !file
        .metadata()
        .map_err(|source| HomeError::LockIo {
            path: lock_path.to_path_buf(),
            source,
        })?
        .is_file()
    {
        return Err(HomeError::NotRegularFile {
            path: lock_path.to_path_buf(),
        });
    }
    Ok(file)
}

/// Takes the exclusive lock on the opened lock file, blocking or not.
fn take_lock(
    file: &File,
    home_path: &Path,
    lock_path: PathBuf,
    nonblocking: bool,
) -> Result<(), HomeError> {
    if nonblocking {
        match file.try_lock() {
            Ok(()) => Ok(()),
            Err(TryLockError::WouldBlock) => Err(HomeError::Locked {
                path: home_path.to_path_buf(),
            }),
            Err(TryLockError::Error(source)) => Err(HomeError::LockIo {
                path: lock_path,
                source,
            }),
        }
    } else {
        file.lock().map_err(|source| HomeError::LockIo {
            path: lock_path,
            source,
        })
    }
}

/// Makes sure the tmp directory exists as a real directory.
fn ensure_tmp_directory(temporary_directory: &Path) -> Result<(), HomeError> {
    match fs::symlink_metadata(temporary_directory) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(HomeError::CannotCreate {
            path: temporary_directory.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "temporary path must be a real directory",
            ),
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => private_directory_builder()
            .create(temporary_directory)
            .map_err(|source| HomeError::CannotCreate {
                path: temporary_directory.to_path_buf(),
                source,
            }),
        Err(source) => Err(HomeError::CannotCreate {
            path: temporary_directory.to_path_buf(),
            source,
        }),
    }
}

/// Removes every entry left in the tmp directory by an earlier process.
fn clear_tmp_directory(temporary_directory: &Path) -> Result<(), HomeError> {
    for entry in fs::read_dir(temporary_directory).map_err(|source| HomeError::CannotCreate {
        path: temporary_directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| HomeError::CannotCreate {
            path: temporary_directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let result = if entry
            .file_type()
            .map_err(|source| HomeError::CannotCreate {
                path: path.clone(),
                source,
            })?
            .is_dir()
        {
            crate::fsx::remove_dir_all(&path)
        } else {
            crate::fsx::remove_file(&path)
        };
        result.map_err(|source| HomeError::CannotCreate {
            path: path.clone(),
            source,
        })?;
    }
    Ok(())
}

/// An exclusive lock held for the lifetime of the guard.
///
/// Keep one guard per process. Share `&HomeLock` for operations that are safe
/// side by side: object puts, `create_folio` and registry writes. Exclusive
/// operations take `&mut HomeLock`. If threads need both, wrap the guard in a
/// [`std::sync::RwLock`] (read for puts, write for exclusive operations) or a
/// [`std::sync::Mutex`].
pub struct HomeLock {
    file: File,
    pub(super) temporary_directory: PathBuf,
}

impl Drop for HomeLock {
    fn drop(&mut self) {
        drop(self.file.unlock());
    }
}

impl HomeLock {
    /// Writes a unique temporary file and renames it to the destination.
    /// The destination must be on the same filesystem as the home; an existing
    /// destination is replaced where the platform allows it.
    pub fn write_temp_file<F>(&self, destination: &Path, write: F) -> Result<(), HomeError>
    where
        F: FnOnce(&mut File) -> io::Result<()>,
    {
        let write_failed = |source| HomeError::WriteFailed {
            path: destination.to_path_buf(),
            source,
        };
        let mut temporary = TempFile::create(&self.temporary_directory, "", '.', TempMode::Private)
            .map_err(|(_, source)| write_failed(source))?;
        write(temporary.file_mut()).map_err(write_failed)?;
        temporary.sync().map_err(write_failed)?;
        temporary.publish(destination).map_err(write_failed)?;
        Ok(())
    }
}
