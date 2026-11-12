//! Prunes absent external placement rows, never files or objects.
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;

use crate::config::limits::PRUNE_CONFIRM_MIN;
use crate::home::HomeLock;
use crate::id::FolioId;
use crate::id::ObjectId;
use crate::placement::{self, Drift, Observation, PlacementKind, PlacementOps};
use crate::registry::{Placement, PlacementPath, PlacementPrefix, Registry, RegistryError};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PruneOptions {
    pub apply: bool,
    pub confirm_mass_deletion: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneFacts {
    pub row: Placement,
    pub observation: Result<Observation, String>,
    pub parent_exists: Result<bool, String>,
    pub hash: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DroppedPlacement {
    pub path: PlacementPath,
    pub kind: PlacementKind,
    /// The staged object held by this copy, if any.
    pub stage: Option<ObjectId>,
    /// A copy's stage has no reference in the surviving row set.
    pub stage_unreachable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeptPlacement {
    pub path: PlacementPath,
    pub kind: PlacementKind,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PruneCounts {
    pub in_scope: usize,
    pub dropped: usize,
    pub kept: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneReport {
    /// In a dry run these are the rows that would be dropped.
    pub dropped: Vec<DroppedPlacement>,
    pub kept: Vec<KeptPlacement>,
    /// Distinct (Folio, blob) stages held only by dropped rows, including twins.
    pub unreachable_staged_blobs: Vec<(FolioId, ObjectId)>,
    pub warnings: Vec<String>,
    pub counts: PruneCounts,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct PrunePlan {
    facts: Vec<PruneFacts>,
    all_rows: Vec<Placement>,
    scope: Vec<PlacementPath>,
}

#[derive(Debug)]
pub enum PruneError {
    Registry(RegistryError),
    ConfirmationRequired { dropped: usize, in_scope: usize },
}

impl fmt::Display for PruneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(f),
            Self::ConfirmationRequired { dropped, in_scope } => write!(
                f,
                "prune would drop {dropped} of {in_scope} rows in scope; explicit mass-deletion confirmation is required"
            ),
        }
    }
}

impl Error for PruneError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::ConfirmationRequired { .. } => None,
        }
    }
}

impl From<RegistryError> for PruneError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

fn in_scope(row: &Placement, scope: &[PlacementPath]) -> bool {
    scope.is_empty()
        || scope
            .iter()
            .any(|path| row.path.to_path_buf().starts_with(path.to_path_buf()))
}

