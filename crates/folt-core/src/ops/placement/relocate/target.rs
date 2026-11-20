use crate::ops::refusal::{Refusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{Observation, PlacementOps};
use crate::placement_path::{self};
use crate::registry::{self, Registry};

/// Destination and registry facts collected while the home lock is held.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelocateTarget {
    pub path: placement_path::PlacementPath,
    pub observation: Observation,
    pub registered: bool,
    pub pending: bool,
    pub rows: Vec<(registry::Placement, registry::Placement)>,
}

/// Directory callers supply a synthetic row whose path and destination use
/// `PlacementPath::resolve_directory`; its kind is Copy and its base is absent.
pub fn gather_relocate_target(
    row: &registry::Placement,
    to: &placement_path::PlacementPath,
    fs: &impl PlacementOps,
    registry: &Registry,
) -> Result<RelocateTarget, RulesError> {
    let from = row.path.to_path_buf();
    let destination = to.to_path_buf();
    let contains = |protected: &std::path::Path| {
        protected.starts_with(&from) || protected.starts_with(&destination)
    };
    let refuse = || Refusal {
        path: from.clone(),
        reason: RefusalReason::Internal,
    };
    if contains(&crate::placement_fs::resolved_path(registry.home_path())?) {
        return Err(refuse().into());
    }
    for id in registry.folios()? {
        if let Some(internal) = registry.internal_placement(id)?
            && contains(&internal.path.to_path_buf())
        {
            return Err(refuse().into());
        }
    }
    let (id, mut placement) = row.to_placement();
    placement.path = destination;
    let observation = fs.read(id, &placement)?;
    let mut originals =
        registry.placements_under(&placement_path::PlacementPrefix::resolve(&from)?)?;
    if registry.placement_at(&row.path)?.is_some() {
        originals = vec![row.clone()];
    } else if originals.is_empty() && fs.read(id, &row.to_placement().1)? != Observation::Directory
    {
        originals.push(row.clone());
    }
    let mut rows = Vec::with_capacity(originals.len());
    let mut registered = false;
    let mut copies = 0;
    for old in originals {
        let mut new = old.clone();
        new.path = old.path.replace_prefix(&row.path, to);
        if let Some(existing) = registry.placement_at(&new.path)? {
            if existing == new {
                copies += 1;
            } else {
                registered = true;
            }
        }
        rows.push((old, new));
    }
    // A prefix cannot absorb rows outside this move's pending set.
    let destination_rows =
        registry.placements_under(&placement_path::PlacementPrefix::resolve(to.to_path_buf())?)?;
    registered |= destination_rows
        .iter()
        .any(|row| !rows.iter().any(|(_, new)| new == row));
    if let Some(root) = registry.placement_at(to)? {
        registered |= !rows.iter().any(|(_, new)| new == &root);
    }
    registered |= copies != 0 && copies != rows.len();
    Ok(RelocateTarget {
        path: to.clone(),
        observation,
        registered,
        pending: !rows.is_empty() && copies == rows.len(),
        rows,
    })
}
