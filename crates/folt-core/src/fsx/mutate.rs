use std::fs::{self, File, Permissions};
use std::io;
use std::path::Path;

pub(crate) fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    fs::remove_file(path)
}

pub(crate) fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    fs::remove_dir(path)
}

pub(crate) fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    fs::remove_dir_all(path)
}

pub(crate) fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(path)
}

pub(crate) fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    fs::rename(from, to)
}

pub(crate) fn hard_link(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    fs::hard_link(from, to)
}

pub(crate) fn set_permissions(path: impl AsRef<Path>, permissions: Permissions) -> io::Result<()> {
    fs::set_permissions(path, permissions)
}

pub(crate) fn sync_file(file: &File) -> io::Result<()> {
    file.sync_all()
}

pub(crate) fn set_file_permissions(file: &File, permissions: Permissions) -> io::Result<()> {
    file.set_permissions(permissions)
}
