mod drift;
mod error;
pub mod fs;
mod kind;
mod model;
mod ops;

pub use drift::{Drift, classify, needs_hash};
pub use error::PlacementError;
pub use kind::PlacementKind;
pub use model::{Observation, PlacementBase, PlacementSpec, StagedBlob, Written};
pub use ops::PlacementOps;

#[cfg(test)]
mod tests;
