use std::path::{Path, PathBuf};

use crate::id::FolioId;

mod create;
mod error;
mod inspect;
pub mod path;

pub(crate) use create::create_folio_with;
pub use create::{create_folio, sweep_new};
pub use error::FolioDirError;
pub use inspect::{Fix, Inspection, Problem, inspect, validate};

/// Content used to create a Folio directory.
pub struct FolioSpec {
    /// Content bytes; an empty vector represents an empty body.
    pub content: Vec<u8>,
}

/// Requested durability for a newly created Folio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Durability {
    /// Rely on normal filesystem write behavior.
    None,
    /// Sync the content and containing directories before returning.
    DataAndDirectories,
}

/// A validated Folio directory.
#[derive(Clone, Debug)]
pub struct FolioDir {
    id: FolioId,
    path: PathBuf,
}

impl FolioDir {
    pub fn id(&self) -> FolioId {
        self.id
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests;
