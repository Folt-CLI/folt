use super::gather::{gather, in_scope, observe};
use super::judge::{judge, needs_confirmation};
use super::model::{PruneError, PruneOptions, PrunePlan, PruneReport};
use super::report::report;
use crate::home::HomeLock;
use crate::placement::PlacementOps;
use crate::placement_path::PlacementPath;
use crate::registry::Registry;

/// Rechecks selected rows and observations inside one transaction under HomeLock.
/// Changed rows are reported and kept. A registry failure rolls back every delete.
pub(crate) fn apply(
    plan: &PrunePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
    confirm_mass_deletion: bool,
) -> Result<PruneReport, PruneError> {
    registry.write(lock, |tx| {
        let all_rows = tx.all_placements()?;
        let in_scope = all_rows
            .iter()
            .filter(|row| in_scope(row, &plan.scope))
            .count();
        let mut checked = Vec::new();
        for original in &plan.facts {
            let mut facts = observe(&original.row, fs);
            if tx.placement_at(&original.row.path)?.as_ref() != Some(&original.row) {
                facts.observation = Err("registry changed before prune".into());
            } else if judge(original).is_some() && judge(&facts).is_none() {
                facts.observation = Err("observation changed before prune; run prune again".into());
            }
            checked.push(facts);
        }
        let candidates = checked
            .iter()
            .filter(|facts| judge(facts).is_none())
            .count();
        if needs_confirmation(candidates, in_scope) && !confirm_mass_deletion {
            return Ok(Err(PruneError::ConfirmationRequired {
                dropped: candidates,
                in_scope,
            }));
        }
        for facts in &mut checked {
            if judge(facts).is_none() {
                // External editors do not take HomeLock. Recheck immediately
                // before each delete, even after the batch confirmation check.
                let row = &facts.row;
                let mut current = observe(row, fs);
                if tx.placement_at(&row.path)?.as_ref() != Some(row) {
                    current.observation = Err("registry changed before prune".into());
                }
                if judge(&current).is_none() {
                    tx.remove_placement(&row.path)?;
                }
                *facts = current;
            }
        }
        Ok(Ok(report(&checked, &all_rows, false)))
    })?
}

/// Dry run by default; applying requires options.apply and the caller's home lock.
pub(crate) fn prune_placements(
    registry: &mut Registry,
    fs: &impl PlacementOps,
    lock: &HomeLock,
    scope: &[PlacementPath],
    options: PruneOptions,
) -> Result<PruneReport, PruneError> {
    let plan = gather(registry, fs, scope)?;
    if options.apply {
        apply(&plan, fs, registry, lock, options.confirm_mass_deletion)
    } else {
        Ok(plan.report())
    }
}
