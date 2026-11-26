use super::gather::CommitFacts;
use crate::id::ObjectId;
use crate::object::Commit;
use crate::registry::Placement;

/// An allowed commit; content and parent are fixed by the recorded stage and base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitPlan {
    pub(super) facts: CommitFacts,
    pub(super) resume: Option<ObjectId>,
}

impl CommitPlan {
    pub fn placement(&self) -> &Placement {
        &self.facts.working.placement
    }
    pub fn resumed_commit(&self) -> Option<ObjectId> {
        self.resume
    }
}

/// Recognises a published commit whose remaining file/registry work can be finished.
pub(crate) fn resume_commit(facts: &CommitFacts) -> Option<ObjectId> {
    resume_commit_with_parents(facts, facts.working.placement.commit_id.as_slice())
}

pub(super) fn resume_commit_with_parents(
    facts: &CommitFacts,
    parents: &[ObjectId],
) -> Option<ObjectId> {
    let row = &facts.working.placement;
    let staged = row.staged?;
    let tip = facts.working.tip?;
    let commit = facts.tip_commit.as_ref()?;
    (Some(tip) != row.commit_id && commit.content == staged.hash && commit.parents == parents)
        .then_some(tip)
}

pub(crate) fn matches_requested_commit(actual: &Commit, requested: &Commit) -> bool {
    actual.author == requested.author
        && actual.message == requested.message
        && actual.body.as_deref() == requested.body.as_deref().filter(|body| !body.is_empty())
        && actual.group == requested.group
        && actual.content == requested.content
        && actual.parents == requested.parents
}
