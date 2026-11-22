use crate::config::names::DEFAULT_BRANCH;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::ops::history::commit::{
    Recovery, advance_branch, branch_moved, build_commit, move_internal, write_rows,
};
use crate::ops::history::{
    CommitFacts, CommitMetadata, HistoryError, HistoryRefusal, HistoryStores, gather_file,
    judge_commit, require_blob, require_observation, require_row,
};
use crate::ops::placement::move_::MoveTarget;
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
    Ok(CommitFacts {
        working,
        link: None,
        internal,
        placements,
        // Continue refuses interrupted publication rather than resuming a commit.
        tip_commit: None,
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
    let mut judged = facts.clone();
    judged.working.placement.conflict = None;
    judge_commit(&judged)?;
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
    let commit = build_commit(
        staged.hash,
        vec![
            row.commit_id.ok_or(HistoryRefusal::MissingBranch)?,
            state.other,
        ],
        metadata,
    )?;
    let id = stores.objects.put_commit(lock, &commit)?;
    stores.objects.sync_blobs()?;
    let (id, name) = advance_branch(id, &commit, &mut facts, Recovery::Refuse, stores, lock)?;
    let target = MoveTarget {
        branch: name,
        commit: id,
        content: staged.hash,
    };
    let internal = move_internal(&facts, target, fs, stores.objects, registry, lock)?;
    write_rows(&facts, id, staged, internal, None, registry, lock)?;
    Ok(id)
}
