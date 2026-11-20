use super::{HistoryError, HistoryRefusal, HistoryStores};
use crate::history::{Expect, RefError, RefStore};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::{ObjectKind, Tag};
use crate::ref_name::{RefKind, RefName};

/// `None` makes a lightweight tag; an annotation must name this ref and target.
pub fn create_tag(
    name: &RefName,
    commit: ObjectId,
    annotation: Option<&Tag>,
    stores: &HistoryStores<'_>,
    lock: &mut HomeLock,
) -> Result<ObjectId, HistoryError> {
    let HistoryStores { objects, refs, .. } = *stores;
    if let Some(tag) = annotation {
        if tag.name != name.as_str() || tag.target != commit {
            return Err(HistoryRefusal::TagMismatch.into());
        }
        tag.encode_body()?;
    }
    if refs.read(RefKind::Tag, name)?.is_some() {
        return Err(HistoryRefusal::TagExists.into());
    }
    let actual = objects.kind_of(commit)?;
    if actual != ObjectKind::Commit {
        return Err(RefError::WrongKind { id: commit, actual }.into());
    }
    let target = match annotation {
        Some(tag) => objects.put_tag(lock, tag)?,
        None => commit,
    };
    objects.sync_blobs()?;
    refs.update(lock, RefKind::Tag, name, target, Expect::Absent)?;
    refs.sync_refs()?;
    Ok(target)
}

pub fn delete_tag(
    name: &RefName,
    old: ObjectId,
    refs: &RefStore,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    refs.delete(lock, RefKind::Tag, name, Expect::Is(old))?;
    refs.sync_refs()?;
    Ok(())
}
