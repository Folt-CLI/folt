use super::{Observation, PlacementKind, PlacementSpec};
use crate::id::ObjectId;

/// The outcome of comparing a copy with its base or observing a link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Drift {
    InSync,
    PlacementEdited,
    Gone,
    Detached,
    Unusable,
}

/// Whether the caller must hash content before classifying drift.
pub fn needs_hash(placement: &PlacementSpec, observation: &Observation) -> bool {
    placement.kind != PlacementKind::Link
        && matches!(observation, Observation::File { signature }
            if placement.base.as_ref().is_none_or(|base| base.signature.size == signature.size))
}

/// Classifies a placement using facts and hashes supplied by the caller.
pub fn classify(
    placement: &PlacementSpec,
    observation: &Observation,
    placement_hash: Option<ObjectId>,
) -> Drift {
    match observation {
        Observation::Missing => Drift::Gone,
        Observation::Directory | Observation::NotAFile => Drift::Unusable,
        Observation::IntactLink if placement.kind == PlacementKind::Link => Drift::InSync,
        Observation::OtherLink { .. } | Observation::File { .. }
            if placement.kind == PlacementKind::Link =>
        {
            Drift::Detached
        }
        Observation::File { signature } => {
            let Some(base) = &placement.base else {
                return if placement_hash.is_some() {
                    Drift::PlacementEdited
                } else {
                    Drift::Unusable
                };
            };
            if signature.size != base.signature.size {
                return Drift::PlacementEdited;
            }
            match placement_hash {
                Some(hash) if hash == base.hash => Drift::InSync,
                Some(_) => Drift::PlacementEdited,
                None => Drift::Unusable,
            }
        }
        Observation::IntactLink | Observation::OtherLink { .. } => Drift::Unusable,
    }
}
