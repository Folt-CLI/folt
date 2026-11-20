use super::{
    HistoryError, HistoryRefusal, HistoryStores, gather_file, require_blob, require_observation,
};
use crate::history::Expect;
use crate::home::HomeLock;
use crate::id::{FolioId, ObjectId};
use crate::ops::facts::PlacementFacts;
use crate::ops::placement::move_::{MovePlan, MoveTarget, apply_move, judge_move};
use crate::placement::{PlacementKind, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::registry::Registry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResetFacts {
    pub id: FolioId,
    pub target: MoveTarget,
    pub old: ObjectId,
    pub placements: Vec<PlacementFacts>,
}

pub fn gather_reset(
    id: FolioId,
    branch: &RefName,
    commit: ObjectId,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<ResetFacts, HistoryError> {
    let HistoryStores {
        objects,
        refs,
        object_limit,
    } = *stores;
    let old = refs
        .read(RefKind::Branch, branch)?
        .ok_or(HistoryRefusal::MissingBranch)?;
    let target = objects.get_commit(commit, object_limit)?;
    require_blob(objects, target.content)?;
    let rows = registry.placements_of_head(id, branch.as_str())?;
    let mut placements = Vec::new();
    for row in rows {
        // Links follow the internal file and have no recorded commit or base to move.
        if row.kind == PlacementKind::Link {
            continue;
        }
        placements.push(gather_file(&row, fs, refs)?);
    }
    Ok(ResetFacts {
        id,
        target: MoveTarget {
            branch: branch.clone(),
            commit,
            content: target.content,
        },
        old,
        placements,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResetPlan {
    facts: ResetFacts,
    moves: Vec<MovePlan>,
}

pub fn judge_reset(facts: &ResetFacts, discard: bool) -> Result<ResetPlan, HistoryRefusal> {
    let mut moves = Vec::new();
    for placement in &facts.placements {
        if placement.placement.id != facts.id
            || placement.placement.head != facts.target.branch.as_str()
            || placement.tip != Some(facts.old)
        {
            return Err(HistoryRefusal::InvalidFacts);
        }
        moves.push(judge_move(placement, &facts.target, discard)?);
    }
    Ok(ResetPlan {
        facts: facts.clone(),
        moves,
    })
}

pub fn apply_reset(
    plan: &ResetPlan,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    let HistoryStores { objects, refs, .. } = *stores;
    let facts = &plan.facts;
    let actual = registry.placements_of_head(facts.id, facts.target.branch.as_str())?;
    let expected: Vec<_> = facts
        .placements
        .iter()
        .map(|facts| facts.placement.clone())
        .collect();
    let actual: Vec<_> = actual
        .into_iter()
        .filter(|row| row.kind != PlacementKind::Link)
        .collect();
    if actual != expected {
        return Err(HistoryRefusal::PlacementsChanged.into());
    }
    for placement in &facts.placements {
        require_observation(placement, fs)?;
    }
    require_blob(objects, facts.target.content)?;
    objects.sync_blobs()?;
    refs.update(
        lock,
        RefKind::Branch,
        &facts.target.branch,
        facts.target.commit,
        Expect::Is(facts.old),
    )?;
    refs.sync_refs()?;
    for movement in &plan.moves {
        apply_move(movement, fs, objects, registry, lock)?;
    }
    Ok(())
}
