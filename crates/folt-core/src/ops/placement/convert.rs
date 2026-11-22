mod apply;

use crate::config::names::DEFAULT_BRANCH;
use crate::id::ObjectId;
use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::stage::refuse::{is_conflicted, is_staged};
use crate::placement::{self, Drift, Observation, PlacementKind};
use crate::registry;

use super::move_::MoveTarget;

pub(crate) use apply::apply_convert;

/// An allowed kind change, constructible only through `judge_convert`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvertPlan {
    placement: registry::Placement,
    observation: Observation,
    placement_hash: Option<ObjectId>,
    to_kind: PlacementKind,
    target: Option<MoveTarget>,
    internal: Option<PlacementFacts>,
    resume: bool,
}

impl ConvertPlan {
    pub fn placement(&self) -> &registry::Placement {
        &self.placement
    }

    pub fn to_kind(&self) -> PlacementKind {
        self.to_kind
    }
}

/// The optional target is the resolved main tip; Copy-to-Link needs no tip.
/// Link-to-copy also requires the Internal row's facts. The home lock must cover
/// collecting these facts through apply.
pub(crate) fn judge_convert(
    facts: &PlacementFacts,
    to_kind: PlacementKind,
    target: Option<&MoveTarget>,
    internal: Option<&PlacementFacts>,
) -> Result<ConvertPlan, PlacementRefusal> {
    let (_, placement) = facts.placement.to_placement();
    let refuse = |reason| PlacementRefusal {
        path: placement.path.clone(),
        reason,
    };
    if placement.kind == PlacementKind::Internal || to_kind == PlacementKind::Internal {
        return Err(refuse(RefusalReason::Internal));
    }
    if placement.kind == to_kind {
        return Err(refuse(RefusalReason::SameKind));
    }
    if target.is_some_and(|target| target.branch.as_str() != DEFAULT_BRANCH)
        || (placement.kind == PlacementKind::Copy && placement.head != DEFAULT_BRANCH)
    {
        return Err(refuse(RefusalReason::CopyBranch));
    }
    if is_conflicted(&facts.placement) {
        return Err(refuse(RefusalReason::Conflicted));
    }
    if is_staged(&facts.placement) {
        return Err(refuse(RefusalReason::Staged));
    }
    if to_kind == PlacementKind::Copy && target.is_none() {
        return Err(refuse(RefusalReason::InternalEdited));
    }
    let resume = match (&facts.observation, to_kind) {
        (Observation::Missing, _) => return Err(refuse(RefusalReason::ConvertGone)),
        (Observation::Directory | Observation::NotAFile, _) => {
            return Err(refuse(RefusalReason::NotAFile));
        }
        (Observation::OtherLink { .. }, _) => return Err(refuse(RefusalReason::OtherLink)),
        (Observation::IntactLink, PlacementKind::Link) => true,
        (Observation::File { .. }, PlacementKind::Copy) => {
            if facts.placement_hash != target.map(|target| target.content) {
                return Err(refuse(RefusalReason::Edited));
            }
            true
        }
        (Observation::File { .. }, PlacementKind::Link) => {
            match placement::classify(&placement, &facts.observation, facts.placement_hash) {
                Drift::InSync => {}
                Drift::PlacementEdited => return Err(refuse(RefusalReason::Edited)),
                _ => return Err(refuse(RefusalReason::Unusable)),
            }
            false
        }
        (Observation::IntactLink, PlacementKind::Copy) => false,
        _ => return Err(refuse(RefusalReason::Unusable)),
    };
    let internal = if to_kind == PlacementKind::Copy {
        let internal = internal.ok_or_else(|| refuse(RefusalReason::InternalEdited))?;
        let (_, working) = internal.placement.to_placement();
        if working.kind != PlacementKind::Internal
            || internal.placement.id != facts.placement.id
            || working.head != DEFAULT_BRANCH
            || is_staged(&internal.placement)
            || internal.placement_hash != target.map(|target| target.content)
            || placement::classify(&working, &internal.observation, internal.placement_hash)
                != Drift::InSync
        {
            return Err(refuse(RefusalReason::InternalEdited));
        }
        Some(internal.clone())
    } else {
        None
    };
    Ok(ConvertPlan {
        placement: facts.placement.clone(),
        observation: facts.observation.clone(),
        placement_hash: facts.placement_hash,
        to_kind,
        target: target.cloned(),
        internal,
        resume,
    })
}

#[cfg(test)]
mod tests;
