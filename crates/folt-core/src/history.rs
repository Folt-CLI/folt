//! The ref store of one Folio. The history operations live in `ops/history`.

mod error;
mod expect;
mod ref_store;

pub use error::RefError;
pub use expect::Expect;
pub use ref_store::RefStore;
