use crate::config::names::DEFAULT_BRANCH;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ops::history::commit::{
    Recovery, advance_branch, branch_moved, build_commit, judge_commit_with_parents,
    matches_requested_commit, move_internal, write_rows,
};
use crate::ops::history::{
    CommitFacts, CommitMetadata, HistoryError, HistoryRefusal, HistoryStores, gather_file,
    require_blob, require_observation, require_row,
};
use crate::ops::placement::move_::MoveTarget;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::stage::refuse::markers_remain;
use crate::ops::step_hook::{NoHook, StepHook};
use crate::placement::{ConflictOperation, PlacementKind, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};

fn gather_continue(
    row: &Placement,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<CommitFacts, HistoryError> {
    require_row(registry, row)?;
    let working = gather_file(row, fs, stores.refs)?;
    let placements = registry.placements_of_head(row.id, &row.head)?;
    let internal = if row.kind == PlacementKind::Copy && row.head == DEFAULT_BRANCH {
        registry
            .internal_placement(row.id)?
            .map(|row| gather_file(&row, fs, stores.refs))
            .transpose()?
    } else {
        None
    };
    let tip_commit = working
        .tip
        .map(|id| stores.objects.get_commit(id, stores.object_limit))
        .transpose()?;
    Ok(CommitFacts {
        working,
        link: None,
        internal,
        placements,
        tip_commit,
    })
}

fn judge_continue(facts: &CommitFacts) -> Result<(), HistoryRefusal> {
    let row = &facts.working.placement;
    if !row
        .conflict
        .as_ref()
        .is_some_and(|state| state.operation == ConflictOperation::Merge)
    {
        return Err(HistoryRefusal::NotConflicted(row.path.to_path_buf()));
    }
    let state = row.conflict.as_ref().ok_or(HistoryRefusal::InvalidFacts)?;
    let parents = [
        row.commit_id.ok_or(HistoryRefusal::MissingBranch)?,
        state.other,
    ];
    judge_commit_with_parents(facts, &parents)?;
    Ok(())
}

/// Publishes the staged resolution as a merge commit and clears its conflict state.
pub(crate) fn continue_merge(
    row: &Placement,
    metadata: CommitMetadata,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    continue_steps(row, metadata, fs, stores, registry, lock, NoHook)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContinueStep {
    BranchPublished,
    InternalMoved,
}

fn continue_steps(
    row: &Placement,
    metadata: CommitMetadata,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    mut hook: impl StepHook<ContinueStep, HistoryError>,
) -> Result<ObjectId, HistoryError> {
    let mut facts = gather_continue(row, fs, stores, registry)?;
    judge_continue(&facts)?;
    require_row(registry, row)?;
    require_observation(&facts.working, fs)?;
    if let Some(internal) = &facts.internal {
        require_row(registry, &internal.placement)?;
        require_observation(internal, fs)?;
    }
    let branch = RefName::parse(&row.head).map_err(crate::history::RefError::from)?;
    let tip = stores.refs.read(RefKind::Branch, &branch)?;
    if tip != facts.working.tip {
        facts.working.tip = tip;
        return Err(branch_moved(&facts).into());
    }
    let state = row.conflict.as_ref().ok_or(HistoryRefusal::InvalidFacts)?;
    let staged = row.staged.ok_or(HistoryRefusal::NothingStaged)?;
    require_blob(stores.objects, staged.hash)?;
    let mut bytes = Vec::new();
    stores
        .objects
        .get_to(staged.hash, ObjectKind::Blob, &mut bytes)?;
    if markers_remain(state, &bytes) {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::ConflictMarkers,
        }
        .into());
    }
    let commit = build_commit(
        staged.hash,
        vec![
            row.commit_id.ok_or(HistoryRefusal::MissingBranch)?,
            state.other,
        ],
        metadata,
    )?;
    let id = if facts.working.tip != row.commit_id {
        if !facts
            .tip_commit
            .as_ref()
            .is_some_and(|tip| matches_requested_commit(tip, &commit))
        {
            return Err(branch_moved(&facts).into());
        }
        judge_commit_with_parents(&facts, &commit.parents)?
            .resumed_commit()
            .ok_or_else(|| branch_moved(&facts))?
    } else {
        stores.objects.put_commit(lock, &commit)?
    };
    stores.objects.sync_blobs()?;
    let (id, name) = advance_branch(id, &commit, &mut facts, Recovery::Resume, stores, lock)?;
    hook.on_step(ContinueStep::BranchPublished)?;
    let target = MoveTarget {
        branch: name,
        commit: id,
        content: staged.hash,
    };
    let internal = move_internal(&facts, target, fs, stores.objects, registry, lock)?;
    hook.on_step(ContinueStep::InternalMoved)?;
    write_rows(&facts, id, staged, internal, None, registry, lock)?;
    Ok(id)
}

#[cfg(test)]
mod tests;
