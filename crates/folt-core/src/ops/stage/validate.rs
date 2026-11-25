use std::collections::HashSet;

use super::stage_references;
use crate::config::names::{DEFAULT_BRANCH, OWNED_ID};
use crate::folio::name::FolioName;
use crate::front_matter::{FrontMatterError, applies_to, read_id, read_name, scan_window};
use crate::id::FolioId;
use crate::placement::PlacementKind;
use crate::registry::{Placement, Registry};

/// Checks optional identity metadata within the selected front matter boundary.
pub(super) fn validate_id(selected: &Placement, bytes: &[u8]) -> Result<(), String> {
    if !applies_to(&selected.path.to_path_buf()) && selected.kind != PlacementKind::Internal {
        return Ok(());
    }
    let id = match read_id(scan_window(bytes)) {
        Ok(id) => id,
        Err(FrontMatterError::Binary) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if let Some(id) = id
        && id != selected.id
    {
        return Err(format!(
            "{OWNED_ID}: expected Folio id {}, actual Folio id {id}",
            selected.id
        ));
    }
    Ok(())
}

/// Checks merged identity, references, cycles and names; stage shares only identity validation.
pub(crate) fn validate_merged(
    row: &Placement,
    bytes: &[u8],
    registry: &Registry,
) -> Result<(), String> {
    validate_id(row, bytes)?;
    let references = stage_references(row, bytes).map_err(|error| error.to_string())?;
    let mut pending = Vec::new();
    for reference in references {
        let name = reference
            .name
            .as_deref()
            .map(FolioName::parse)
            .transpose()
            .map_err(|error| error.to_string())?;
        let child = match reference.id {
            Some(id) => Some(id),
            None => name
                .as_ref()
                .map(|name| registry.find_by_name(name))
                .transpose()
                .map_err(|error| error.to_string())?
                .flatten(),
        };
        pending.extend(child);
    }
    let mut visited = HashSet::new();
    while let Some(child) = pending.pop() {
        if child == row.id {
            return Err(format!("reference cycle returns to Folio {}", row.id));
        }
        if !visited.insert(child) {
            continue;
        }
        for reference in registry
            .references(child, DEFAULT_BRANCH)
            .map_err(|error| error.to_string())?
        {
            if !reference.child.is_empty() {
                pending.push(FolioId::parse(&reference.child).map_err(|error| error.to_string())?);
            }
        }
    }
    let prefix = scan_window(bytes);
    if !applies_to(&row.path.to_path_buf()) && row.kind != PlacementKind::Internal {
        return Ok(());
    }
    let name = match read_name(prefix) {
        Ok(name) => name,
        Err(FrontMatterError::Binary) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if let Some(name) = name {
        let name = FolioName::parse(&name).map_err(|error| error.to_string())?;
        if registry
            .find_by_name(&name)
            .map_err(|error| error.to_string())?
            .is_some_and(|id| id != row.id)
        {
            return Err(format!("folio-name '{name}' belongs to a different Folio"));
        }
    }
    Ok(())
}
