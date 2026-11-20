use crate::home::HomeLock;
use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::ops::stage::refuse::is_staged;
use crate::placement::{self, Drift, Observation, PlacementKind, PlacementOps};
use crate::registry::Registry;

use super::{check_file, check_row};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovePlan {
    facts: PlacementFacts,
    delete_file: bool,
}

pub fn judge_remove(
    facts: &PlacementFacts,
    delete_file: bool,
    discard: bool,
) -> Result<RemovePlan, PlacementRefusal> {
    let (_, placement) = facts.placement.to_placement();
    let refuse = |reason| PlacementRefusal {
        path: placement.path.clone(),
        reason,
    };
    if placement.kind == PlacementKind::Internal {
        return Err(refuse(RefusalReason::Internal));
    }
    if delete_file && facts.observation != Observation::Missing {
        match (placement.kind, &facts.observation) {
            (PlacementKind::Link, Observation::IntactLink) => {}
            (PlacementKind::Copy, Observation::File { .. }) => {
                if !discard {
                    if is_staged(&facts.placement) {
                        return Err(refuse(RefusalReason::Staged));
                    }
                    match placement::classify(&placement, &facts.observation, facts.placement_hash)
                    {
                        Drift::InSync => {}
                        Drift::PlacementEdited => return Err(refuse(RefusalReason::Edited)),
                        _ => return Err(refuse(RefusalReason::Unusable)),
                    }
                }
            }
            (_, Observation::Directory | Observation::NotAFile) => {
                return Err(refuse(RefusalReason::NotAFile));
            }
            _ => return Err(refuse(RefusalReason::LinkReplaced)),
        }
    }
    Ok(RemovePlan {
        facts: facts.clone(),
        delete_file,
    })
}

pub fn apply_remove(
    plan: &RemovePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    apply_remove_after_row(plan, fs, registry, lock, || Ok(()))
}

fn apply_remove_after_row(
    plan: &RemovePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
    after_delete: impl FnOnce() -> Result<(), RulesError>,
) -> Result<(), RulesError> {
    let row = &plan.facts.placement;
    check_row(row, registry)?;
    let (id, placement) = row.to_placement();
    let observation = fs.read(id, &placement)?;
    if observation != Observation::Missing {
        check_file(&plan.facts, fs)?;
    }
    registry.write(lock, |tx| tx.remove_placement(&row.path))?;
    after_delete()?;
    if plan.delete_file && observation != Observation::Missing {
        fs.unlink(id, &placement, &plan.facts.observation)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
