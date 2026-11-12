//! Owns the Folio id and the object id, importing config and external crates only.

mod folio;
mod object;

pub use folio::{FolioId, InvalidFolioId};
pub use object::{InvalidObjectId, ObjectId};

#[cfg(test)]
mod tests;
