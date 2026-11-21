use std::io;
use std::path::Path;

/// Syncs directory entries on Unix, propagating open and sync errors.
///
/// This is a no-op on Windows: `File::open` cannot open a directory there.
/// Directory durability is therefore not guaranteed on Windows; callers still
/// sync regular files separately.
pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    sync_directory_impl(path)
}

#[cfg(unix)]
fn sync_directory_impl(path: &Path) -> io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory_impl(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests;
