use super::RenameError;
use crate::folio::name::FolioName;
use crate::fsx::Signature;
use crate::id::FolioId;
use crate::object::{ObjectKind, hash_blob};
use crate::ops::history::{self, HistoryError, HistoryStores};
use crate::ops::stage::refuse::is_staged;
use crate::placement::Observation;
use crate::placement_fs::PlacementFs;
use crate::placement_path::RegistryError;
use crate::registry::{Placement, Registry};
use std::fs;

mod replacement;

use replacement::prepare_replacement;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenamePlan {
    pub(super) row: Placement,
    pub(super) old: FolioName,
    pub(super) new: FolioName,
    pub(super) current: FolioName,
    pub(super) bytes: Vec<u8>,
    pub(super) signature: Signature,
    pub(super) replacement: Option<Vec<u8>>,
}

impl RenamePlan {
    pub fn id(&self) -> FolioId {
        self.row.id
    }
}

/// Reads only the internal row. Referrers are checked in apply's registry
/// transaction, with the caller's cross-Folio branch-ref liveness callback.
pub(crate) fn plan_rename(
    old: &FolioName,
    new: &FolioName,
    fs: &PlacementFs<'_>,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<RenamePlan, RenameError> {
    let (id, retry) = resolve_rename(old, new, registry)?;
    let row = registry
        .internal_placement(id)?
        .ok_or(history::HistoryRefusal::MissingInternal)
        .map_err(HistoryError::from)?;
    let facts = crate::ops::working::gather(&row, fs, stores.refs, registry)?;
    let Observation::File { ref signature } = facts.observation else {
        return Err(RenameError::InternalChanged(row.path.to_path_buf()));
    };
    let (bytes, base_bytes) = read_internal(&row, stores)?;
    let replacement = prepare_replacement(&row, &base_bytes, old, new, retry)?;
    check_internal(
        &facts,
        &bytes,
        replacement.as_deref(),
        retry,
        fs,
        stores,
        registry,
    )?;
    if fs::metadata(row.path.to_path_buf()).map(|m| crate::fsx::signature(&m))? != *signature {
        return Err(RenameError::PlanChanged);
    }
    Ok(RenamePlan {
        row,
        old: old.clone(),
        new: new.clone(),
        current: if retry { new.clone() } else { old.clone() },
        bytes,
        signature: signature.clone(),
        replacement,
    })
}

fn resolve_rename(
    old: &FolioName,
    new: &FolioName,
    registry: &Registry,
) -> Result<(FolioId, bool), RenameError> {
    let (id, retry) = match registry.find_by_name(old)? {
        Some(id) => (id, false),
        None => (
            registry
                .find_by_name(new)?
                .ok_or_else(|| RenameError::UnknownName(old.clone()))?,
            true,
        ),
    };
    let pending = registry.pending_rename(id)?;
    if retry && pending.as_ref() != Some(&(old.clone(), new.clone())) {
        return Err(RenameError::UnknownName(old.clone()));
    }
    if let Some((pending_old, pending_new)) = pending
        && (pending_old != *old || pending_new != *new)
    {
        return Err(RenameError::PendingRename {
            old: pending_old,
            new: pending_new,
        });
    }
    if registry
        .find_by_name(new)?
        .is_some_and(|holder| holder != id)
    {
        return Err(RegistryError::NameTaken(new.clone()).into());
    }
    Ok((id, retry))
}

fn read_internal(
    row: &Placement,
    stores: &HistoryStores<'_>,
) -> Result<(Vec<u8>, Vec<u8>), RenameError> {
    let bytes = fs::read(row.path.to_path_buf())?;
    let base = row
        .base
        .as_ref()
        .ok_or_else(|| RenameError::InternalChanged(row.path.to_path_buf()))?;
    let mut base_bytes = Vec::new();
    stores
        .objects
        .get_to(base.hash, ObjectKind::Blob, &mut base_bytes)?;
    Ok((bytes, base_bytes))
}

fn check_internal(
    facts: &crate::ops::facts::PlacementFacts,
    bytes: &[u8],
    replacement: Option<&[u8]>,
    retry: bool,
    fs: &PlacementFs<'_>,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<(), RenameError> {
    let row = &facts.placement;
    let base = row
        .base
        .as_ref()
        .ok_or_else(|| RenameError::InternalChanged(row.path.to_path_buf()))?;
    let own_edit = retry && replacement == Some(bytes);
    let clean = hash_blob(bytes) == base.hash;
    let own_stage = own_edit
        && row.staged.is_some_and(|stage| {
            stage.hash == hash_blob(bytes) && stage.size == bytes.len() as u64
        });
    if (!clean && !own_edit) || (is_staged(row) && !own_stage) {
        return Err(RenameError::InternalChanged(row.path.to_path_buf()));
    }
    // Ref advancement is owned by history. Only a pending rename stage can
    // resume a published commit; other branch movement must be resolved first.
    if facts.tip != row.commit_id {
        if !own_stage {
            return Err(RenameError::PlanChanged);
        }
        history::judge_commit(&history::gather_commit(row, fs, stores, registry)?)
            .map_err(HistoryError::from)?;
    }
    Ok(())
}
