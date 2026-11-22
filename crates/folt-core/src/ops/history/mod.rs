use crate::history::{RefError, RefStore};
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ops::facts::PlacementFacts;
use crate::placement::{Observation, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

/// Stores for one Folio and the caller's bound for commit object reads.
#[derive(Clone, Copy)]
pub struct HistoryStores<'a> {
    pub objects: &'a ObjectStore,
    pub refs: &'a RefStore,
    pub object_limit: u64,
}

fn require_row(registry: &Registry, row: &Placement) -> Result<(), HistoryError> {
    if registry.placement_at(&row.path)?.as_ref() != Some(row) {
        return Err(HistoryRefusal::RegistryChanged(row.path.to_path_buf()).into());
    }
    Ok(())
}

fn require_observation(facts: &PlacementFacts, fs: &impl PlacementOps) -> Result<(), HistoryError> {
    if fs.read(facts.placement.id, &facts.placement.to_placement().1)? != facts.observation {
        return Err(HistoryRefusal::PlacementChanged(facts.placement.path.to_path_buf()).into());
    }
    Ok(())
}

fn gather_file(
    row: &Placement,
    fs: &impl PlacementOps,
    refs: &RefStore,
) -> Result<PlacementFacts, HistoryError> {
    let (_, placement) = row.to_placement();
    let observation = fs.read(row.id, &placement)?;
    let placement_hash = if matches!(observation, Observation::File { .. }) {
        Some(fs.hash_placement(&placement)?)
    } else {
        None
    };
    Ok(PlacementFacts {
        placement: row.clone(),
        observation,
        placement_hash,
        tip: refs.read(
            RefKind::Branch,
            &RefName::parse(&row.head).map_err(RefError::from)?,
        )?,
    })
}

fn require_blob(objects: &ObjectStore, id: ObjectId) -> Result<(), HistoryError> {
    let actual = objects.kind_of(id)?;
    if actual != ObjectKind::Blob {
        return Err(RefError::WrongKind { id, actual }.into());
    }
    Ok(())
}

mod branch;
mod commit;
mod error;
mod merge;
mod rebase;
mod refusal;
mod reset;
mod tag;

#[cfg(test)]
pub(crate) use branch::judge_delete_branch;
pub(crate) use branch::{create_branch, delete_branch};
pub use commit::{CommitFacts, CommitMetadata, CommitPlan};
pub(crate) use commit::{apply_commit, gather_commit, judge_commit};
pub use error::HistoryError;
pub(crate) use merge::conflict::conflict_state;
pub use merge::{MergeOutcome, MergePlan, MergeRequest};
pub(crate) use merge::{abort_merge, continue_merge, merge_branch};
pub use rebase::{LeftBehind, RebaseFacts, RebasePlan};
pub(crate) use rebase::{apply_rebase, gather_rebase, judge_rebase};
pub use refusal::HistoryRefusal;
pub use reset::{ResetFacts, ResetPlan};
pub(crate) use reset::{apply_reset, gather_reset, judge_reset};
pub(crate) use tag::{create_tag, delete_tag};

#[cfg(test)]
mod tests;
