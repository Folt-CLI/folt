use super::{MergeFacts, MergePlan, MergePlanData};
use crate::config::names::DEFAULT_BRANCH;
use crate::history::MergeBase;
use crate::merge::MergeResult;
use crate::object::hash_blob;
use crate::ops::history::HistoryRefusal;
use crate::ops::history::commit::branch_moved;
use crate::ops::placement::move_::{MoveTarget, judge_move};
use crate::ops::stage::refuse::is_staged;
use crate::ops::status::{Edit, judge_working};
use crate::placement::{Observation, PlacementKind};
use crate::ref_name::RefName;

pub(crate) fn judge_merge(facts: &MergeFacts) -> Result<MergePlan, HistoryRefusal> {
    let history = &facts.history;
    let working = &history.working;
    let row = &working.placement;
    if let Some(link) = &history.link {
        return Err(HistoryRefusal::Link(link.path.to_path_buf()));
    }
    if row.kind == PlacementKind::Link {
        return Err(HistoryRefusal::Link(row.path.to_path_buf()));
    }
    if !matches!(working.observation, Observation::File { .. }) {
        return Err(HistoryRefusal::Unusable(row.path.to_path_buf()));
    }
    if is_staged(row) || judge_working(working).edit != Edit::Clean {
        return Err(HistoryRefusal::MergeEdited(row.path.to_path_buf()));
    }
    if working.tip != row.commit_id {
        return Err(branch_moved(history));
    }
    let other = facts.other.ok_or(HistoryRefusal::MissingBranch)?;
    if working.tip == Some(other) || facts.base == MergeBase::One(other) {
        return Err(HistoryRefusal::MergeAlreadyMerged);
    }
    if facts.base == MergeBase::None {
        return Err(HistoryRefusal::MergeNoBase);
    }
    if facts.ancestors_binary_conflict {
        return Err(HistoryRefusal::MergeBaseBinaryConflict);
    }
    let bytes = match &facts.merged {
        MergeResult::Clean(bytes) => bytes,
        MergeResult::Conflicted {
            merged: Some(bytes),
            marker_lines,
        } if row.kind == PlacementKind::Copy => {
            return Ok(MergePlan::Conflicted {
                plan: MergePlanData {
                    facts: facts.clone(),
                    bytes: bytes.clone(),
                },
                marker_lines: marker_lines.clone(),
            });
        }
        MergeResult::Conflicted { .. } => return Err(HistoryRefusal::MergeConflict),
    };
    facts
        .validation
        .clone()
        .map_err(HistoryRefusal::MergeInvalid)?;
    if row.kind == PlacementKind::Internal || row.head == DEFAULT_BRANCH {
        if row.kind == PlacementKind::Internal && row.head != DEFAULT_BRANCH {
            return Err(HistoryRefusal::InvalidFacts);
        }
        if row.kind == PlacementKind::Copy {
            let internal = history
                .internal
                .as_ref()
                .ok_or(HistoryRefusal::MissingInternal)?;
            if internal.placement.id != row.id
                || internal.placement.kind != PlacementKind::Internal
                || internal.placement.head != DEFAULT_BRANCH
            {
                return Err(HistoryRefusal::InvalidFacts);
            }
            if is_staged(&internal.placement) || judge_working(internal).edit != Edit::Clean {
                return Err(HistoryRefusal::InternalEdited(
                    internal.placement.path.to_path_buf(),
                ));
            }
            judge_move(
                internal,
                &MoveTarget {
                    branch: RefName::parse(DEFAULT_BRANCH)
                        .map_err(|_| HistoryRefusal::InvalidFacts)?,
                    commit: working.tip.ok_or(HistoryRefusal::MissingBranch)?,
                    content: hash_blob(bytes),
                },
                false,
            )?;
        }
    }
    Ok(MergePlan::Clean(MergePlanData {
        facts: facts.clone(),
        bytes: bytes.clone(),
    }))
}
