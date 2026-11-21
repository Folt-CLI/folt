use super::super::{
    HistoryError, HistoryRefusal, HistoryStores, gather_commit, judge_commit, require_blob,
    require_observation, require_row,
};
use super::finish::{advance_branch, move_internal, write_rows};
use super::gather::CommitFacts;
use super::judge::branch_moved;
use super::plan::{CommitPlan, matches_requested_commit};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::{Author, Commit, CommitTime, ObjectKind, hash_blob};
use crate::ops::working::resolve_working;
use crate::placement::PlacementOps;
use crate::registry::Registry;
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
pub(crate) fn apply_commit(
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
