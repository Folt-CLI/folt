use crate::history::{MergeBase, merge_base};
use crate::id::ObjectId;
use crate::merge::{MergeResult, merge3};
use crate::object::ObjectKind;
use crate::store::{ObjectStore, StoreError};

/// Fold ancestors in their supplied object id byte order, without storing objects.
/// `None` reports a binary conflict, including one in a recursively folded base.
pub(super) fn fold(
    objects: &ObjectStore,
    ids: &[ObjectId],
    limit: u64,
) -> Result<Option<Vec<u8>>, StoreError> {
    let Some((&first, rest)) = ids.split_first() else {
        return Ok(Some(Vec::new()));
    };
    let mut bytes = content(objects, first, limit)?;
    let mut previous = first;
    for &next in rest {
        let base = match merge_base(objects, previous, next, limit)? {
            MergeBase::None => Vec::new(),
            MergeBase::One(id) => content(objects, id, limit)?,
            MergeBase::Several(ids) => {
                let Some(bytes) = fold(objects, &ids, limit)? else {
                    return Ok(None);
                };
                bytes
            }
        };
        bytes = match merge3(&base, &bytes, &content(objects, next, limit)?) {
            MergeResult::Clean(bytes)
            | MergeResult::Conflicted {
                merged: Some(bytes),
                ..
            } => bytes,
            MergeResult::Conflicted { merged: None, .. } => return Ok(None),
        };
        // The accumulated bytes stay in memory; the next pair uses these
        // adjacent ancestors for its ancestry search.
        previous = next;
    }
    Ok(Some(bytes))
}

fn content(objects: &ObjectStore, id: ObjectId, limit: u64) -> Result<Vec<u8>, StoreError> {
    let commit = objects.get_commit(id, limit)?;
    objects.get(commit.content, ObjectKind::Blob, u64::MAX)
}

#[cfg(test)]
mod tests;
