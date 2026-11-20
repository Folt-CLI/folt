use crate::home::HomeLock;
use crate::ops::refusal::{Refusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{Observation, PlacementKind, PlacementOps};
use crate::registry::Registry;

use super::super::check_file;
use super::judge::{RelocatePlan, judge_relocate_checked};
use super::target::gather_relocate_target;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RelocateStep {
    Inserted,
    Renamed,
    Deleted,
}

/// Inserts destination rows, renames without replacement, then deletes source rows.
pub fn apply_relocate(
    plan: &RelocatePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    apply_relocate_steps(plan, fs, registry, lock, |_, _| Ok(()))
}

pub(super) fn apply_relocate_steps(
    plan: &RelocatePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
    mut after: impl FnMut(RelocateStep, &Registry) -> Result<(), RulesError>,
) -> Result<(), RulesError> {
    let Some((source, pending)) = recheck_rows_and_source(plan, fs, registry, lock)? else {
        return Ok(());
    };
    if source != Observation::Missing {
        normalize_and_insert_rows(plan, fs, registry, lock, pending)?;
        after(RelocateStep::Inserted, registry)?;
        rename_entry(plan, fs, registry, lock, &source)?;
        after(RelocateStep::Renamed, registry)?;
    } else if !pending {
        return Err(Refusal {
            path: plan.facts.placement.path.to_path_buf(),
            reason: RefusalReason::Gone,
        }
        .into());
    }
    delete_source_rows(plan, registry, lock)?;
    after(RelocateStep::Deleted, registry)?;
    Ok(())
}

fn recheck_rows_and_source(
    plan: &RelocatePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<Option<(Observation, bool)>, RulesError> {
    let row = &plan.facts.placement;
    let (id, placement) = row.to_placement();
    let source = fs.read(id, &placement)?;
    let destination = gather_relocate_target(row, &plan.target.path, fs, registry)?;
    let rows = &plan.target.rows;
    let originals = rows
        .iter()
        .map(|(old, _)| registry.placement_at(&old.path))
        .collect::<Result<Vec<_>, _>>()?;
    let copies = rows
        .iter()
        .map(|(_, new)| registry.placement_at(&new.path))
        .collect::<Result<Vec<_>, _>>()?;
    let pending = copies
        .iter()
        .zip(rows)
        .all(|(copy, (_, new))| copy.as_ref() == Some(new));
    let finished = originals.iter().all(Option::is_none) && pending;
    if finished && source == Observation::Missing && destination.observation != Observation::Missing
    {
        return Ok(None);
    }
    for (actual, (old, _)) in originals.iter().zip(rows) {
        if actual.as_ref() != Some(old) {
            return Err(Refusal {
                path: old.path.to_path_buf(),
                reason: RefusalReason::RegistryChanged,
            }
            .into());
        }
    }
    let mut current = plan.facts.clone();
    current.observation = source.clone();
    if let Err(error) = judge_relocate_checked(&current, &destination) {
        if pending && source != Observation::Missing {
            cleanup_pending_rows(plan, registry, lock)?;
        }
        return Err(error.into());
    }
    if source != Observation::Missing {
        if let Err(error) = check_file(&plan.facts, fs) {
            if pending {
                cleanup_pending_rows(plan, registry, lock)?;
            }
            return Err(error);
        }
        // Recheck the entire planned prefix before either registry transaction.
        if destination.rows != *rows {
            return Err(Refusal {
                path: placement.path,
                reason: RefusalReason::RegistryChanged,
            }
            .into());
        }
    }
    Ok(Some((source, pending)))
}

fn normalize_and_insert_rows(
    plan: &RelocatePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
    pending: bool,
) -> Result<(), RulesError> {
    let rows = &plan.target.rows;
    // Normalising preserves meaning at the old path, even if the move fails.
    for (old, _) in rows {
        if old.kind == PlacementKind::Link {
            fs.normalize_link(old.id, &old.to_placement().1)?;
        }
    }
    if !pending {
        registry.write(lock, |tx| {
            for (_, new) in rows {
                tx.insert_placement(new)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn rename_entry(
    plan: &RelocatePlan,
    fs: &impl PlacementOps,
    registry: &mut Registry,
    lock: &HomeLock,
    source: &Observation,
) -> Result<(), RulesError> {
    let (id, placement) = plan.facts.placement.to_placement();
    if let Err(error) = fs.relocate(id, &placement, &plan.target.path.to_path_buf(), source) {
        cleanup_pending_rows(plan, registry, lock)?;
        return Err(error.into());
    }
    Ok(())
}

fn delete_source_rows(
    plan: &RelocatePlan,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    registry.write(lock, |tx| {
        for (old, _) in &plan.target.rows {
            tx.remove_placement(&old.path)?;
        }
        Ok(())
    })?;
    Ok(())
}

fn cleanup_pending_rows(
    plan: &RelocatePlan,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    registry.write(lock, |tx| {
        for (_, new) in &plan.target.rows {
            tx.remove_placement(&new.path)?;
        }
        Ok(())
    })?;
    Ok(())
}
