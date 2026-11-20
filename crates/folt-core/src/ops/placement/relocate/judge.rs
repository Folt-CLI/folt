use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::placement::{Observation, PlacementKind};

use super::target::RelocateTarget;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelocatePlan {
    pub(super) facts: PlacementFacts,
    pub(super) target: RelocateTarget,
}

/// Plans a move or the cleanup of its pending rows before reporting a collision.
pub fn judge_relocate(
    facts: &PlacementFacts,
    target: &RelocateTarget,
) -> Result<RelocatePlan, PlacementRefusal> {
    match judge_relocate_checked(facts, target) {
        Err(error)
            if target.pending
                && facts.observation != Observation::Missing
                && matches!(
                    error.reason,
                    RefusalReason::DestinationExists | RefusalReason::DestinationRegistered
                ) =>
        {
            Ok(RelocatePlan {
                facts: facts.clone(),
                target: target.clone(),
            })
        }
        result => result,
    }
}

pub(super) fn judge_relocate_checked(
    facts: &PlacementFacts,
    target: &RelocateTarget,
) -> Result<RelocatePlan, PlacementRefusal> {
    let path = facts.placement.path.to_path_buf();
    let refuse = |reason| PlacementRefusal {
        path: path.clone(),
        reason,
    };
    if facts.placement.kind == PlacementKind::Internal {
        return Err(refuse(RefusalReason::Internal));
    }
    let destination = target.path.to_path_buf();
    if destination == path || destination.starts_with(&path) {
        return Err(refuse(RefusalReason::InsideSource));
    }
    if target.registered {
        return Err(refuse(RefusalReason::DestinationRegistered));
    }
    if facts.observation == Observation::Missing {
        if !target.pending || target.observation == Observation::Missing {
            return Err(refuse(RefusalReason::Gone));
        }
    } else {
        if target.observation != Observation::Missing {
            return Err(refuse(RefusalReason::DestinationExists));
        }
        match (&facts.observation, facts.placement.kind) {
            (Observation::IntactLink, PlacementKind::Link)
            | (Observation::File { .. }, PlacementKind::Copy) => {}
            (Observation::Directory, PlacementKind::Copy)
                if !target
                    .rows
                    .iter()
                    .any(|(old, _)| old.path == facts.placement.path) => {}
            (Observation::Directory | Observation::NotAFile, _) => {
                return Err(refuse(RefusalReason::NotAFile));
            }
            _ => return Err(refuse(RefusalReason::LinkReplaced)),
        }
    }
    Ok(RelocatePlan {
        facts: facts.clone(),
        target: target.clone(),
    })
}
