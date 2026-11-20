use super::{HistoryError, HistoryRefusal};
use crate::config::names::FOLIOS_DIR;
use crate::home::HomeLock;
use crate::id::{FolioId, ObjectId};
use crate::object::ObjectKind;
use crate::placement_path::RegistryError;
use crate::ref_name::{RefKind, RefName};
use crate::refs::{Expect, RefError, RefStore};
use crate::registry::{Placement, Registry};
use crate::store::{ObjectStore, StoreError};

pub fn create_branch(
    id: FolioId,
    name: &RefName,
    commit: ObjectId,
    refs: &RefStore,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    if let Some(actual) = refs.read(RefKind::Branch, name)? {
        return Err(crate::refs::RefError::Stale {
            actual: Some(actual),
        }
        .into());
    }
    // Match ref-update target validation before changing any cached rows.
    let row = registry
        .folio_row(id)?
        .ok_or(RegistryError::FolioMissing(id))?;
    let directory =
        crate::folio_dir::validate(registry.home_path(), &row).map_err(|error| StoreError::Io {
            path: registry.home_path().join(FOLIOS_DIR).join(id.to_string()),
            source: std::io::Error::other(error),
        })?;
    let objects = ObjectStore::open(&directory, &row.format)?;
    let actual = objects.kind_of(commit).map_err(|error| match error {
        StoreError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            RefError::MissingTarget(commit)
        }
        error => RefError::Store(error),
    })?;
    if actual != ObjectKind::Commit {
        return Err(RefError::WrongKind { id: commit, actual }.into());
    }
    // Placements keep a branch live even before its first ref exists. Only a
    // dead name must lose its previous incarnation's cache before publication.
    if registry.placements_of_head(id, name.as_str())?.is_empty() {
        registry.write(lock, |tx| tx.delete_branch_references(id, name.as_str()))?;
    }
    refs.update(lock, RefKind::Branch, name, commit, Expect::Absent)?;
    refs.sync_refs()?;
    Ok(())
}

/// Pass `placements_of_head`, including the internal row, to this pure judge.
pub fn judge_delete_branch(placements: &[Placement]) -> Result<(), HistoryRefusal> {
    if placements.is_empty() {
        Ok(())
    } else {
        Err(HistoryRefusal::BranchInUse(
            placements
                .iter()
                .map(|row| row.path.to_path_buf())
                .collect(),
        ))
    }
}

pub fn delete_branch(
    id: FolioId,
    name: &RefName,
    old: ObjectId,
    refs: &RefStore,
    registry: &Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    judge_delete_branch(&registry.placements_of_head(id, name.as_str())?)?;
    refs.delete(lock, RefKind::Branch, name, Expect::Is(old))?;
    refs.sync_refs()?;
    Ok(())
}
