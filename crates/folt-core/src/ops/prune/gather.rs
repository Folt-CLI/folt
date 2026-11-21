use super::model::{PruneFacts, PrunePlan};
use crate::placement::{Observation, PlacementKind, PlacementOps};
use crate::placement_path::{PlacementPath, PlacementPrefix, RegistryError};
use crate::registry::{Placement, Registry};
use std::{collections::HashSet, fs, io};

pub(super) fn in_scope(row: &Placement, scope: &[PlacementPath]) -> bool {
    scope.is_empty()
        || scope
            .iter()
            .any(|path| row.path.to_path_buf().starts_with(path.to_path_buf()))
}

pub(super) fn observe(row: &Placement, fs: &impl PlacementOps) -> PruneFacts {
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
pub(crate) fn gather(
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
