use std::fs::{File, Metadata};
use std::io;
use std::path::Path;

mod atomic;
#[cfg(test)]
pub(crate) mod fault;
mod links;
mod private;
mod rename;
mod signature;
mod sync;
mod temp;

pub(crate) use atomic::atomic_rewrite;
pub(crate) use links::{copy_new, ensure_single, hard_links_unsupported, require_single_link};
pub(crate) use private::{private_directory_builder, private_file_options};
pub use signature::{Signature, signature};
pub(crate) use temp::{TempFile, TempMode, temp_name};

pub(crate) trait FileSystem {
    fn rewrite_metadata(&self, output: &File, path: &Path) -> io::Result<Metadata>;
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
    fn rename_no_replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
    fn create_symlink(&self, target: &Path, link: &Path) -> io::Result<()>;
    fn link_count(&self, path: &Path) -> io::Result<u64>;
}

pub(crate) struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn rewrite_metadata(&self, output: &File, _path: &Path) -> io::Result<Metadata> {
        output.metadata()
    }
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
