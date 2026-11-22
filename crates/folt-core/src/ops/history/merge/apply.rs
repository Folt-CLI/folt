use super::{MergeOutcome, MergePlan, MergeRequest, gather_merge, judge::judge_merge};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::{ObjectKind, hash_blob};
use crate::ops::history::commit::{
    Recovery, advance_branch, branch_moved, build_commit, move_internal, write_rows,
};
use crate::ops::history::{
    CommitMetadata, HistoryError, HistoryRefusal, HistoryStores, require_observation, require_row,
};
use crate::ops::placement::move_::{MoveTarget, apply_move_file, judge_move};
use crate::ops::stage::{stage_references, validate_merged};
use crate::placement::{Observation, PlacementOps, StagedBlob};
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};

/// Combines another branch from one clean working file under the home lock.
pub(crate) fn merge_branch(
    row: &Placement,
    request: MergeRequest<'_>,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<MergeOutcome, HistoryError> {
    let facts = gather_merge(row, request.other, fs, stores, registry)?;
    let plan = judge_merge(&facts)?;
    match &plan {
        MergePlan::Clean(_) => apply_merge(&plan, request.metadata, fs, stores, registry, lock)
            .map(MergeOutcome::Merged),
        MergePlan::Conflicted { .. } => {
            super::conflict::apply_conflict(&plan, fs, stores, registry, lock)
                .map(MergeOutcome::Conflicted)
        }
    }
}

pub(super) fn apply_merge(
    plan: &MergePlan,
    metadata: CommitMetadata,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    let MergePlan::Clean(plan) = plan else {
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
    validate_merged(row, bytes, registry).map_err(HistoryRefusal::MergeInvalid)?;
    let references = stage_references(row, bytes)
        .map_err(|error| HistoryRefusal::MergeInvalid(error.to_string()))?;
    let content = StagedBlob {
        hash: hash_blob(bytes),
        size: bytes.len() as u64,
    };
    let commit = build_commit(
        content.hash,
        vec![
            tip.ok_or(HistoryRefusal::MissingBranch)?,
            plan.facts.other.ok_or(HistoryRefusal::MissingBranch)?,
        ],
        metadata,
    )?;
    stores.objects.put(lock, ObjectKind::Blob, bytes)?;
    let id = stores.objects.put_commit(lock, &commit)?;
    stores.objects.sync_blobs()?;
    let (id, name) = advance_branch(id, &commit, &mut facts, Recovery::Refuse, stores, lock)?;
    let target = MoveTarget {
        branch: name,
        commit: id,
        content: content.hash,
    };
    let internal = move_internal(&facts, target.clone(), fs, stores.objects, registry, lock)?;
    let movement = judge_move(&facts.working, &target, false)?;
    let base = apply_move_file(&movement, fs, stores.objects, registry, lock)?
        .ok_or(HistoryRefusal::InvalidFacts)?;
    facts.working.observation = Observation::File {
        signature: base.signature,
    };
    facts.working.placement_hash = Some(content.hash);
    write_rows(
        &facts,
        id,
        content,
        internal,
        Some(&references),
        registry,
        lock,
    )?;
    Ok(id)
}

#[cfg(test)]
pub(crate) fn merge_branch_steps(
    row: &Placement,
    request: MergeRequest<'_>,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    hook: impl crate::ops::step_hook::StepHook<super::conflict::ConflictStep, HistoryError>,
) -> Result<MergeOutcome, HistoryError> {
    let facts = gather_merge(row, request.other, fs, stores, registry)?;
    let plan = judge_merge(&facts)?;
    match &plan {
        MergePlan::Clean(_) => apply_merge(&plan, request.metadata, fs, stores, registry, lock)
            .map(MergeOutcome::Merged),
        MergePlan::Conflicted { .. } => {
            super::conflict::apply_conflict_steps(&plan, fs, stores, registry, lock, hook)
                .map(MergeOutcome::Conflicted)
        }
    }
}
