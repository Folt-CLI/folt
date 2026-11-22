use super::{LeftBehind, RebaseFacts, RebasePlan};
use crate::history::commit_range;
use crate::home::HomeLock;
use crate::id::{FolioId, ObjectId};
use crate::object::ObjectKind;
use crate::ops::history::{HistoryError, HistoryRefusal, HistoryStores, apply_reset, gather_reset};
use crate::placement::PlacementOps;
use crate::ref_name::{RefKind, RefName};
use crate::registry::Registry;

pub(crate) fn gather_rebase(
    id: FolioId,
    branch: &RefName,
    onto: ObjectId,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<RebaseFacts, HistoryError> {
    let reset = gather_reset(id, branch, onto, fs, stores, registry)?;
    let range = commit_range(stores.objects, reset.old, onto, stores.object_limit)?;
    let mut commits = std::collections::HashMap::new();
    let mut contents = std::collections::HashMap::new();
    let mut ids: Vec<_> = range.iter().copied().collect();
    ids.push(onto);
    for id in ids.clone() {
        ids.extend(stores.objects.get_commit(id, stores.object_limit)?.parents);
    }
    for id in ids {
        if commits.contains_key(&id) {
            continue;
        }
        let commit = stores.objects.get_commit(id, stores.object_limit)?;
        contents.insert(
            commit.content,
            stores
                .objects
                .get(commit.content, ObjectKind::Blob, stores.object_limit)?,
        );
        commits.insert(id, commit);
    }
    let mut left_behind = Vec::new();
    for kind in [RefKind::Tag, RefKind::Pin] {
        for (name, target) in stores.refs.list(kind)? {
            let commit = if stores.objects.kind_of(target)? == ObjectKind::Tag {
                stores.objects.get_tag(target, stores.object_limit)?.target
            } else {
                target
            };
            if range.contains(&commit) {
                left_behind.push(LeftBehind { kind, name, commit });
            }
        }
    }
    Ok(RebaseFacts {
        reset,
        range,
        commits,
        contents,
        left_behind,
    })
}

pub(crate) fn apply_rebase(
    plan: &RebasePlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    let facts = &plan.facts;
    let current = gather_rebase(
        facts.reset.id,
        &facts.reset.target.branch,
        facts.reset.target.commit,
        fs,
        stores,
        registry,
    )?;
    if current != *facts || super::judge_rebase(&current, plan.discard)? != *plan {
        return Err(HistoryRefusal::InvalidFacts.into());
    }
    // All mutable inputs were rechecked before publishing any replay objects.
    for (_, commit, content) in &plan.replayed {
        stores.objects.put(lock, ObjectKind::Blob, content)?;
        stores.objects.put_commit(lock, commit)?;
    }
    stores.objects.sync_blobs()?;
    apply_reset(&plan.reset, fs, stores, registry, lock)?;
    Ok(plan.tip)
}
