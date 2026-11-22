use super::{RebaseOutcome, stopped::RebaseStep};
use crate::history::Expect;
use crate::home::HomeLock;
use crate::ops::history::{
    HistoryError, HistoryRefusal, HistoryStores, gather_file, require_observation,
};
use crate::ops::placement::move_::{MoveTarget, judge_move};
use crate::ops::step_hook::StepHook;
use crate::placement::{PlacementKind, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry, StoppedRebase};

pub(super) fn finish(
    row: &Placement,
    progress: &StoppedRebase,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    mut hook: impl StepHook<RebaseStep, HistoryError>,
) -> Result<RebaseOutcome, HistoryError> {
    let branch = RefName::parse(&progress.branch).map_err(crate::history::RefError::from)?;
    let target = MoveTarget {
        branch: branch.clone(),
        commit: progress.new_tip,
        content: stores
            .objects
            .get_commit(progress.new_tip, stores.object_limit)?
            .content,
    };
    let mut moves = Vec::new();
    for placement in registry.placements_of_head(row.id, &progress.branch)? {
        if placement.kind == PlacementKind::Link
            || (placement.path != row.path && placement.conflict.is_some())
        {
            continue;
        }
        let mut facts = gather_file(&placement, fs, stores.refs)?;
        if placement.path == row.path
            && facts.placement_hash != Some(progress.file_before)
            && facts.placement_hash != Some(target.content)
        {
            return Err(HistoryRefusal::PlacementChanged(row.path.to_path_buf()).into());
        }
        facts.placement.conflict = None;
        // The resolved copy is the source of the new content. Other placements
        // retain the reset rules, including recognition of already moved bytes.
        judge_move(
            &facts,
            &target,
            progress.discard || placement.path == row.path,
        )?;
        facts.placement = placement;
        moves.push(facts);
    }
    for facts in &moves {
        require_observation(facts, fs)?;
    }
    let tip = stores.refs.read(RefKind::Branch, &branch)?;
    if tip == Some(progress.original_tip) {
        stores.refs.update(
            lock,
            RefKind::Branch,
            &branch,
            progress.new_tip,
            Expect::Is(progress.original_tip),
        )?;
        stores.refs.sync_refs()?;
    } else if tip != Some(progress.new_tip) {
        return Err(HistoryRefusal::BranchMoved {
            tip,
            ahead: Vec::new(),
        }
        .into());
    }
    hook.on_step(RebaseStep::BranchMoved)?;
    for facts in moves {
        let mut updated = facts.placement;
        let written = fs.refresh(
            updated.id,
            &updated.to_placement().1,
            stores.objects,
            target.content,
            Some(&facts.observation),
        )?;
        hook.on_step(RebaseStep::FileWritten)?;
        updated.commit_id = Some(progress.new_tip);
        updated.base = written.base;
        updated.staged = None;
        // Keep the initiating conflict state until every placement is moved.
        registry.write(lock, |tx| tx.update_placement(&updated))?;
    }
    let mut updated = registry
        .placement_at(&row.path)?
        .ok_or(HistoryRefusal::InvalidFacts)?;
    updated.conflict = None;
    registry.write(lock, |tx| {
        tx.update_placement(&updated)?;
        tx.remove_stopped_rebase(row.id)
    })?;
    Ok(RebaseOutcome::Finished(progress.new_tip))
}
