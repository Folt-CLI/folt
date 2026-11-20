//! Prunes absent external placement rows, never files or objects.

mod apply;
mod gather;
mod judge;
mod model;
mod report;

pub use apply::{apply, prune_placements};
pub use gather::gather;
pub use judge::{judge, needs_confirmation};
pub use model::*;

#[cfg(test)]
mod tests;
