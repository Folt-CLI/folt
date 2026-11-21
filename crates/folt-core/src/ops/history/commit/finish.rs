use super::super::{HistoryError, HistoryRefusal, HistoryStores, judge_commit};
use super::gather::CommitFacts;
use super::judge::branch_moved;
use super::plan::{CommitPlan, matches_requested_commit};
use crate::fsx::Signature;
use crate::history::{Expect, RefError};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::Commit;
use crate::ops::placement::move_::{MoveTarget, apply_move_file, judge_move, moved_row};
use crate::placement::{Observation, PlacementBase, PlacementOps};
use crate::placement_path::RegistryError;
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

pub(super) fn advance_branch(
    mut id: ObjectId,
    commit: &Commit,
    facts: &mut CommitFacts,
    stores: &HistoryStores<'_>,
    lock: &mut HomeLock,
) -> Result<(ObjectId, RefName), HistoryError> {
    let HistoryStores {
        objects,
        refs,
        object_limit,
    } = *stores;
    let row = &facts.working.placement;
    let name = RefName::parse(&row.head).map_err(RefError::from)?;
    let expect = row.commit_id.map_or(Expect::Absent, Expect::Is);
    match refs.update(lock, RefKind::Branch, &name, id, expect) {
        Ok(()) => {}
        Err(RefError::Stale { actual }) => {
            facts.working.tip = actual;
            facts.tip_commit = actual
                .map(|id| objects.get_commit(id, object_limit))
                .transpose()?;
            if !facts
                .tip_commit
                .as_ref()
                .is_some_and(|tip| matches_requested_commit(tip, commit))
            {
                return Err(branch_moved(facts).into());
            }
            id = judge_commit(facts)?
                .resume
                .ok_or_else(|| branch_moved(facts))?;
        }
        Err(error) => return Err(error.into()),
    }
    refs.sync_refs()?;
    Ok((id, name))
}

pub(super) fn move_internal(
    facts: &CommitFacts,
    id: ObjectId,
    name: RefName,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &Registry,
    lock: &HomeLock,
) -> Result<Option<(Placement, Placement)>, HistoryError> {
    let staged = facts
        .working
        .placement
        .staged
        .ok_or(HistoryRefusal::InvalidFacts)?;
    let internal = if let Some(internal) = &facts.internal {
        let target = MoveTarget {
            branch: name,
            commit: id,
            content: staged.hash,
        };
        let move_plan = judge_move(internal, &target, false)?;
        let base = apply_move_file(&move_plan, fs, objects, registry, lock)?;
        Some((internal.placement.clone(), moved_row(&move_plan, base)))
    } else {
        None
    };
    Ok(internal)
}

pub(super) fn write_rows(
    plan: &CommitPlan,
    facts: &CommitFacts,
    id: ObjectId,
    internal: Option<(Placement, Placement)>,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), HistoryError> {
    let row = plan.placement();
    let staged = row.staged.ok_or(HistoryRefusal::InvalidFacts)?;
    let mut updated = row.clone();
    updated.commit_id = Some(id);
    updated.staged = None;
    let Observation::File { signature } = &facts.working.observation else {
        return Err(HistoryRefusal::InvalidFacts.into());
    };
    updated.base = Some(PlacementBase {
        hash: staged.hash,
        signature: if facts.working.placement_hash == Some(staged.hash) {
            signature.clone()
        } else {
            // Post-stage edits stay in the file; their metadata is not the staged base.
            Signature {
                size: staged.size,
                mtime: None,
                ctime: None,
                inode: None,
            }
        },
    });
    registry.write(lock, |tx| {
        if tx.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(RegistryError::PlacementChanged(row.path.clone()));
        }
        if let Some((before, after)) = &internal {
            if tx.placement_at(&before.path)?.as_ref() != Some(before) {
                return Err(RegistryError::PlacementChanged(before.path.clone()));
            }
            tx.update_placement(after)?;
        }
        tx.update_placement(&updated)
    })?;
    Ok(())
}
