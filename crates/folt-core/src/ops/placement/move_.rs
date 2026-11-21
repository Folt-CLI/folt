use crate::config::names::DEFAULT_BRANCH;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{self, Drift, Observation, PlacementError, PlacementKind, PlacementOps};
use crate::ref_name::RefName;
use crate::registry::{self, Registry};
use crate::store::ObjectStore;

/// The caller resolves a commit and its content blob before judging a move.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveTarget {
    pub branch: RefName,
    pub commit: ObjectId,
    pub content: ObjectId,
}

/// An allowed move, constructible only through `judge_move`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MovePlan {
    placement: registry::Placement,
    observation: Observation,
    target: MoveTarget,
}

impl MovePlan {
    pub fn placement(&self) -> &registry::Placement {
        &self.placement
    }

    pub fn target(&self) -> &MoveTarget {
        &self.target
    }
}

pub(crate) fn judge_move(
    facts: &PlacementFacts,
    target: &MoveTarget,
    discard: bool,
) -> Result<MovePlan, PlacementRefusal> {
    let (_, placement) = facts.placement.to_placement();
    let refuse = |reason| PlacementRefusal {
        path: placement.path.clone(),
        reason,
    };
    if placement.kind == PlacementKind::Link {
        return Err(refuse(RefusalReason::Link));
    }
    if placement.kind == PlacementKind::Internal && target.branch.as_str() != DEFAULT_BRANCH {
        return Err(refuse(RefusalReason::InternalBranch));
    }
    match &facts.observation {
        Observation::Missing => return Err(refuse(RefusalReason::Gone)),
        Observation::Directory | Observation::NotAFile => {
            return Err(refuse(RefusalReason::NotAFile));
        }
        Observation::IntactLink | Observation::OtherLink { .. } => {
            return Err(refuse(RefusalReason::Unusable));
        }
        Observation::File { .. } => {}
    }
    match placement::classify(&placement, &facts.observation, facts.placement_hash) {
        Drift::InSync => {}
        Drift::PlacementEdited if discard || facts.placement_hash == Some(target.content) => {}
        Drift::PlacementEdited => return Err(refuse(RefusalReason::Edited)),
        _ => return Err(refuse(RefusalReason::Unusable)),
    }
    Ok(MovePlan {
        placement: facts.placement.clone(),
        observation: facts.observation.clone(),
        target: target.clone(),
    })
}

/// Rechecks an allowed move before writing its file, then updates just its row.
/// The home lock must cover gathering, judging and applying the move.
pub(crate) fn apply_move(
    plan: &MovePlan,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let base = apply_move_file(plan, fs, objects, registry, lock)?;
    let updated = moved_row(plan, base);
    registry.write(lock, |tx| tx.update_placement(&updated))?;
    Ok(())
}

/// Rechecks the row and observation, then refreshes only the file.
/// The caller persists `moved_row` after all related file work succeeds.
pub(crate) fn apply_move_file(
    plan: &MovePlan,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &Registry,
    _lock: &HomeLock,
) -> Result<Option<crate::placement::PlacementBase>, RulesError> {
    let (id, placement) = plan.placement.to_placement();
    if registry.placement_at(&plan.placement.path)?.as_ref() != Some(&plan.placement) {
        return Err(PlacementRefusal {
            path: placement.path,
            reason: RefusalReason::RegistryChanged,
        }
        .into());
    }
    if fs.read(id, &placement)? != plan.observation {
        return Err(PlacementRefusal {
            path: placement.path,
            reason: RefusalReason::PlacementChanged,
        }
        .into());
    }
    let written = fs
        .refresh(
            id,
            &placement,
            objects,
            plan.target.content,
            Some(&plan.observation),
        )
        .map_err(|error| match error {
            PlacementError::PlacementChanged => RulesError::Refused(PlacementRefusal {
                path: placement.path.clone(),
                reason: RefusalReason::PlacementChanged,
            }),
            error => RulesError::Placement(error),
        })?;
    Ok(written.base)
}

pub(crate) fn moved_row(
    plan: &MovePlan,
    base: Option<crate::placement::PlacementBase>,
) -> registry::Placement {
    let mut updated = plan.placement.clone();
    updated.head = plan.target.branch.as_str().into();
    updated.commit_id = Some(plan.target.commit);
    updated.base = base;
    updated.staged = None;
    updated
}

#[cfg(test)]
mod tests;
