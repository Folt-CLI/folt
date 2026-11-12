use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::limits::STREAM_BUFFER_SIZE;
use crate::config::names::{
    CONTENT_FILE, FOLIOS_DIR, TEMP_TAG_CONVERT, TEMP_TAG_COPY, TEMP_TAG_LINK_PROBE, TEMP_TAG_NAME,
};
use crate::home::Home;
use crate::id::FolioId;
use crate::id::ObjectId;
use crate::object::{ObjectHasher, ObjectKind};
use crate::placement::{
    Observation, PlacementBase, PlacementError, PlacementKind, PlacementOps, PlacementSpec,
    Signature, Written,
};
use crate::store::ObjectStore;

/// Returns the working file path for a Folio's internal placement.
pub fn internal_path(home: &Home, id: FolioId) -> PathBuf {
    home.path()
        .join(FOLIOS_DIR)
        .join(id.to_string())
        .join(CONTENT_FILE)
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Filesystem implementation of link and copy placement operations.
pub struct PlacementFs<'a> {
    home: &'a Home,
}

impl<'a> PlacementFs<'a> {
    pub fn new(home: &'a Home) -> Self {
        Self { home }
    }

    /// Returns the canonical derived link target spelling used by create and read.
    pub fn link_target(&self, id: FolioId) -> PathBuf {
        internal_path(self.home, id)
    }

