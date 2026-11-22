use super::{CommitFacts, HistoryError, HistoryStores, gather_file};
use crate::config::names::DEFAULT_BRANCH;
use crate::history::{MergeBase, merge_base};
use crate::id::ObjectId;
use crate::merge::{MergeResult, merge3};
use crate::object::ObjectKind;
use crate::ops::stage::validate_merged;
use crate::ops::working::resolve_working;
use crate::placement::{PlacementKind, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::registry::{Placement, Registry};

mod apply;
mod judge;

pub(crate) use apply::merge_branch;
#[cfg(test)]
pub(super) use judge::judge_merge;

/// Read-only inputs for the merge decision, including merged-content validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MergeFacts {
    pub history: CommitFacts,
    pub other: Option<ObjectId>,
    pub base: MergeBase,
    pub merged: MergeResult,
    pub validation: Result<(), String>,
}

/// A clean combination with both parent tips fixed by the decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergePlan {
    facts: MergeFacts,
    bytes: Vec<u8>,
}

/// The other branch and caller-supplied metadata for its merge commit.
pub struct MergeRequest<'a> {
    pub other: &'a RefName,
    pub metadata: super::CommitMetadata,
}

pub(super) fn gather_merge(
    selected: &Placement,
    other: &RefName,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<MergeFacts, HistoryError> {
    let row = if selected.kind == PlacementKind::Link {
        selected.clone()
    } else {
        resolve_working(selected, registry, fs)?
    };
    let working = gather_file(&row, fs, stores.refs)?;
    let placements = registry.placements_of_head(row.id, &row.head)?;
    let internal = if row.kind == PlacementKind::Copy && row.head == DEFAULT_BRANCH {
        registry
            .internal_placement(row.id)?
            .map(|row| gather_file(&row, fs, stores.refs))
            .transpose()?
    } else {
        None
    };
    let other = stores.refs.read(RefKind::Branch, other)?;
    let base = match (working.tip, other) {
        (Some(ours), Some(theirs)) => {
            merge_base(stores.objects, ours, theirs, stores.object_limit)?
        }
        _ => MergeBase::None,
    };
    let content = |id: Option<ObjectId>| -> Result<Vec<u8>, HistoryError> {
        let Some(id) = id else {
            return Ok(Vec::new());
        };
        let commit = stores.objects.get_commit(id, stores.object_limit)?;
        Ok(stores
            .objects
            .get(commit.content, ObjectKind::Blob, u64::MAX)?)
    };
    let base_bytes = content(if let MergeBase::One(id) = base {
        Some(id)
    } else {
        None
    })?;
    let ours = content(working.tip)?;
    let theirs = content(other)?;
    let merged = merge3(&base_bytes, &ours, &theirs);
    let validation = match &merged {
        MergeResult::Clean(bytes) => validate_merged(&row, bytes, registry),
        MergeResult::Conflicted { .. } => Ok(()),
    };
    Ok(MergeFacts {
        history: CommitFacts {
            working,
            link: (selected.kind == PlacementKind::Link).then(|| selected.clone()),
            internal,
            placements,
            tip_commit: None,
        },
        other,
        base,
        merged,
        validation,
    })
}
