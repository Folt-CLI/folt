use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::config::limits::STREAM_BUFFER_SIZE;
use crate::config::names::{FOLIOS_DIR, TEMP_TAG_CONVERT, TEMP_TAG_COPY, TEMP_TAG_LINK_PROBE};
use crate::folio::dir::path::content_path;
use crate::fsx::{
    FileSystem, RealFileSystem, TempFile, TempMode, copy_new, hard_links_unsupported, signature,
    temp_name,
};
use crate::home::Home;
use crate::id::FolioId;
use crate::id::ObjectId;
use crate::object::{ObjectHasher, ObjectKind};
use crate::placement::{
    Observation, PlacementBase, PlacementError, PlacementKind, PlacementOps, PlacementSpec, Written,
};
use crate::store::ObjectStore;

/// Filesystem implementation of link and copy placement operations.
pub struct PlacementFs<'a> {
    home: &'a Home,
    fs: &'a dyn FileSystem,
}

impl<'a> PlacementFs<'a> {
    pub fn new(home: &'a Home) -> Self {
        Self::with_fs(home, &RealFileSystem)
    }

    pub(crate) fn with_fs(home: &'a Home, fs: &'a dyn FileSystem) -> Self {
        Self { home, fs }
    }

    /// Checks whether the path's filesystem permits creating a symbolic link.
    /// Runs only when a link is explicitly requested; a copy never calls it.
    pub fn probe_link(&self, path: &Path) -> Result<(), PlacementError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let probe_path = temporary_path(parent, TEMP_TAG_LINK_PROBE);
        let probe =
            TempFile::sibling_at(&probe_path).map_err(|source| symlink_failure(path, source))?;
        self.fs
            .create_symlink(&self.home.path().join(FOLIOS_DIR), probe.path())
            .map_err(|source| symlink_failure(path, source))?;
        probe
            .remove()
            .map_err(|source| failure(&probe_path, source))
    }

    fn refresh_link(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        let target = self.link_target(id);
        fs::metadata(&target).map_err(|e| failure(&target, e))?;
        self.rewrite_link(id, placement, expected)
    }

    fn rewrite_link(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        let target = self.link_target(id);
        match fs::symlink_metadata(&placement.path) {
            Ok(existing) if existing.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(failure(
                    &placement.path,
                    io::Error::other("cannot refresh a link placement over a non-symlink"),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure(&placement.path, error)),
        }
        let link_path = temporary_path(
            placement.path.parent().unwrap_or_else(|| Path::new(".")),
            "folt-link",
        );
        let temporary = TempFile::sibling_at(&link_path).map_err(|e| failure(&link_path, e))?;
        self.fs
            .create_symlink(&target, temporary.path())
            .map_err(|e| failure(&link_path, e))?;
        if let Some(expected) = expected
            && self.read(id, placement)? != *expected
        {
            return Err(PlacementError::PlacementChanged);
        }
        temporary
            .publish(&placement.path)
            .map_err(|e| failure(&placement.path, e))?;
        Ok(Written { base: None })
    }

    fn relocate_with_rename(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to: &Path,
        expected: &Observation,
        rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<Written, PlacementError> {
        if self.read(id, placement)? != *expected {
            return Err(PlacementError::PlacementChanged);
        }
        require_absent(to)?;
        rename(&placement.path, to).map_err(|error| move_failure(to, error))?;
        Ok(Written {
            base: placement.base.clone(),
        })
    }

    fn copy_content(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
        replace: bool,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary = create_temporary_file(parent, TEMP_TAG_COPY)?;
        write_copy(&mut temporary, objects, content)?;
        // Defer destination errors so a changed observation is reported as such.
        let prepare = if replace {
            match fs::symlink_metadata(&placement.path) {
                Ok(existing) if existing.is_file() => {
                    crate::fsx::set_permissions(temporary.path(), existing.permissions())
                        .map_err(|error| failure(temporary.path(), error))
                }
                Ok(_) => Err(failure(
                    &placement.path,
                    io::Error::other("cannot refresh a copy placement over a non-regular file"),
                )),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(failure(&placement.path, error)),
            }
        } else {
            Ok(())
        };
        if !replace {
            if let Err(error) = fs::symlink_metadata(&placement.path) {
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(failure(&placement.path, error));
                }
            } else {
                return Err(failure(
                    &placement.path,
                    io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "placement path already exists",
                    ),
                ));
            }
        }
        let publish = if replace {
            if let Some(expected) = expected {
                match self.read(id, placement) {
                    Ok(observation) if &observation == expected => {}
                    Ok(_) => return Err(PlacementError::PlacementChanged),
                    Err(error) => return Err(error),
                }
            }
            prepare?;
            temporary.publish(&placement.path)
        } else {
            let linked = match crate::fsx::hard_link(temporary.path(), &placement.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
                Err(error) if hard_links_unsupported(&error) => {
                    copy_new(temporary.path(), &placement.path)
                }
                Err(error) => Err(error),
            };
            // The temporary name is gone before the placement is read, so its link count is one.
            drop(temporary);
            linked
        };
        if let Err(error) = publish {
            return Err(failure(&placement.path, error));
        }
        let metadata = fs::symlink_metadata(&placement.path)
            .map_err(|error| failure(&placement.path, error))?;
        Ok(Written {
            base: Some(PlacementBase {
                hash: content,
                signature: signature(&metadata),
            }),
        })
    }
}

