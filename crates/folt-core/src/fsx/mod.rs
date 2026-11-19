use std::io;
use std::path::Path;

mod atomic;
mod links;
mod private;
mod rename;
mod signature;
mod sync;
mod temp;

pub(crate) use atomic::atomic_rewrite;
#[cfg(test)]
pub(crate) use atomic::with_rewrite_metadata_failure;
pub(crate) use links::{copy_new, ensure_single, hard_links_unsupported, require_single_link};
pub(crate) use private::{private_directory_builder, private_file_options};
pub use signature::{Signature, signature};
#[cfg(test)]
pub(crate) use sync::{with_sync_failure, with_sync_failure_after};
pub(crate) use temp::{TempFile, TempMode, temp_name};

pub(crate) trait FileSystem {
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
    fn rename_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
    fn create_symlink(&self, target: &Path, link: &Path) -> io::Result<()>;
    fn link_count(&self, path: &Path) -> io::Result<u64>;
}

pub(crate) struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        sync::sync_directory(path)
    }
    fn rename_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        rename::rename_no_replace(source, destination)
    }
    fn create_symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
        links::create_symlink(target, link)
    }
    fn link_count(&self, path: &Path) -> io::Result<u64> {
        links::link_count(path)
    }
}

#[cfg(test)]
mod tests;
