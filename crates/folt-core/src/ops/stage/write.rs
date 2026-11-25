use super::stage_references;
use crate::config::limits::SCAN_WINDOW;
use crate::history::RefStore;
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::ObjectKind;
use crate::ops::rules_error::RulesError;
use crate::placement::{PlacementError, PlacementKind, StagedBlob};
use crate::placement_path::RegistryError;
use crate::ref_name::RefKind;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

pub(super) fn write_stage(
    row: &Placement,
    selected: &Placement,
    staged: Option<StagedBlob>,
    blob: Option<(&ObjectStore, ObjectId, &RefStore)>,
    registry: &mut Registry,
    lock: &HomeLock,
) -> Result<(), RulesError> {
    let markdown = crate::front_matter::applies_to(&selected.path.to_path_buf());
    let prefix = blob
        .filter(|_| markdown || selected.kind == PlacementKind::Internal)
        .map(|(objects, hash, _)| objects.get_prefix(hash, ObjectKind::Blob, SCAN_WINDOW))
        .transpose()
        .map_err(PlacementError::Store)?;
    // Internal working files have no Markdown extension. Only their own named
    // front matter enables detection; copies and links use the selected path.
    if !markdown && selected.kind == PlacementKind::Internal {
        match crate::front_matter::read_name(prefix.as_deref().unwrap_or_default()) {
            Ok(_) | Err(crate::front_matter::FrontMatterError::Binary) => {}
            Err(error) => {
                return Err(RegistryError::References(format!(
                    "{error}; the internal file starts with a front matter block that cannot be read: fix or remove it to stage"
                ))
                .into());
            }
        }
    }
    if let Some(prefix) = &prefix {
        super::validate::validate_id(selected, prefix).map_err(RegistryError::References)?;
    }
    let references = blob
        .map(|_| stage_references(selected, prefix.as_deref().unwrap_or_default()))
        .transpose()
        .map_err(|error| RegistryError::References(error.to_string()))?;
    let live_branches = blob
        .map(|(_, _, refs)| refs.list(RefKind::Branch))
        .transpose()?;
    registry.write(lock, |tx| {
        if tx.placement_at(&row.path)?.as_ref() != Some(row) {
            return Err(RegistryError::PlacementChanged(row.path.clone()));
        }
        let mut updated = row.clone();
        updated.staged = staged;
        tx.update_placement(&updated)?;
        if let Some(references) = &references {
            let live = live_branches
                .as_ref()
                .ok_or_else(|| {
                    RegistryError::References("missing live branches for staged references".into())
                })?
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            tx.purge_dead_branch_references(row.id, &live)?;
            // Internal rows (including the target of a link) are constrained to
            // main by the placements schema; copies stage on their own head.
            tx.rebuild_references(row.id, &row.head, references)?;
        }
        Ok(())
    })?;
    Ok(())
}
