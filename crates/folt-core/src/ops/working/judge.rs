use crate::id::ObjectId;
use crate::ops::facts::{Behind, PlacementFacts};
use crate::placement::Observation;

use super::{Edit, Staged, WorkingStatus};

/// Compares only the supplied observations; no commits or files are read.
pub fn judge_working(facts: &PlacementFacts) -> WorkingStatus {
    let row = &facts.placement;
    let matches = |hash: ObjectId, size: u64| {
        matches!(&facts.observation, Observation::File { signature } if signature.size == size)
            && facts.placement_hash == Some(hash)
    };
    let edit = if row
        .base
        .as_ref()
        .is_some_and(|base| matches(base.hash, base.signature.size))
    {
        Edit::Clean
    } else {
        Edit::Edited
    };
    let staged = match row.staged {
        None => Staged::None,
        Some(staged) if matches(staged.hash, staged.size) => Staged::Staged,
        Some(_) => Staged::StagedThenModified,
    };
    let behind = match (row.commit_id, facts.tip) {
        (None, None) => Behind::NoHistory,
        (None, Some(_)) => Behind::Unrecorded,
        (Some(_), None) => Behind::MissingTip,
        (Some(commit), Some(tip)) if commit == tip => Behind::UpToDate,
        (Some(_), Some(tip)) => Behind::Behind { tip },
    };
    WorkingStatus {
        edit,
        staged,
        behind,
    }
}
