use super::{RenameError, RenamePlan, plan_rename};
use crate::fsx::{RealFileSystem, atomic_rewrite};
use crate::home::HomeLock;
use crate::id::{FolioId, ObjectId};
use crate::object::{Commit, hash_blob};
use crate::ops::history::{self, CommitMetadata, HistoryError, HistoryStores};
use crate::ops::{rules_error::RulesError, stage};
use crate::placement::PlacementOps;
use crate::placement_fs::PlacementFs;
use crate::placement_path::RegistryError;
use crate::registry::Registry;

/// The callback must check branch refs for the supplied parent Folio. Registry
/// placements also keep branches live. Metadata identity/time/body/group come
/// from the caller; rename supplies the operation's required commit message.
/// Returns the main commit, or None for a registry-only rename/no-op.
pub fn apply_rename(
    plan: &RenamePlan,
    mut metadata: CommitMetadata,
    fs: &PlacementFs<'_>,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
    is_live: &mut dyn FnMut(FolioId, &str) -> Result<bool, RegistryError>,
) -> Result<Option<ObjectId>, RenameError> {
    if plan_rename(&plan.old, &plan.new, fs, stores, registry)? != *plan {
        return Err(RenameError::PlanChanged);
    }
    metadata.message = format!("Rename `{}` to `{}`", plan.old, plan.new);
    if let Some(bytes) = &plan.replacement {
        Commit {
            content: hash_blob(bytes),
            parents: plan.row.commit_id.into_iter().collect(),
            author: metadata.author.clone(),
            time: metadata.time,
            message: metadata.message.clone(),
            body: metadata.body.clone(),
            group: metadata.group.clone(),
        }
        .encode_body()?;
    }
    registry.write(lock, |tx| {
        if tx.placement_at(&plan.row.path)?.as_ref() != Some(&plan.row)
            || tx.find_by_name(&plan.current)? != Some(plan.id())
        {
            return Err(RegistryError::PlacementChanged(plan.row.path.clone()));
        }
        tx.check_referrers(plan.id(), "rename", is_live)?;
        tx.rename(plan.id(), &plan.new)?;
        if plan.replacement.is_some() {
            tx.set_pending_rename(plan.id(), &plan.old, &plan.new)?;
        } else {
            tx.clear_pending_rename(plan.id())?;
        }
        Ok(())
    })?;
    let Some(bytes) = &plan.replacement else {
        return Ok(None);
    };
    if *bytes != plan.bytes {
        atomic_rewrite(
            &RealFileSystem,
            &plan.row.path.to_path_buf(),
            bytes,
            &plan.signature,
            |_| {},
        )?;
    }
    if fs
        .hash_placement(&plan.row.to_placement().1)
        .map_err(RulesError::from)?
        != hash_blob(bytes)
    {
        return Err(RenameError::InternalChanged(plan.row.path.to_path_buf()));
    }
    // Retain a pending stage so history can resume its published-ref window.
    if plan.row.staged.is_none() {
        stage::stage(&plan.row, fs, stores.objects, stores.refs, registry, lock)?;
    }
    let row = registry
        .internal_placement(plan.id())?
        .ok_or(history::HistoryRefusal::MissingInternal)
        .map_err(HistoryError::from)?;
    if !row
        .staged
        .is_some_and(|stage| stage.hash == hash_blob(bytes) && stage.size == bytes.len() as u64)
    {
        return Err(RenameError::InternalChanged(row.path.to_path_buf()));
    }
    let facts = history::gather_commit(&row, fs, stores, registry)?;
    let commit = history::judge_commit(&facts).map_err(HistoryError::from)?;
    let commit = history::apply_commit(&commit, metadata, fs, stores, registry, lock)?;
    registry.write(lock, |tx| tx.clear_pending_rename(plan.id()))?;
    Ok(Some(commit))
}