impl PlacementOps for PlacementFs<'_> {
    fn link_target(&self, id: FolioId) -> PathBuf {
        content_path(self.home, id)
    }

    fn create(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
    ) -> Result<Written, PlacementError> {
        let target = self.link_target(id);
        match placement.kind {
            PlacementKind::Link => {
                fs::metadata(&target).map_err(|e| failure(&target, e))?;
                self.fs
                    .create_symlink(&target, &placement.path)
                    .map_err(|e| failure(&placement.path, e))?;
                Ok(Written { base: None })
            }
            PlacementKind::Copy | PlacementKind::Internal => {
                self.copy_content(id, placement, objects, content, false, None)
            }
        }
    }

    fn read(&self, id: FolioId, placement: &PlacementSpec) -> Result<Observation, PlacementError> {
        let target = self.link_target(id);
        let metadata = match fs::symlink_metadata(&placement.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Observation::Missing);
            }
            Err(error) => return Err(failure(&placement.path, error)),
        };
        if metadata.file_type().is_symlink() {
            return match fs::read_link(&placement.path) {
                Ok(found) => {
                    let found_path = placement
                        .path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(&found);
                    // Resolving a detached target can fail (for example, a file
                    // used as a directory). The link itself is still observable.
                    let intact = match (resolved_path(&found_path), resolved_path(&target)) {
                        (Ok(found), Ok(expected)) => found == expected,
                        _ => false,
                    };
                    Ok(if intact {
                        Observation::IntactLink
                    } else {
                        Observation::OtherLink { target: found }
                    })
                }
                Err(error) => Err(failure(&placement.path, error)),
            };
        }
        if metadata.is_file() {
            return Ok(Observation::File {
                signature: signature(&metadata),
            });
        }
        Ok(if metadata.is_dir() {
            Observation::Directory
        } else {
            Observation::NotAFile
        })
    }

    fn refresh(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError> {
        match placement.kind {
            PlacementKind::Copy | PlacementKind::Internal => {
                self.copy_content(id, placement, objects, content, true, expected)
            }
            PlacementKind::Link => self.refresh_link(id, placement, expected),
        }
    }

    fn normalize_link(&self, id: FolioId, placement: &PlacementSpec) -> Result<(), PlacementError> {
        if self.read(id, placement)? != Observation::IntactLink {
            return Ok(());
        }
        let target =
            fs::read_link(&placement.path).map_err(|error| failure(&placement.path, error))?;
        if !target.is_absolute() {
            self.rewrite_link(id, placement, Some(&Observation::IntactLink))?;
        }
        Ok(())
    }

    fn sync_parent(&self, placement: &PlacementSpec) -> Result<(), PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        self.fs
            .sync_directory(parent)
            .map_err(|error| failure(parent, error))
    }

    fn convert(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to_kind: PlacementKind,
        objects: &ObjectStore,
        content: ObjectId,
        expected: &Observation,
    ) -> Result<Written, PlacementError> {
        let parent = placement.path.parent().unwrap_or_else(|| Path::new("."));
        let temporary_path = temporary_path(parent, TEMP_TAG_CONVERT);
        (|| {
            let temporary = match (placement.kind, to_kind, expected) {
                (PlacementKind::Link, PlacementKind::Copy, Observation::IntactLink) => {
                    let mut temporary = create_temporary_file(parent, TEMP_TAG_CONVERT)?;
                    write_copy(&mut temporary, objects, content)?;
                    let output = OpenOptions::new()
                        .write(true)
                        .open(temporary.path())
                        .map_err(|error| failure(temporary.path(), error))?;
                    let metadata = fs::metadata(&placement.path)
                        .map_err(|error| failure(&placement.path, error))?;
                    crate::fsx::set_file_permissions(&output, metadata.permissions())
                        .map_err(|error| failure(temporary.path(), error))?;
                    crate::fsx::sync_file(&output)
                        .map_err(|error| failure(temporary.path(), error))?;
                    temporary
                }
                (PlacementKind::Copy, PlacementKind::Link, Observation::File { .. }) => {
                    self.fs
                        .link_count(&placement.path)
                        .and_then(crate::fsx::ensure_single)
                        .map_err(|error| failure(&placement.path, error))?;
                    let target = self.link_target(id);
                    fs::metadata(&target).map_err(|error| failure(&target, error))?;
                    let temporary = TempFile::sibling_at(&temporary_path)
                        .map_err(|error| symlink_failure(&placement.path, error))?;
                    self.fs
                        .create_symlink(&target, temporary.path())
                        .map_err(|error| symlink_failure(&placement.path, error))?;
                    self.fs
                        .sync_directory(parent)
                        .map_err(|error| failure(parent, error))?;
                    temporary
                }
                _ => {
                    return Err(failure(
                        &placement.path,
                        io::Error::other("invalid conversion"),
                    ));
                }
            };
            if self.read(id, placement)? != *expected {
                return Err(PlacementError::PlacementChanged);
            }
            if to_kind == PlacementKind::Link {
                self.fs
                    .link_count(&placement.path)
                    .and_then(crate::fsx::ensure_single)
                    .map_err(|error| failure(&placement.path, error))?;
            }
            // Rename replaces the entry atomically (MoveFileExW with
            // MOVEFILE_REPLACE_EXISTING on Windows). Never unlink first: a
            // failed replacement must leave the original entry available for retry.
            temporary
                .publish(&placement.path)
                .map_err(|error| failure(&placement.path, error))?;
            self.fs
                .sync_directory(parent)
                .map_err(|error| failure(parent, error))?;
            let base = if to_kind == PlacementKind::Copy {
                let metadata = fs::symlink_metadata(&placement.path)
                    .map_err(|error| failure(&placement.path, error))?;
                Some(PlacementBase {
                    hash: content,
                    signature: signature(&metadata),
                })
            } else {
                None
            };
            Ok(Written { base })
        })()
    }

    fn relocate(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to: &Path,
        expected: &Observation,
    ) -> Result<Written, PlacementError> {
        self.relocate_with_rename(id, placement, to, expected, |from, to| {
            self.fs.rename_no_replace(from, to)
        })
    }

    fn unlink(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        expected: &Observation,
    ) -> Result<(), PlacementError> {
        if self.read(id, placement)? != *expected {
            return Err(PlacementError::PlacementChanged);
        }
        match expected {
            Observation::IntactLink | Observation::OtherLink { .. } | Observation::File { .. } => {
                crate::fsx::remove_file(&placement.path)
                    .map_err(|error| failure(&placement.path, error))
            }
            _ => Err(PlacementError::PlacementChanged),
        }
    }

    fn hash_placement(&self, placement: &PlacementSpec) -> Result<ObjectId, PlacementError> {
        hash_file(&placement.path)
    }
}

