use super::{RebaseFacts, bytes};
use crate::id::ObjectId;
use crate::merge::{MergeResult, merge3};
use crate::object::{Commit, ObjectKind, hash_blob, hash_object};
use crate::ops::history::HistoryRefusal;

type Replayed = (ObjectId, Commit, Vec<u8>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StopPoint {
    pub current: ObjectId,
    pub remaining: Vec<ObjectId>,
    pub bytes: Vec<u8>,
    pub marker_lines: Vec<Vec<u8>>,
}

pub(super) struct Replay {
    pub replayed: Vec<Replayed>,
    pub tip: ObjectId,
    pub content: Vec<u8>,
    pub stop: Option<StopPoint>,
}

pub(super) fn ordered(facts: &RebaseFacts) -> Result<Vec<ObjectId>, HistoryRefusal> {
    let mut remaining = facts.range.clone();
    let mut ids = Vec::new();
    while !remaining.is_empty() {
        // Merges participate in ordering but are dropped during replay.
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
        ids.push(id);
    }
    Ok(ids)
}

pub(super) fn resolved(
    original: &Commit,
    tip: ObjectId,
    content: Vec<u8>,
) -> Result<Replayed, HistoryRefusal> {
    let mut commit = original.clone();
    commit.parents = vec![tip];
    commit.content = hash_blob(&content);
    let body = commit
        .encode_body()
        .map_err(|_| HistoryRefusal::InvalidFacts)?;
    Ok((hash_object(ObjectKind::Commit, &body), commit, content))
}

pub(super) fn replay(
    facts: &RebaseFacts,
    mut tip: ObjectId,
    mut content: Vec<u8>,
    ids: &[ObjectId],
) -> Result<Replay, HistoryRefusal> {
    let mut replayed = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let original = facts.commits.get(id).ok_or(HistoryRefusal::InvalidFacts)?;
        let parent = match original.parents.as_slice() {
            [] => return Err(HistoryRefusal::RebaseRoot(*id)),
            [parent] => parent,
            _ => continue,
        };
        let base = facts
            .commits
            .get(parent)
            .ok_or(HistoryRefusal::InvalidFacts)?;
        match merge3(
            bytes(facts, base.content)?,
            &content,
            bytes(facts, original.content)?,
        ) {
            MergeResult::Clean(bytes) => content = bytes,
            MergeResult::Conflicted {
                merged,
                marker_lines,
            } => {
                return Ok(Replay {
                    replayed,
                    tip,
                    content,
                    stop: Some(StopPoint {
                        current: *id,
                        remaining: ids[index + 1..].to_vec(),
                        bytes: merged.unwrap_or_default(),
                        marker_lines,
                    }),
                });
            }
        }
        let entry = resolved(original, tip, content.clone())?;
        tip = entry.0;
        replayed.push(entry);
    }
    Ok(Replay {
        replayed,
        tip,
        content,
        stop: None,
    })
}
