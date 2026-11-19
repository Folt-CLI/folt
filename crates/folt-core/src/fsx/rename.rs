#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use std::fs;
use std::io;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(io::Error::from)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests;
