use crate::config::names::DEFAULT_BRANCH;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{self, Drift, Observation, PlacementError, PlacementKind, PlacementOps};
use crate::registry::{self, Registry};
use crate::store::ObjectStore;

use super::move_::MoveTarget;

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
pub fn judge_convert(
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
    if facts.placement.staged.is_some() {
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
            || internal.placement.staged.is_some()
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

/// Rechecks the selected rows and files, publishes the entry, then writes its row.
/// A retry with the requested shape syncs its parent before the registry update.
pub fn apply_convert(
    plan: &ConvertPlan,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let refuse = |reason| {
        RulesError::Refused(PlacementRefusal {
            path: plan.placement.path.to_path_buf(),
            reason,
        })
    };
    let check = |row: &registry::Placement, observation: &Observation, hash: Option<ObjectId>| {
        if registry.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(refuse(RefusalReason::RegistryChanged));
        }
        let (id, placement) = row.to_placement();
        if fs.read(id, &placement)? != *observation
            || (matches!(observation, Observation::File { .. })
                && hash.is_some()
                && Some(fs.hash_placement(&placement)?) != hash)
        {
            return Err(refuse(RefusalReason::PlacementChanged));
        }
        Ok(())
    };
    check(&plan.placement, &plan.observation, plan.placement_hash)?;
    if let Some(internal) = &plan.internal {
        check(
            &internal.placement,
            &internal.observation,
            internal.placement_hash,
        )?;
    }
    let base = if plan.resume {
        fs.sync_parent(&plan.placement.to_placement().1)?;
        match &plan.observation {
            Observation::File { signature } => Some(placement::PlacementBase {
                hash: plan
                    .target
                    .as_ref()
                    .ok_or(RulesError::Internal("missing copy target"))?
                    .content,
                signature: signature.clone(),
            }),
            _ => None,
        }
    } else {
        let (id, placement) = plan.placement.to_placement();
        fs.convert(
            id,
            &placement,
            plan.to_kind,
            objects,
            plan.target
                .as_ref()
                .map(|target| target.content)
                .or(plan.placement_hash)
                .ok_or(RulesError::Internal("missing conversion content"))?,
            &plan.observation,
        )
        .map_err(|error| match error {
            PlacementError::PlacementChanged => refuse(RefusalReason::PlacementChanged),
            error => RulesError::Placement(error),
        })?
        .base
    };
    let updated = converted_row(plan, base);
    registry.write(lock, |tx| tx.update_placement(&updated))?;
    Ok(())
}

pub fn converted_row(
    plan: &ConvertPlan,
    base: Option<placement::PlacementBase>,
) -> registry::Placement {
    let mut updated = plan.placement.clone();
    updated.kind = plan.to_kind;
    updated.head = DEFAULT_BRANCH.into();
    updated.commit_id = if plan.to_kind == PlacementKind::Copy {
        plan.target.as_ref().map(|target| target.commit)
    } else {
        None
    };
    updated.base = if plan.to_kind == PlacementKind::Copy {
        base
    } else {
        None
    };
    updated.staged = None;
    updated
}

#[cfg(test)]
mod tests;
