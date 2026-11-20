use crate::folio::dir::path::former_home;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{Refusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{Observation, PlacementKind, PlacementOps};
use crate::registry::Registry;
use crate::store::ObjectStore;
use std::path::PathBuf;

use super::{check_file, check_row};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairLinkPlan {
    facts: PlacementFacts,
}

pub(super) fn former_link_home(facts: &PlacementFacts) -> Option<PathBuf> {
    let Observation::OtherLink { target } = &facts.observation else {
        return None;
    };
    let path = facts.placement.path.to_path_buf();
    let target = path.parent()?.join(target);
    former_home(&target, facts.placement.id)
}

pub fn judge_repair_link(facts: &PlacementFacts) -> Result<RepairLinkPlan, Refusal> {
    let refuse = |reason| Refusal {
        path: facts.placement.path.to_path_buf(),
        reason,
    };
    if facts.placement.kind == PlacementKind::Internal {
        return Err(refuse(RefusalReason::Internal));
    }
    if facts.placement.kind != PlacementKind::Link {
        return Err(refuse(RefusalReason::OtherLink));
    }
    match &facts.observation {
        Observation::OtherLink { .. } if former_link_home(facts).is_some() => {}
        _ => return Err(refuse(RefusalReason::OtherLink)),
    }
    Ok(RepairLinkPlan {
        facts: facts.clone(),
    })
}

pub fn apply_repair_link(
    plan: &RepairLinkPlan,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let _ = lock;
    check_row(&plan.facts.placement, registry)?;
    let (id, placement) = plan.facts.placement.to_placement();
    if fs.read(id, &placement)? == Observation::IntactLink {
        return Ok(());
    }
    check_file(&plan.facts, fs)?;
    fs.refresh(
        id,
        &placement,
        objects,
        ObjectId::from_bytes([0; 32]),
        Some(&plan.facts.observation),
    )?;
    Ok(())
}
