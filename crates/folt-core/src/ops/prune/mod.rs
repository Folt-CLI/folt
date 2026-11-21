//! Prunes absent external placement rows, never files or objects.

mod apply;
mod gather;
mod judge;
mod model;
mod report;

pub(crate) use apply::{apply, prune_placements};
pub(crate) use gather::gather;
#[cfg(test)]
pub(crate) use judge::{judge, needs_confirmation};
pub use model::*;

#[cfg(test)]
mod tests;
