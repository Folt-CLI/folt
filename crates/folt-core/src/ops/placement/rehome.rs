use crate::folio::dir::path::former_home;
use crate::home::HomeLock;
use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::resolve_existing_ancestor;
use crate::placement::{Observation, PlacementKind, PlacementOps};
use crate::placement_path::{self, RegistryError};
use crate::registry::{self, Registry};
use crate::store::ObjectStore;

use super::repair::{apply_repair_link, former_link_home, judge_repair_link};

/// Rewrites every internal path in one transaction before repairing former-home links.
/// Repeating with the same old home resumes the link pass after interruption.
pub(crate) fn rehome(
    old_home: &std::path::Path,
    fs: &impl PlacementOps,
    objects: &ObjectStore,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let old_home = resolve_existing_ancestor(old_home)?;
    let mut updates = Vec::new();
    let mut repairs = Vec::new();
    let mut prefix = None;
    for id in registry.folios()? {
        let row = registry
            .internal_placement(id)?
            .ok_or_else(|| PlacementRefusal {
                path: old_home.to_path_buf(),
                reason: RefusalReason::MissingInternal,
            })?;
        let path = row.path.to_path_buf();
        let home = former_home(&path, id).ok_or_else(|| PlacementRefusal {
            path: path.clone(),
            reason: RefusalReason::MixedHomes,
        })?;
        let home = resolve_existing_ancestor(&home)?;
        let current = fs.link_target(id);
        let current_home =
            resolve_existing_ancestor(&former_home(&current, id).ok_or_else(|| {
                PlacementRefusal {
                    path: current.clone(),
                    reason: RefusalReason::MixedHomes,
                }
            })?)?;
        if (home != old_home && home != current_home)
            || prefix.as_ref().is_some_and(|prefix| prefix != &home)
        {
            return Err(PlacementRefusal {
                path,
                reason: RefusalReason::MixedHomes,
            }
            .into());
        }
        prefix = Some(home);
        let new_path = placement_path::PlacementPath::resolve(current)?;
        if row.path != new_path {
            if registry.placement_at(&new_path)?.is_some() {
                return Err(PlacementRefusal {
                    path: new_path.to_path_buf(),
                    reason: RefusalReason::DestinationRegistered,
                }
                .into());
            }
            updates.push((row.path, new_path));
        }
        for link in registry
            .placements_of(id)?
            .into_iter()
            .filter(|row| row.kind == PlacementKind::Link)
        {
            let observation = fs.read(id, &link.to_placement().1)?;
            if observation == Observation::IntactLink {
                continue;
            }
            let facts = PlacementFacts {
                placement: link,
                observation,
                placement_hash: None,
                tip: None,
            };
            let plan = judge_repair_link(&facts)?;
            let target_home = former_link_home(&facts)
                .map(|home| resolve_existing_ancestor(&home))
                .transpose()?;
            if target_home.as_ref() != Some(&old_home) {
                return Err(PlacementRefusal {
                    path: facts.placement.path.to_path_buf(),
                    reason: RefusalReason::OtherLink,
                }
                .into());
            }
            repairs.push(plan);
        }
    }
    registry.write(lock, |tx| {
        for (old, new) in &updates {
            if tx.move_placement(old, new)? != registry::MoveResult::Moved {
                return Err(RegistryError::PlacementChanged(old.clone()));
            }
        }
        Ok(())
    })?;
    for plan in repairs {
        apply_repair_link(&plan, fs, objects, registry, lock)?;
    }
    Ok(())
}
