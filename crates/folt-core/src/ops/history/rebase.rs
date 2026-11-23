use std::collections::{HashMap, HashSet};

use super::{HistoryRefusal, ResetFacts, ResetPlan, judge_reset};
use crate::id::ObjectId;
use crate::object::{Commit, ObjectKind, hash_blob, hash_object};
use crate::placement::ConflictState;
use crate::placement_path::PlacementPath;
use crate::ref_name::{RefKind, RefName};
use crate::registry::StoppedRebase;

mod finish;
mod io;
mod replay;
mod resume;
mod stopped;
pub(crate) use stopped::{abort_rebase, continue_rebase};

#[cfg(test)]
mod tests;

/// Result of applying or continuing a rebase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RebaseOutcome {
    Finished(ObjectId),
    Stopped(ConflictState),
}
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
    pub copy: Option<PlacementPath>,
    pub stopped: Option<StoppedRebase>,
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
    stop: Option<replay::StopPoint>,
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
    if facts.stopped.is_some() {
        return Err(HistoryRefusal::RebaseStopped);
    }
    let ids = replay::ordered(facts)?;
    let result = replay::replay(
        facts,
        reset.target.commit,
        bytes(facts, onto.content)?.to_vec(),
        &ids,
    )?;
    if let Some(stop) = &result.stop {
        if facts.copy.is_none() || stop.marker_lines.is_empty() {
            return Err(HistoryRefusal::RebaseConflict(stop.current));
        }
        let copy = facts
            .reset
            .placements
            .iter()
            .find(|p| Some(&p.placement.path) == facts.copy.as_ref())
            .ok_or(HistoryRefusal::InvalidFacts)?;
        if copy.placement.kind != crate::placement::PlacementKind::Copy
            || copy.placement.conflict.is_some()
            || copy.placement.staged.is_some()
        {
            return Err(HistoryRefusal::InvalidFacts);
        }
        let _ = crate::ops::placement::move_::judge_move(copy, &facts.reset.target, discard)?;
    }
    reset.target.commit = result.tip;
    reset.target.content = hash_blob(&result.content);
    Ok(RebasePlan {
        facts: facts.clone(),
        reset: judge_reset(&reset, discard)?,
        replayed: result.replayed,
        tip: reset.target.commit,
        discard,
        stop: result.stop,
    })
}

fn bytes(facts: &RebaseFacts, id: ObjectId) -> Result<&[u8], HistoryRefusal> {
    facts
        .contents
        .get(&id)
        .map(Vec::as_slice)
        .ok_or(HistoryRefusal::InvalidFacts)
}
