use crate::home::HomeLock;
use crate::object::ObjectKind;
use crate::ops::history::{HistoryError, HistoryRefusal, HistoryStores, require_row};
use crate::ops::stage::stage_references;
use crate::placement::{ConflictOperation, PlacementOps};
use crate::placement_path::RegistryError;
use crate::registry::{Placement, Registry};

/// Restores the pre-merge file and clears this copy's stage and conflict state.
pub(crate) fn abort_merge(
    row: &Placement,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Result<(), HistoryError> {
    require_row(registry, row)?;
    let state = row
        .conflict
        .as_ref()
        .filter(|state| state.operation == ConflictOperation::Merge)
        .ok_or_else(|| HistoryRefusal::NotConflicted(row.path.to_path_buf()))?;
    let placement = row.to_placement().1;
    let observation = fs.read(row.id, &placement)?;
    let bytes = stores
        .objects
        .get(state.pre_merge, ObjectKind::Blob, u64::MAX)?;
    let references = row
        .staged
        .map(|_| stage_references(row, &bytes))
        .transpose()
        .map_err(|error| HistoryRefusal::MergeInvalid(error.to_string()))?;
    let written = fs.refresh(
        row.id,
        &placement,
        stores.objects,
        state.pre_merge,
        Some(&observation),
    )?;
    let mut updated = row.clone();
    updated.base = written.base;
    updated.staged = None;
    updated.conflict = None;
    registry.write(lock, |tx| {
        if tx.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(RegistryError::PlacementChanged(row.path.clone()));
        }
        tx.update_placement(&updated)?;
        if let Some(references) = &references {
            tx.rebuild_references(row.id, &row.head, references)?;
        }
        Ok(())
    })?;
    Ok(())
}
