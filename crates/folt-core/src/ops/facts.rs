use crate::id::ObjectId;
use crate::placement::{self, Observation};
use crate::registry;

/// Read-only facts used by status and move decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementFacts {
    pub placement: registry::Placement,
    pub observation: Observation,
    pub placement_hash: Option<ObjectId>,
    pub tip: Option<ObjectId>,
}

pub(crate) fn needs_hash(row: &registry::Placement, observation: &Observation) -> bool {
    placement::needs_hash(&row.to_placement().1, observation)
        || matches!((observation, row.staged),
            (Observation::File { signature }, Some(staged)) if signature.size == staged.size)
}

/// Tip comparison only; this does not inspect commit ancestry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Behind {
    UpToDate,
    Behind {
        tip: ObjectId,
    },
    NoHistory,
    Unrecorded,
    /// A recorded commit remains, but its branch ref is missing.
    MissingTip,
}