/// Resolve existing ancestors even when a former home has been moved or removed.
pub(crate) fn resolved_path(path: &Path) -> Result<PathBuf, PlacementError> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut resolved) => {
                for name in missing.iter().rev() {
                    resolved.push(name);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = ancestor.file_name() else {
                    return Err(failure(path, error));
                };
                missing.push(name);
                ancestor = ancestor.parent().ok_or_else(|| failure(path, error))?;
            }
            Err(error) => return Err(failure(path, error)),
        }
    }
}

fn move_failure(path: &Path, source: io::Error) -> PlacementError {
    if source.kind() == io::ErrorKind::CrossesDevices {
        failure(
            path,
            io::Error::new(
                io::ErrorKind::CrossesDevices,
                "moves across filesystems are not supported",
            ),
        )
    } else {
        failure(path, source)
    }
}

fn require_absent(path: &Path) -> Result<(), PlacementError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(failure(
            path,
            io::Error::new(io::ErrorKind::AlreadyExists, "destination exists"),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failure(path, error)),
    }
}

fn create_temporary_file(parent: &Path, tag: &str) -> Result<TempFile, PlacementError> {
    let path = temporary_path(parent, tag);
    TempFile::create_at(&path, TempMode::Default).map_err(|error| failure(&path, error))
}

