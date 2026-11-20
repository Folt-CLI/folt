use super::super::{
    HistoryError, HistoryRefusal, HistoryStores, gather_commit, judge_commit, require_blob,
    require_observation, require_row,
};
use super::gather::CommitFacts;
use super::judge::branch_moved;
use super::plan::{CommitPlan, matches_requested_commit};
use crate::fsx::Signature;
use crate::history::{Expect, RefError};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::{Author, Commit, CommitTime, ObjectKind, hash_blob};
use crate::ops::placement::move_::{MoveTarget, apply_move_file, judge_move, moved_row};
use crate::ops::working::resolve_working;
use crate::placement::{Observation, PlacementBase, PlacementOps};
use crate::placement_path::RegistryError;
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

/// Metadata comes from the caller; core history never reads identity or a clock.
pub struct CommitMetadata {
    pub author: Author,
    pub time: CommitTime,
    pub message: String,
    pub body: Option<String>,
    pub group: Option<String>,
}

/// Publishes objects, then the ref, then files, then both registry rows together.
pub fn apply_commit(
    plan: &CommitPlan,
    metadata: CommitMetadata,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    let (mut facts, checked) = check_plan(plan, fs, stores, registry)?;
    let commit = build_commit(plan, metadata, &facts, &checked)?;
    let id = publish_objects(&commit, &checked, stores.objects, lock, plan)?;
    let (id, name) = advance_branch(id, &commit, &mut facts, stores, lock)?;
    let internal = move_internal(&facts, id, name, fs, stores.objects, registry, lock)?;
    write_rows(plan, &facts, id, internal, registry, lock)?;
    Ok(id)
}

fn check_plan(
    plan: &CommitPlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<(CommitFacts, CommitPlan), HistoryError> {
    let row = plan.placement();
    require_row(registry, row)?;
    if let Some(link) = &plan.facts.link {
        require_row(registry, link)?;
        if resolve_working(link, registry, fs)? != *row {
            return Err(HistoryRefusal::RegistryChanged(link.path.to_path_buf()).into());
        }
    }
    require_observation(&plan.facts.working, fs)?;
    let facts = gather_commit(row, fs, stores, registry)?;
    let checked = judge_commit(&facts)?;
    if let Some(internal) = &plan.facts.internal {
        require_row(registry, &internal.placement)?;
        if checked.resume.is_none() {
            require_observation(internal, fs)?;
        }
    }
    Ok((facts, checked))
}

fn build_commit(
    plan: &CommitPlan,
    metadata: CommitMetadata,
    facts: &CommitFacts,
    checked: &CommitPlan,
) -> Result<Commit, HistoryError> {
    let row = plan.placement();
    let staged = row.staged.ok_or(HistoryRefusal::InvalidFacts)?;
    let commit = Commit {
        content: staged.hash,
        parents: row.commit_id.into_iter().collect(),
        author: metadata.author,
        time: metadata.time,
        message: metadata.message,
        body: metadata.body,
        group: metadata.group,
    };
    // Validate all metadata before any writes, even when finishing a prior commit.
    commit.encode_body()?;
    if checked.resume.is_some()
        && !facts
            .tip_commit
            .as_ref()
            .is_some_and(|tip| matches_requested_commit(tip, &commit))
    {
        return Err(branch_moved(facts).into());
    }
    Ok(commit)
}

fn publish_objects(
    commit: &Commit,
    checked: &CommitPlan,
    objects: &ObjectStore,
    lock: &HomeLock,
    plan: &CommitPlan,
) -> Result<ObjectId, HistoryError> {
    let staged = plan
        .placement()
        .staged
        .ok_or(HistoryRefusal::InvalidFacts)?;
    if staged.hash == hash_blob(b"") {
        if staged.size != 0 {
            return Err(HistoryRefusal::InvalidFacts.into());
        }
        objects.put(lock, ObjectKind::Blob, b"")?;
    } else {
        require_blob(objects, staged.hash)?;
    }
    let id = if let Some(id) = checked.resume {
        id
    } else {
        objects.put_commit(lock, commit)?
    };
    objects.sync_blobs()?;
    Ok(id)
}

fn advance_branch(
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

fn move_internal(
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

fn write_rows(
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
