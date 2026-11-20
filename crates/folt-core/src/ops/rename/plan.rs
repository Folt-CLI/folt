use super::RenameError;
use crate::folio_name::FolioName;
use crate::front_matter::{self, Block, FrontMatterError};
use crate::fsx::Signature;
use crate::id::FolioId;
use crate::object::{ObjectKind, hash_blob};
use crate::ops::history::{self, HistoryError, HistoryStores};
use crate::ops::stage;
use crate::placement::Observation;
use crate::placement_fs::PlacementFs;
use crate::placement_path::RegistryError;
use crate::registry::{Placement, Registry};
use std::fs;

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

fn renamed(bytes: &[u8], name: &FolioName) -> Result<Vec<u8>, RenameError> {
    let edit = Block::set_name(
        front_matter::locate(bytes)?.as_ref(),
        bytes,
        Some(name.as_str()),
        false,
    )?;
    Ok(match edit {
        Some(edit) => front_matter::apply_edit(bytes, &edit)?,
        None => bytes.to_vec(),
    })
}

/// Reads only the internal row. Referrers are checked in apply's registry
/// transaction, with the caller's cross-Folio branch-ref liveness callback.
pub fn plan_rename(
    old: &FolioName,
    new: &FolioName,
    fs: &PlacementFs<'_>,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<RenamePlan, RenameError> {
    let (id, retry) = resolve_rename(old, new, registry)?;
    let row = registry
        .internal_placement(id)?
        .ok_or(history::Refusal::MissingInternal)
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

fn prepare_replacement(
    row: &Placement,
    base_bytes: &[u8],
    old: &FolioName,
    new: &FolioName,
    retry: bool,
) -> Result<Option<Vec<u8>>, RenameError> {
    let named = match front_matter::read_name(base_bytes) {
        Ok(Some(name)) => {
            // A fresh rename must leave a base that its pending retry accepts.
            if !retry && name != old.as_str() && name != new.as_str() {
                return Err(RenameError::InternalNameMismatch {
                    path: row.path.to_path_buf(),
                    name: name.into_owned(),
                });
            }
            true
        }
        Ok(None) | Err(FrontMatterError::Binary) => false,
        Err(error) => return Err(RenameError::FrontMatter(error)),
    };
    // The pending record identifies the operation. The base must still hold
    // its old name or the new name from a commit completed before record clear.
    if retry {
        let base_name = front_matter::read_name(base_bytes)?;
        if !named
            || !matches!(base_name.as_deref(), Some(name) if name == old.as_str() || name == new.as_str())
        {
            return Err(RenameError::UnknownName(old.clone()));
        }
    }
    let replacement = if named && old != new {
        let replacement = renamed(base_bytes, new)?;
        (replacement != base_bytes).then_some(replacement)
    } else {
        None
    };
    // Preflight both history and the exact replacement prefix before any
    // registry/file writes, even when resuming an own edit or existing stage.
    for content in std::iter::once(base_bytes).chain(replacement.as_deref()) {
        for reference in stage::stage_references(row, content)? {
            crate::registry::reference_name(&reference)?;
        }
    }
    Ok(replacement)
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
    if (!clean && !own_edit) || (row.staged.is_some() && !own_stage) {
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
