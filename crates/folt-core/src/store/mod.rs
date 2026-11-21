use std::io::{Read, Seek};
use std::path::PathBuf;

use crate::config::names::{BLOB_EXTENSION, BLOBS_DIR};
use crate::folio::dir::FolioDir;
use crate::folio::format::{FolioFormat, FormatError, check_format};
use crate::fsx::{FileSystem, RealFileSystem};
use crate::id::ObjectId;

mod error;
mod maintenance;
mod probe;
mod read;
mod source;
mod write;

pub use error::StoreError;

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Immutable objects stored beneath one Folio's `blobs` directory.
pub struct ObjectStore {
    fs: Box<dyn FileSystem>,
    root: PathBuf,
    format: FolioFormat,
}

impl ObjectStore {
    pub fn open(folio: &FolioDir, format: &FolioFormat) -> Result<Self, StoreError> {
        Self::open_with(folio, format, Box::new(RealFileSystem))
    }

    pub(crate) fn open_with(
        folio: &FolioDir,
        format: &FolioFormat,
        fs: Box<dyn FileSystem>,
    ) -> Result<Self, StoreError> {
        check_format(format).map_err(|error| match error {
            FormatError::Newer { found, supported } => StoreError::NewerFormat { found, supported },
            FormatError::Older { found, supported } => StoreError::OlderFormat { found, supported },
            FormatError::UnsupportedAlgorithm(value) => StoreError::UnsupportedAlgorithm(value),
        })?;
        Ok(Self {
            fs,
            root: folio.path().join(BLOBS_DIR),
            format: format.clone(),
        })
    }
    pub fn format(&self) -> FolioFormat {
        self.format.clone()
    }
    fn path(&self, id: ObjectId) -> PathBuf {
        self.root.join(format!("{id}{BLOB_EXTENSION}"))
    }
}
