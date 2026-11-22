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
mod virtual_base;

pub(crate) use apply::merge_branch;
#[cfg(test)]
pub(super) use judge::judge_merge;

/// Read-only inputs for the merge decision, including merged-content validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MergeFacts {
    pub history: CommitFacts,
    pub other: Option<ObjectId>,
    pub base: MergeBase,
    pub ancestors_binary_conflict: bool,
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
    let base_bytes = match &base {
        MergeBase::None => Some(Vec::new()),
        MergeBase::One(id) => Some(content(Some(*id))?),
        MergeBase::Several(ids) => virtual_base::fold(stores.objects, ids, stores.object_limit)?,
    };
    let ancestors_binary_conflict = base_bytes.is_none();
    let ours = content(working.tip)?;
    let theirs = content(other)?;
    let merged = match base_bytes {
        Some(bytes) => merge3(&bytes, &ours, &theirs),
        None => MergeResult::Conflicted {
            merged: None,
            marker_lines: Vec::new(),
        },
    };
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
        ancestors_binary_conflict,
        merged,
        validation,
    })
}