    /// Checks whether the path's filesystem permits creating a symbolic link.
    /// Runs only when a link is explicitly requested; a copy never calls it.
    pub fn probe_link(&self, path: &Path) -> Result<(), PlacementError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let probe = temporary_path(parent, TEMP_TAG_LINK_PROBE);
        create_symlink(&self.home.path().join(FOLIOS_DIR), &probe)
            .map_err(|source| symlink_failure(path, source))?;
        fs::remove_file(&probe).map_err(|source| failure(&probe, source))
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
        let temporary = temporary_path(
            placement.path.parent().unwrap_or_else(|| Path::new(".")),
            "folt-link",
        );
        create_symlink(&target, &temporary).map_err(|e| failure(&temporary, e))?;
        if let Some(expected) = expected
            && self.read(id, placement)? != *expected
        {
            let _ = fs::remove_file(&temporary);
            return Err(PlacementError::PlacementChanged);
        }
        fs::rename(&temporary, &placement.path).map_err(|e| failure(&placement.path, e))?;
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
        let temporary = temporary_path(parent, TEMP_TAG_COPY);
        write_copy(&temporary, objects, content)?;
        // Defer destination errors so a changed observation is reported as such.
        let prepare = if replace {
            match fs::symlink_metadata(&placement.path) {
                Ok(existing) if existing.is_file() => {
                    fs::set_permissions(&temporary, existing.permissions())
                        .map_err(|error| failure(&temporary, error))
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
                    let _ = fs::remove_file(&temporary);
                    return Err(failure(&placement.path, error));
                }
            } else {
                let _ = fs::remove_file(&temporary);
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
                    Ok(_) => {
                        let _ = fs::remove_file(&temporary);
                        return Err(PlacementError::PlacementChanged);
                    }
                    Err(error) => {
                        let _ = fs::remove_file(&temporary);
                        return Err(error);
                    }
                }
            }
            if let Err(error) = prepare {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
            fs::rename(&temporary, &placement.path)
        } else {
            match fs::hard_link(&temporary, &placement.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
                Err(error) if hard_links_unsupported(&error) => {
                    copy_new(&temporary, &placement.path)
                }
                Err(error) => Err(error),
            }
        };
        if let Err(error) = publish {
            let _ = fs::remove_file(&temporary);
            return Err(failure(&placement.path, error));
        }
        let _ = fs::remove_file(&temporary);
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
                create_symlink(&target, &placement.path)
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
        crate::fs::sync_directory(parent).map_err(|error| failure(parent, error))
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
        let temporary = temporary_path(parent, TEMP_TAG_CONVERT);
        let mut prepared = false;
        let result = (|| {
            match (placement.kind, to_kind, expected) {
                (PlacementKind::Link, PlacementKind::Copy, Observation::IntactLink) => {
                    write_copy(&temporary, objects, content)?;
                    prepared = true;
                    let output = OpenOptions::new()
                        .write(true)
                        .open(&temporary)
                        .map_err(|error| failure(&temporary, error))?;
                    let metadata = fs::metadata(&placement.path)
                        .map_err(|error| failure(&placement.path, error))?;
                    output
                        .set_permissions(metadata.permissions())
                        .map_err(|error| failure(&temporary, error))?;
                    output
                        .sync_all()
                        .map_err(|error| failure(&temporary, error))?;
                }
                (PlacementKind::Copy, PlacementKind::Link, Observation::File { .. }) => {
                    require_single_link(&placement.path)?;
                    let target = self.link_target(id);
                    fs::metadata(&target).map_err(|error| failure(&target, error))?;
                    create_symlink(&target, &temporary)
                        .map_err(|error| symlink_failure(&placement.path, error))?;
                    prepared = true;
                    crate::fs::sync_directory(parent).map_err(|error| failure(parent, error))?;
                }
                _ => {
                    return Err(failure(
                        &placement.path,
                        io::Error::other("invalid conversion"),
                    ));
                }
            }
            if self.read(id, placement)? != *expected {
                return Err(PlacementError::PlacementChanged);
            }
            if to_kind == PlacementKind::Link {
                require_single_link(&placement.path)?;
            }
            // Rename replaces the entry atomically (MoveFileExW with
            // MOVEFILE_REPLACE_EXISTING on Windows). Never unlink first: a
            // failed replacement must leave the original entry available for retry.
            fs::rename(&temporary, &placement.path)
                .map_err(|error| failure(&placement.path, error))?;
            crate::fs::sync_directory(parent).map_err(|error| failure(parent, error))?;
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
        })();
        if prepared {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn relocate(
        &self,
        id: FolioId,
        placement: &PlacementSpec,
        to: &Path,
        expected: &Observation,
    ) -> Result<Written, PlacementError> {
        self.relocate_with_rename(id, placement, to, expected, rename_no_replace)
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
                fs::remove_file(&placement.path).map_err(|error| failure(&placement.path, error))
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(io::Error::from)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(source, destination)
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

fn write_copy(path: &Path, objects: &ObjectStore, content: ObjectId) -> Result<(), PlacementError> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| failure(path, error))?;
    let result = (|| {
        objects
            .get_to(content, ObjectKind::Blob, &mut output)
            .map_err(PlacementError::Store)?;
        output.flush().map_err(|error| failure(path, error))?;
        output.sync_all().map_err(|error| failure(path, error))
    })();
    drop(output);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
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

/// Nanoseconds per second (nanoseconds).
#[cfg(unix)]
const NANOS_PER_SECOND: i128 = 1_000_000_000;
/// Windows-to-Unix epoch offset in FILETIME ticks.
#[cfg(windows)]
const WINDOWS_TO_UNIX_EPOCH_TICKS: i128 = 116_444_736_000_000_000;
/// Nanoseconds per FILETIME tick (nanoseconds).
#[cfg(windows)]
const NANOS_PER_TICK: i128 = 100;
/// Windows privilege-not-held error code (dimensionless).
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

pub(crate) fn signature(metadata: &fs::Metadata) -> Signature {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    #[cfg(unix)]
    let (mtime, ctime, inode) = (
        Some(metadata.mtime() as i128 * NANOS_PER_SECOND + metadata.mtime_nsec() as i128),
        Some(metadata.ctime() as i128 * NANOS_PER_SECOND + metadata.ctime_nsec() as i128),
        Some(metadata.ino()),
    );
    #[cfg(windows)]
    let (mtime, ctime, inode) = {
        use std::os::windows::fs::MetadataExt;
        // FILETIME uses 100 ns ticks since 1601; registry timestamps use Unix ns.
        let unix_ns = |ticks| (i128::from(ticks) - WINDOWS_TO_UNIX_EPOCH_TICKS) * NANOS_PER_TICK;
        (
            Some(unix_ns(metadata.last_write_time())),
            Some(unix_ns(metadata.creation_time())),
            None,
        )
    };
    Signature {
        size: metadata.len(),
        mtime,
        ctime,
        inode,
    }
}

/// Replace an unchanged regular file through a synced sibling temporary file.
/// `published` runs immediately after rename, before the directory sync, so
/// callers can roll back even when the final durability step fails.
pub(crate) fn atomic_rewrite(
    path: &Path,
    bytes: &[u8],
    expected: &Signature,
    published: impl FnOnce(Signature),
) -> io::Result<()> {
    let check = || {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || signature(&metadata) != *expected {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file changed"));
        }
        if metadata.permissions().readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only file",
            ));
        }
        require_single_link(path).map_err(io::Error::other)?;
        Ok(metadata)
    };
    let metadata = check()?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = temporary_path(parent, TEMP_TAG_NAME);
    let mut output = create_rewrite_temp(&temporary, &metadata.permissions())?;
    let result = (|| {
        output.write_all(bytes)?;
        output.sync_all()?;
        check()?;
        let fallback_signature = signature(&output.metadata()?);
        // Known limit: a rename cannot be conditional on content, so an edit made
        // between the check above and this rename is replaced. See "Known limit:
        // the write window" in docs/dev/register.md.
        fs::rename(&temporary, path)?;
        let metadata = rewrite_metadata(&output, path);
        // Rename may change ctime. Prefer the post-rename signature, but always
        // record publication before propagating a metadata failure. The fallback
        // lets rollback restore the file or report its path if the signature no
        // longer matches; a published name must never be silently forgotten.
        published(
            metadata
                .as_ref()
                .map(signature)
                .unwrap_or(fallback_signature),
        );
        metadata?;
        crate::fs::sync_directory(parent)
    })();
    drop(output);
    let _ = fs::remove_file(temporary);
    result
}

