use super::MergePlan;
use crate::home::HomeLock;
use crate::object::{ObjectKind, hash_blob};
use crate::ops::history::commit::branch_moved;
use crate::ops::history::{
    HistoryError, HistoryRefusal, HistoryStores, require_observation, require_row,
};
use crate::ops::step_hook::{NoHook, StepHook};
use crate::placement::{ConflictOperation, ConflictState, PlacementOps};
use crate::placement_path::{PlacementPath, RegistryError};
use crate::ref_name::{RefKind, RefName};
use crate::registry::Registry;

pub(crate) fn conflict_state(
    path: &PlacementPath,
    registry: &Registry,
) -> Result<Option<ConflictState>, RegistryError> {
    Ok(registry.placement_at(path)?.and_then(|row| row.conflict))
}

pub(super) fn apply_conflict(
    plan: &MergePlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<ConflictState, HistoryError> {
    apply_conflict_steps(plan, fs, stores, registry, lock, NoHook)
}

/// The conflict file has been published, but its row has not been written yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConflictStep {
    FileWritten,
}

pub(crate) fn apply_conflict_steps(
    plan: &MergePlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    mut hook: impl StepHook<ConflictStep, HistoryError>,
) -> Result<ConflictState, HistoryError> {
    let MergePlan::Conflicted { plan, marker_lines } = plan else {
        return Err(HistoryRefusal::InvalidFacts.into());
    };
    let bytes = &plan.bytes;
    let mut facts = plan.facts.history.clone();
    require_row(registry, &facts.working.placement)?;
    require_observation(&facts.working, fs)?;
    if let Some(internal) = &facts.internal {
        require_row(registry, &internal.placement)?;
        require_observation(internal, fs)?;
    }
    let row = &facts.working.placement;
    let branch = RefName::parse(&row.head).map_err(crate::history::RefError::from)?;
    let tip = stores.refs.read(RefKind::Branch, &branch)?;
    if tip != facts.working.tip {
        facts.working.tip = tip;
        return Err(branch_moved(&facts).into());
    }
    let original = std::fs::read(row.path.to_path_buf()).map_err(RegistryError::from)?;
    if Some(hash_blob(&original)) != facts.working.placement_hash {
        return Err(HistoryRefusal::PlacementChanged(row.path.to_path_buf()).into());
    }
    let state = ConflictState {
        operation: ConflictOperation::Merge,
        other: plan.facts.other.ok_or(HistoryRefusal::MissingBranch)?,
        marker_lines: marker_lines.clone(),
        pre_merge: stores.objects.put(lock, ObjectKind::Blob, &original)?,
    };
    let content = stores.objects.put(lock, ObjectKind::Blob, bytes)?;
    stores.objects.sync_blobs()?;
    fs.refresh(
        row.id,
        &row.to_placement().1,
        stores.objects,
        content,
        Some(&facts.working.observation),
    )?;
    hook.on_step(ConflictStep::FileWritten)?;
    registry.write(lock, |tx| {
        if tx.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(RegistryError::PlacementChanged(row.path.clone()));
        }
        tx.update_conflict_state(&row.path, &state)
    })?;
    Ok(state)
}