/// Writes the object into the temporary file and closes it; the guard removes it on failure.
fn write_copy(
    temporary: &mut TempFile,
    objects: &ObjectStore,
    content: ObjectId,
) -> Result<(), PlacementError> {
    let path = temporary.path().to_path_buf();
    let output = temporary.file_mut();
    objects
        .get_to(content, ObjectKind::Blob, &mut *output)
        .map_err(PlacementError::Store)?;
    output.flush().map_err(|error| failure(&path, error))?;
    crate::fsx::sync_file(output).map_err(|error| failure(&path, error))?;
    temporary.close();
    Ok(())
}

fn hash_file(path: &Path) -> Result<ObjectId, PlacementError> {
    let metadata = fs::metadata(path).map_err(|e| failure(path, e))?;
    let expected = metadata.len();
    let mut input = File::open(path).map_err(|e| failure(path, e))?;
    let mut hasher = ObjectHasher::new(ObjectKind::Blob, expected);
    let mut buffer = vec![0; STREAM_BUFFER_SIZE];
    let mut total = 0u64;
    loop {
        let count = input.read(&mut buffer).map_err(|e| failure(path, e))?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| failure(path, io::Error::other("content length overflow")))?;
        hasher.update(&buffer[..count]);
    }
    if total != expected {
        return Err(failure(
            path,
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "placement content changed while hashing",
            ),
        ));
    }
    Ok(hasher.finish())
}

fn temporary_path(parent: &Path, tag: &str) -> PathBuf {
    parent.join(temp_name(&format!(".{tag}-"), '-'))
}

pub(crate) fn require_single_link(path: &Path) -> Result<(), PlacementError> {
    crate::fsx::require_single_link(path).map_err(|error| failure(path, error))
}

/// Windows privilege-not-held error code (dimensionless).
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

fn symlink_failure(path: &Path, source: io::Error) -> PlacementError {
    let unavailable = source.kind() == io::ErrorKind::Unsupported
        || cfg!(windows)
            && (source.kind() == io::ErrorKind::PermissionDenied
                || source.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD));
    if unavailable {
        PlacementError::LinkUnavailable {
            path: path.to_path_buf(),
            source,
        }
    } else {
        failure(path, source)
    }
}

fn failure(path: &Path, source: io::Error) -> PlacementError {
    PlacementError::OperationFailed {
        path: path.to_path_buf(),
        source,
    }
}
#[cfg(test)]
mod tests;
