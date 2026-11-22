use super::super::{HistoryError, HistoryRefusal, HistoryStores, judge_commit};
use super::gather::CommitFacts;
use super::judge::branch_moved;
use super::plan::matches_requested_commit;
use crate::fsx::Signature;
use crate::history::{Expect, RefError};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::Commit;
use crate::ops::placement::move_::{MoveTarget, apply_move_file, judge_move, moved_row};
use crate::placement::{Observation, PlacementBase, PlacementOps, StagedBlob};
use crate::placement_path::RegistryError;
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

pub(crate) enum Recovery {
    Resume,
    Refuse,
}

pub(crate) fn advance_branch(
    mut id: ObjectId,
    commit: &Commit,
    facts: &mut CommitFacts,
    recovery: Recovery,
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
            if matches!(recovery, Recovery::Refuse) {
                return Err(branch_moved(facts).into());
            }
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

pub(crate) fn move_internal(
    facts: &CommitFacts,
    target: MoveTarget,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &Registry,
    lock: &HomeLock,
) -> Result<Option<(Placement, Placement)>, HistoryError> {
    let internal = if let Some(internal) = &facts.internal {
        let move_plan = judge_move(internal, &target, false)?;
        let base = apply_move_file(&move_plan, fs, objects, registry, lock)?;
        Some((internal.placement.clone(), moved_row(&move_plan, base)))
    } else {
        None
    };
    Ok(internal)
}

pub(crate) fn write_rows(
    facts: &CommitFacts,
    id: ObjectId,
    content: StagedBlob,
    internal: Option<(Placement, Placement)>,
    references: Option<&[crate::front_matter::Reference<'_>]>,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), HistoryError> {
    let row = &facts.working.placement;
    let mut updated = row.clone();
    updated.commit_id = Some(id);
    updated.staged = None;
    let Observation::File { signature } = &facts.working.observation else {
        return Err(HistoryRefusal::InvalidFacts.into());
    };
    updated.base = Some(PlacementBase {
        hash: content.hash,
        signature: if facts.working.placement_hash == Some(content.hash) {
            signature.clone()
        } else {
            // Post-stage edits stay in the file; their metadata is not the staged base.
            Signature {
                size: content.size,
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
        tx.update_placement(&updated)?;
        if let Some(references) = references {
            tx.rebuild_references(row.id, &row.head, references)?;
        }
        Ok(())
    })?;
    Ok(())
}
