use super::super::HistoryRefusal;
use super::gather::CommitFacts;
use super::plan::{CommitPlan, resume_commit};
use crate::config::names::DEFAULT_BRANCH;
use crate::ops::placement::move_::{MoveTarget, judge_move};
use crate::ops::status::{Edit, judge_working};
use crate::placement::{Observation, PlacementKind};
use crate::ref_name::RefName;

pub fn judge_commit(facts: &CommitFacts) -> Result<CommitPlan, HistoryRefusal> {
    let row = &facts.working.placement;
    if row.kind == PlacementKind::Link {
        return Err(HistoryRefusal::Link(row.path.to_path_buf()));
    }
    let staged = row.staged.ok_or(HistoryRefusal::NothingStaged)?;
    if !matches!(facts.working.observation, Observation::File { .. }) {
        return Err(HistoryRefusal::Unusable(row.path.to_path_buf()));
    }
    if row.kind == PlacementKind::Internal && row.head != DEFAULT_BRANCH {
        return Err(HistoryRefusal::InvalidFacts);
    }
    let resume = resume_commit(facts);
    if facts.working.tip != row.commit_id && resume.is_none() {
        return Err(branch_moved(facts));
    }
    if row.kind == PlacementKind::Copy && row.head == DEFAULT_BRANCH {
        let internal = facts
            .internal
            .as_ref()
            .ok_or(HistoryRefusal::MissingInternal)?;
        if internal.placement.id != row.id
            || internal.placement.kind != PlacementKind::Internal
            || internal.placement.head != DEFAULT_BRANCH
        {
            return Err(HistoryRefusal::InvalidFacts);
        }
        // A completed file replacement may precede the atomic registry update.
        let already_written = resume.is_some() && internal.placement_hash == Some(staged.hash);
        if internal.placement.staged.is_some()
            || (judge_working(internal).edit != Edit::Clean && !already_written)
        {
            return Err(HistoryRefusal::InternalEdited(
                internal.placement.path.to_path_buf(),
            ));
        }
        // Only the branch and content affect move permission; apply supplies
        // the new commit id once its caller-provided metadata has been encoded.
        let target = MoveTarget {
            branch: RefName::parse(DEFAULT_BRANCH).map_err(|_| HistoryRefusal::InvalidFacts)?,
            commit: resume.or(row.commit_id).unwrap_or(staged.hash),
            content: staged.hash,
        };
        judge_move(internal, &target, false)?;
    }
    Ok(CommitPlan {
        facts: facts.clone(),
        resume,
    })
}

pub(super) fn branch_moved(facts: &CommitFacts) -> HistoryRefusal {
    HistoryRefusal::BranchMoved {
        tip: facts.working.tip,
        ahead: facts
            .placements
            .iter()
            .filter(|row| {
                facts.working.tip.is_some()
                    && row.commit_id == facts.working.tip
                    && row.path != facts.working.placement.path
                    && row.kind != PlacementKind::Link
            })
            .map(|row| row.path.to_path_buf())
            .collect(),
    }
}