fn rewrite_metadata(output: &File, path: &Path) -> io::Result<fs::Metadata> {
    #[cfg(not(test))]
    let _ = path;
    #[cfg(test)]
    if REWRITE_METADATA_FAILURE.with(|failure| {
        let mut failure = failure.borrow_mut();
        if failure.as_deref() == Some(path) {
            failure.take();
            true
        } else {
            false
        }
    }) {
        return Err(io::Error::other("injected rewrite metadata failure"));
    }
    output.metadata()
}

#[cfg(test)]
thread_local! {
    static REWRITE_METADATA_FAILURE: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
pub(crate) fn with_rewrite_metadata_failure<T>(path: &Path, run: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            REWRITE_METADATA_FAILURE.with(|failure| *failure.borrow_mut() = None);
        }
    }
    REWRITE_METADATA_FAILURE.with(|failure| {
        assert!(failure.borrow().is_none());
        *failure.borrow_mut() = Some(path.to_path_buf());
    });
    let _reset = Reset;
    run()
}

fn create_rewrite_temp(path: &Path, permissions: &fs::Permissions) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(permissions.mode());
    }
    let output = options.open(path)?;
    if let Err(error) = output.set_permissions(permissions.clone()) {
        drop(output);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(output)
}

fn temporary_path(parent: &Path, tag: &str) -> PathBuf {
    parent.join(format!(
        ".{tag}-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}

fn copy_new(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    io::copy(&mut input, &mut output)?;
    output.sync_all()
}

pub(crate) fn require_single_link(path: &Path) -> Result<(), PlacementError> {
    link_count(path)
        .and_then(ensure_single)
        .map_err(|error| failure(path, error))
}

#[cfg(unix)]
fn link_count(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::symlink_metadata(path)?.nlink())
}

#[cfg(windows)]
fn link_count(path: &Path) -> io::Result<u64> {
    let file = File::open(path)?;
    Ok(winapi_util::file::information(&file)?.number_of_links())
}

fn ensure_single(count: u64) -> io::Result<()> {
    if count == 1 {
        Ok(())
    } else {
        Err(io::Error::other("source has other hard links"))
    }
}

fn hard_links_unsupported(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(windows)]
    {
        error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

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
#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[cfg(test)]
mod tests;
