//! Read-only walks of every commit parent, including histories with several merge bases.

use std::collections::HashSet;

use crate::id::ObjectId;
use crate::store::{ObjectStore, StoreError};

/// The best common ancestors of two commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeBase {
    /// The histories have no common ancestor.
    None,
    /// Exactly one best common ancestor.
    One(ObjectId),
    /// At least two best common ancestors, sorted by object id bytes.
    Several(Vec<ObjectId>),
}

/// Whether `ancestor` is reachable from `descendant` by zero or more parent steps.
/// A commit is its own ancestor. Every reachable commit is read and validated.
/// `limit` bounds each object read, rather than the number of commits visited.
///
/// # Errors
/// Returns the store error if any reachable commit is missing, unreadable, invalid or over `limit`.
pub fn is_ancestor(
    objects: &ObjectStore,
    ancestor: ObjectId,
    descendant: ObjectId,
    limit: u64,
) -> Result<bool, StoreError> {
    Ok(ancestors(objects, descendant, limit)?.contains(&ancestor))
}

/// Finds common ancestors that are not proper ancestors of another common ancestor.
/// Several results are reported without merging them or creating a virtual base.
/// `limit` bounds each object read, rather than the number of commits visited.
///
/// # Errors
/// Returns the store error if any reachable commit is missing, unreadable, invalid or over `limit`.
pub fn merge_base(
    objects: &ObjectStore,
    a: ObjectId,
    b: ObjectId,
    limit: u64,
) -> Result<MergeBase, StoreError> {
    let a_ancestors = ancestors(objects, a, limit)?;
    let b_ancestors = ancestors(objects, b, limit)?;
    let mut best: HashSet<_> = a_ancestors.intersection(&b_ancestors).copied().collect();
    let mut stack: Vec<_> = best.iter().copied().collect();
    let mut visited = HashSet::new();
    // Every parent reached from a common ancestor is strictly older and cannot be best.
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        for parent in objects.get_commit(id, limit)?.parents {
            best.remove(&parent);
            stack.push(parent);
        }
    }
    let mut best: Vec<_> = best.into_iter().collect();
    best.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(match best.as_slice() {
        [] => MergeBase::None,
        [id] => MergeBase::One(*id),
        _ => MergeBase::Several(best),
    })
}

/// Commits reachable from `tip` and not reachable from `exclude`.
/// Every reachable commit on both sides is validated with the caller's per-object limit.
///
/// # Errors
/// Returns the store error for a missing, invalid, unreadable or oversized commit.
pub fn commit_range(
    objects: &ObjectStore,
    tip: ObjectId,
    exclude: ObjectId,
    limit: u64,
) -> Result<HashSet<ObjectId>, StoreError> {
    let reachable = ancestors(objects, tip, limit)?;
    let excluded = ancestors(objects, exclude, limit)?;
    Ok(reachable.difference(&excluded).copied().collect())
}

fn ancestors(
    objects: &ObjectStore,
    start: ObjectId,
    limit: u64,
) -> Result<HashSet<ObjectId>, StoreError> {
    let mut stack = vec![start];
    let mut visited = HashSet::new();
    while let Some(id) = stack.pop() {
        if visited.insert(id) {
            stack.extend(objects.get_commit(id, limit)?.parents);
        }
    }
    Ok(visited)
}
