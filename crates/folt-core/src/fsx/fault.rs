use std::cell::{Cell, RefCell};
use std::fs::{File, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use super::{FileSystem, RealFileSystem};

#[derive(Default)]
pub(crate) struct FaultFileSystem {
    sync_failure: Option<(PathBuf, Cell<usize>)>,
    rewrite_metadata_failure: RefCell<Option<PathBuf>>,
}

impl FaultFileSystem {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_failure(mut self, path: &Path, successful: usize) -> Self {
        self.sync_failure = Some((path.to_path_buf(), Cell::new(successful)));
        self
    }

    pub(crate) fn rewrite_metadata_failure(self, path: &Path) -> Self {
        *self.rewrite_metadata_failure.borrow_mut() = Some(path.to_path_buf());
        self
    }
}

impl FileSystem for FaultFileSystem {
    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        if let Some((target, remaining)) = &self.sync_failure
            && (target == path
                || target
                    .canonicalize()
                    .is_ok_and(|target| path.canonicalize().is_ok_and(|path| target == path)))
        {
            if remaining.get() == 0 {
                return Err(io::Error::other("injected directory sync failure"));
            }
            RealFileSystem.sync_directory(path)?;
            remaining.set(remaining.get() - 1);
            return Ok(());
        }
        RealFileSystem.sync_directory(path)
    }

    fn rewrite_metadata(&self, output: &File, path: &Path) -> io::Result<Metadata> {
        let mut failure = self.rewrite_metadata_failure.borrow_mut();
        if failure.as_deref() == Some(path) {
            failure.take();
            return Err(io::Error::other("injected rewrite metadata failure"));
        }
        RealFileSystem.rewrite_metadata(output, path)
    }

    fn rename_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        RealFileSystem.rename_no_replace(source, destination)
    }

    fn create_symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
        RealFileSystem.create_symlink(target, link)
    }

    fn link_count(&self, path: &Path) -> io::Result<u64> {
        RealFileSystem.link_count(path)
    }
}

#[cfg(test)]
mod tests;
