//! The ref store and commit ancestry of one Folio. The history operations live in `ops/history`.

mod ancestry;
mod error;
mod expect;
mod ref_store;

pub use ancestry::{MergeBase, commit_range, is_ancestor, merge_base};
pub use error::RefError;
pub use expect::Expect;
pub use ref_store::RefStore;