fn observe(row: &Placement, fs: &impl PlacementOps) -> PruneFacts {
    let (id, placement) = row.to_placement();
    let mut observation = fs.read(id, &placement).map_err(|error| error.to_string());
    let hash =
        if matches!(observation, Ok(Observation::File { .. })) && row.kind != PlacementKind::Link {
            match fs.hash_placement(&placement) {
                Ok(hash) => Some(hash),
                Err(error) => {
                    observation = Err(error.to_string());
                    None
                }
            }
        } else {
            None
        };
    let parent_exists = match placement
        .path
        .parent()
        .ok_or_else(|| {
            io::Error::other(format!("path has no parent: {}", placement.path.display()))
        })
        .and_then(fs::metadata)
    {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err("parent is not a directory".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    };
    PruneFacts {
        row: row.clone(),
        observation,
        parent_exists,
        hash,
    }
}

/// Reads the whole registry for stage reachability, observing only selected rows.
/// Scope keys name a path or directory itself (use resolve_directory for directories).
pub fn gather(
    registry: &Registry,
    fs: &impl PlacementOps,
    scope: &[PlacementPath],
) -> Result<PrunePlan, RegistryError> {
    let all_rows = registry.all_placements()?;
    let mut selected = HashSet::new();
    for path in scope {
        selected.insert(path.clone());
        selected.extend(
            registry
                .placements_under(&PlacementPrefix::from(path))?
                .into_iter()
                .map(|row| row.path),
        );
    }
    let facts = all_rows
        .iter()
        .filter(|row| {
            scope.is_empty()
                || selected.contains(&row.path)
                || (row.kind == PlacementKind::Internal && in_scope(row, scope))
        })
        .map(|row| observe(row, fs))
        .collect();
    Ok(PrunePlan {
        facts,
        all_rows,
        scope: scope.to_vec(),
    })
}

/// Returns None only for a missing external row with an existing parent.
/// All decisions depend solely on supplied facts.
pub fn judge(facts: &PruneFacts) -> Option<String> {
    if facts.row.kind == PlacementKind::Internal {
        return Some(match &facts.observation {
            Err(error) => {
                format!("internal placement; read error: {error}; rebuild belongs to #51")
            }
            Ok(Observation::Missing) => "internal placement missing; damaged home (#51)".into(),
            Ok(_) => "internal placement".into(),
        });
    }
    let observation = match &facts.observation {
        Ok(observation) => observation,
        Err(error) => return Some(format!("read error: {error}")),
    };
    match &facts.parent_exists {
        Ok(true) => {}
        Ok(false) => return Some("parent directory missing".into()),
        Err(error) => return Some(format!("parent read error: {error}")),
    }
    if observation == &Observation::Missing {
        return None;
    }
    let (_, placement) = facts.row.to_placement();
    if let (Observation::File { signature }, Some(stage)) = (observation, facts.row.staged)
        && facts.hash == Some(stage.hash)
        && signature.size == stage.size
    {
        return Some("matches stage".into());
    }
    Some(
        match placement::classify(&placement, observation, facts.hash) {
            Drift::InSync => "in sync",
            Drift::Detached => "a different file than expected",
            Drift::PlacementEdited => "edited or different",
            Drift::Unusable => "unusable entry",
            Drift::Gone => return None,
        }
        .into(),
    )
}

/// Requires at least five drops AND strictly more than half the scoped rows.
/// Division avoids overflow when comparing counts.
pub fn needs_confirmation(dropped: usize, in_scope: usize) -> bool {
    dropped >= PRUNE_CONFIRM_MIN && dropped > in_scope / 2
}

impl PrunePlan {
    pub fn report(&self) -> PruneReport {
        report(&self.facts, &self.all_rows, true)
    }
}

fn report(facts: &[PruneFacts], all_rows: &[Placement], dry_run: bool) -> PruneReport {
    let mut kept = Vec::new();
    let mut dropped_rows = Vec::new();
    let mut warnings = Vec::new();
    for facts in facts {
        if let Some(reason) = judge(facts) {
            if reason != "in sync"
                && reason != "matches stage"
                && facts.row.kind != PlacementKind::Internal
            {
                warnings.push(format!(
                    "{}: {reason}",
                    facts.row.path.to_path_buf().display()
                ));
            }
            kept.push(KeptPlacement {
                path: facts.row.path.clone(),
                kind: facts.row.kind,
                reason,
            });
        } else {
            dropped_rows.push(&facts.row);
        }
    }
    let paths: HashSet<_> = dropped_rows.iter().map(|row| &row.path).collect();
    let retained: HashSet<_> = all_rows
        .iter()
        .filter(|row| !paths.contains(&row.path))
        .filter_map(|row| row.staged.map(|stage| (row.id, stage.hash)))
        .collect();
    let mut seen = HashSet::new();
    let unreachable: Vec<_> = dropped_rows
        .iter()
        .filter_map(|row| row.staged.map(|stage| (row.id, stage.hash)))
        .filter(|stage| !retained.contains(stage))
        .filter(|stage| seen.insert(*stage))
        .collect();
    let dropped: Vec<_> = dropped_rows
        .iter()
        .map(|row| DroppedPlacement {
            path: row.path.clone(),
            kind: row.kind,
            stage: row.staged.map(|stage| stage.hash),
            stage_unreachable: row.kind == PlacementKind::Copy
                && row
                    .staged
                    .is_some_and(|stage| seen.contains(&(row.id, stage.hash))),
        })
        .collect();
    if !dropped.is_empty() {
        warnings
            .push("a dropped row's file may still exist elsewhere; rebind is out of scope".into());
    }
    PruneReport {
        counts: PruneCounts {
            in_scope: facts.len(),
            dropped: dropped.len(),
            kept: kept.len(),
        },
        dropped,
        kept,
        unreachable_staged_blobs: unreachable,
        warnings,
        dry_run,
    }
}

/// Rechecks selected rows and observations inside one transaction under HomeLock.
/// Changed rows are reported and kept. A registry failure rolls back every delete.
pub fn apply(
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
pub fn prune_placements(
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

#[cfg(test)]
mod tests;
