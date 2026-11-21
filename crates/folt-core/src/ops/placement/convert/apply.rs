use crate::config::names::DEFAULT_BRANCH;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{self, Observation, PlacementError, PlacementKind, PlacementOps};
use crate::registry::{self, Registry};
use crate::store::ObjectStore;

use super::ConvertPlan;

/// Rechecks the selected rows and files, publishes the entry, then writes its row.
/// A retry with the requested shape syncs its parent before the registry update.
pub(crate) fn apply_convert(
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

fn converted_row(
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
