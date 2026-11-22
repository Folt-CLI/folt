use super::{RebaseOutcome, RebasePlan};
use crate::home::HomeLock;
use crate::object::{ObjectKind, hash_blob};
use crate::ops::history::{HistoryError, HistoryRefusal, HistoryStores, gather_file, require_row};
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::step_hook::{NoHook, StepHook};
use crate::placement::{ConflictOperation, ConflictState, PlacementOps};
use crate::placement_path::RegistryError;
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry, StoppedRebase};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RebaseStep {
    RowWritten,
    FileWritten,
    ObjectsPublished,
    BranchMoved,
}

pub(super) fn apply_stop(
    plan: &RebasePlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<RebaseOutcome, HistoryError> {
    apply_stop_steps(plan, fs, stores, registry, lock, NoHook)
}

pub(super) fn apply_stop_steps(
    plan: &RebasePlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    mut hook: impl StepHook<RebaseStep, HistoryError>,
) -> Result<RebaseOutcome, HistoryError> {
    let stop = plan.stop.as_ref().ok_or(HistoryRefusal::InvalidFacts)?;
    let path = plan
        .facts
        .copy
        .as_ref()
        .ok_or(HistoryRefusal::InvalidFacts)?;
    let row = registry
        .placement_at(path)?
        .ok_or(HistoryRefusal::InvalidFacts)?;
    let facts = gather_file(&row, fs, stores.refs)?;
    let original = read_file(&row)?;
    if facts.placement_hash != Some(hash_blob(&original)) {
        return Err(HistoryRefusal::PlacementChanged(path.to_path_buf()).into());
    }
    for (_, commit, bytes) in &plan.replayed {
        stores.objects.put(lock, ObjectKind::Blob, bytes)?;
        stores.objects.put_commit(lock, commit)?;
    }
    let pre_merge = stores.objects.put(lock, ObjectKind::Blob, &original)?;
    let marker_blob = stores.objects.put(lock, ObjectKind::Blob, &stop.bytes)?;
    stores.objects.sync_blobs()?;
    hook.on_step(RebaseStep::ObjectsPublished)?;
    let state = ConflictState {
        operation: ConflictOperation::Rebase,
        other: stop.current,
        marker_lines: stop.marker_lines.clone(),
        pre_merge,
    };
    let mut progress = StoppedRebase {
        id: row.id,
        branch: row.head.clone(),
        copy: path.clone(),
        original_tip: plan.facts.reset.old,
        destination: plan.facts.reset.target.commit,
        remaining: stop.remaining.clone(),
        current_replay: Some(stop.current),
        new_tip: plan.tip,
        file_before: pre_merge,
        marker_blob: Some(marker_blob),
        discard: plan.discard,
    };
    registry.write(lock, |tx| {
        tx.insert_stopped_rebase(&progress)?;
        tx.update_conflict_state(path, &state)
    })?;
    hook.on_step(RebaseStep::RowWritten)?;
    fs.refresh(
        row.id,
        &row.to_placement().1,
        stores.objects,
        marker_blob,
        Some(&facts.observation),
    )?;
    // Acknowledging publication lets a resolution equal to the original bytes
    // differ from recovery of a row written before its marker file.
    progress.file_before = marker_blob;
    registry.write(lock, |tx| tx.update_stopped_rebase(&progress))?;
    hook.on_step(RebaseStep::FileWritten)?;
    Ok(RebaseOutcome::Stopped(state))
}

pub(crate) fn continue_rebase(
    row: &Placement,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<RebaseOutcome, HistoryError> {
    super::resume::continue_steps(row, fs, stores, registry, lock, NoHook)
}

pub(super) fn checked(
    row: &Placement,
    registry: &Registry,
    stores: &HistoryStores<'_>,
) -> Result<StoppedRebase, HistoryError> {
    let progress = registry
        .stopped_rebase(row.id)?
        .ok_or(HistoryRefusal::NoStoppedRebase)?;
    require_row(registry, row)?;
    if progress.copy != row.path
        || progress.branch != row.head
        || row
            .conflict
            .as_ref()
            .is_none_or(|state| state.operation != ConflictOperation::Rebase)
    {
        return Err(HistoryRefusal::InvalidFacts.into());
    }
    let branch = RefName::parse(&progress.branch).map_err(crate::history::RefError::from)?;
    let tip = stores.refs.read(RefKind::Branch, &branch)?;
    if tip != Some(progress.original_tip)
        && !(progress.current_replay.is_none() && tip == Some(progress.new_tip))
    {
        return Err(HistoryRefusal::BranchMoved {
            tip,
            ahead: Vec::new(),
        }
        .into());
    }
    Ok(progress)
}

pub(crate) fn abort_rebase(
    row: &Placement,
    discard: bool,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    let progress = checked(row, registry, stores)?;
    if progress.current_replay.is_none() {
        return Err(HistoryRefusal::InvalidFacts.into());
    }
    let state = row.conflict.as_ref().ok_or(HistoryRefusal::InvalidFacts)?;
    let facts = gather_file(row, fs, stores.refs)?;
    if !discard
        && facts.placement_hash != progress.marker_blob
        && facts.placement_hash != Some(progress.file_before)
        && facts.placement_hash != Some(state.pre_merge)
    {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::Edited,
        }
        .into());
    }
    let written = fs.refresh(
        row.id,
        &row.to_placement().1,
        stores.objects,
        state.pre_merge,
        Some(&facts.observation),
    )?;
    let mut restored = row.clone();
    restored.base = written.base;
    restored.conflict = None;
    registry.write(lock, |tx| {
        tx.update_placement(&restored)?;
        tx.remove_stopped_rebase(row.id)
    })?;
    Ok(())
}

pub(super) fn read_file(row: &Placement) -> Result<Vec<u8>, HistoryError> {
    Ok(std::fs::read(row.path.to_path_buf()).map_err(RegistryError::from)?)
}
