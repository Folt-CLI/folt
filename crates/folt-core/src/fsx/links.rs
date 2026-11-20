use super::{FileSystem, RealFileSystem};
#[cfg(unix)]
use std::fs;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Windows privilege-not-held error code (dimensionless).
#[cfg(windows)]
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

pub(crate) fn copy_new(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    io::copy(&mut input, &mut output)?;
    output.sync_all()
}

pub(crate) fn require_single_link(path: &Path) -> io::Result<()> {
    RealFileSystem
        .link_count(path)
        .and_then(super::ensure_single)
}

#[cfg(unix)]
pub(super) fn link_count(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::symlink_metadata(path)?.nlink())
}

#[cfg(windows)]
pub(super) fn link_count(path: &Path) -> io::Result<u64> {
    let file = File::open(path)?;
    Ok(winapi_util::file::information(&file)?.number_of_links())
}

pub(crate) fn ensure_single(count: u64) -> io::Result<()> {
    if count == 1 {
        Ok(())
    } else {
        Err(io::Error::other("source has other hard links"))
    }
}

pub(crate) fn hard_links_unsupported(error: &io::Error) -> bool {
    #[cfg(windows)]
    let privilege_missing = error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD);
    #[cfg(not(windows))]
    let privilege_missing = false;
    error.kind() == io::ErrorKind::Unsupported || privilege_missing
}

#[cfg(unix)]
pub(super) fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(windows)]
pub(super) fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[cfg(test)]
mod tests;
