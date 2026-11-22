use std::collections::{HashMap, HashSet};

use super::{HistoryRefusal, ResetFacts, ResetPlan, judge_reset};
use crate::id::ObjectId;
use crate::merge::{MergeResult, merge3};
use crate::object::{Commit, ObjectKind, hash_blob, hash_object};
use crate::ref_name::{RefKind, RefName};

mod io;
pub(crate) use io::{apply_rebase, gather_rebase};

/// A tag or retention pin that stays on a commit in the original range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeftBehind {
    pub kind: RefKind,
    pub name: RefName,
    pub commit: ObjectId,
}

/// Immutable history and placement inputs for replay onto `reset.target`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebaseFacts {
    pub reset: ResetFacts,
    pub range: HashSet<ObjectId>,
    /// Range commits, their original parents and the destination commit.
    pub commits: HashMap<ObjectId, Commit>,
    /// Content bytes indexed by blob id.
    pub contents: HashMap<ObjectId, Vec<u8>>,
    pub left_behind: Vec<LeftBehind>,
}

/// A deterministic replay followed by the reset that publishes its final tip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebasePlan {
    facts: RebaseFacts,
    reset: ResetPlan,
    replayed: Vec<(ObjectId, Commit, Vec<u8>)>,
    tip: ObjectId,
    discard: bool,
}

impl RebasePlan {
    pub fn tip(&self) -> ObjectId {
        self.tip
    }

    pub fn left_behind(&self) -> &[LeftBehind] {
        &self.facts.left_behind
    }
}

pub(crate) fn judge_rebase(
    facts: &RebaseFacts,
    discard: bool,
) -> Result<RebasePlan, HistoryRefusal> {
    // Validate placement facts even when there is nothing to replay.
    judge_reset(&facts.reset, discard)?;
    for (id, commit) in &facts.commits {
        if hash_object(
            ObjectKind::Commit,
            &commit
                .encode_body()
                .map_err(|_| HistoryRefusal::InvalidFacts)?,
        ) != *id
        {
            return Err(HistoryRefusal::InvalidFacts);
        }
    }
    for (id, bytes) in &facts.contents {
        if hash_blob(bytes) != *id {
            return Err(HistoryRefusal::InvalidFacts);
        }
    }
    let mut reset = facts.reset.clone();
    let onto = facts
        .commits
        .get(&reset.target.commit)
        .ok_or(HistoryRefusal::InvalidFacts)?;
    if onto.content != reset.target.content {
        return Err(HistoryRefusal::InvalidFacts);
    }
    let mut content = bytes(facts, onto.content)?.to_vec();
    let mut remaining = facts.range.clone();
    let mut replayed = Vec::new();
    while !remaining.is_empty() {
        // Include merges in the ordering, so descendants wait for every original parent.
        let id = remaining
            .iter()
            .filter(|id| {
                facts.commits.get(id).is_some_and(|commit| {
                    commit
                        .parents
                        .iter()
                        .all(|parent| !remaining.contains(parent))
                })
            })
            .min_by_key(|id| *id.as_bytes())
            .copied()
            .ok_or(HistoryRefusal::InvalidFacts)?;
        remaining.remove(&id);
        let original = facts.commits.get(&id).ok_or(HistoryRefusal::InvalidFacts)?;
        let parent = match original.parents.as_slice() {
            [] => return Err(HistoryRefusal::RebaseRoot(id)),
            [parent] => parent,
            _ => continue,
        };
        let base = facts
            .commits
            .get(parent)
            .ok_or(HistoryRefusal::InvalidFacts)?;
        content = match merge3(
            bytes(facts, base.content)?,
            &content,
            bytes(facts, original.content)?,
        ) {
            MergeResult::Clean(bytes) => bytes,
            MergeResult::Conflicted { .. } => return Err(HistoryRefusal::RebaseConflict(id)),
        };
        let mut commit = original.clone();
        commit.parents = vec![reset.target.commit];
        commit.content = hash_blob(&content);
        let body = commit
            .encode_body()
            .map_err(|_| HistoryRefusal::InvalidFacts)?;
        reset.target.commit = hash_object(ObjectKind::Commit, &body);
        reset.target.content = commit.content;
        replayed.push((reset.target.commit, commit, content.clone()));
    }
    Ok(RebasePlan {
        facts: facts.clone(),
        reset: judge_reset(&reset, discard)?,
        replayed,
        tip: reset.target.commit,
        discard,
    })
}

fn bytes(facts: &RebaseFacts, id: ObjectId) -> Result<&[u8], HistoryRefusal> {
    facts
        .contents
        .get(&id)
        .map(Vec::as_slice)
        .ok_or(HistoryRefusal::InvalidFacts)
}
