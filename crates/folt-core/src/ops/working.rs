use crate::config::names::DEFAULT_BRANCH;
use crate::ops::facts::{PlacementFacts, needs_hash};
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{Observation, PlacementKind, PlacementOps};
use crate::ref_name::{RefKind, RefName};
use crate::refs::{RefError, RefStore};
use crate::registry::{Placement, Registry};

/// Collects one working file, hashing when it has no base or a recorded size matches.
pub fn gather(
    row: &Placement,
    fs: &impl PlacementOps,
    refs: &RefStore,
    registry: &Registry,
) -> Result<PlacementFacts, RulesError> {
    let row = &resolve_working(row, registry, fs)?;
    let observation = read_working(row, fs)?;
    let placement_hash = if needs_hash(row, &observation) {
        Some(fs.hash_placement(&row.to_placement().1)?)
    } else {
        None
    };
    let tip = refs.read(
        RefKind::Branch,
        &RefName::parse(&row.head).map_err(RefError::from)?,
    )?;
    Ok(PlacementFacts {
        placement: row.clone(),
        observation,
        placement_hash,
        tip,
    })
}

/// Links share the internal working row, provided their entry still points to it.
pub fn resolve_working(
    row: &Placement,
    registry: &Registry,
    fs: &impl PlacementOps,
) -> Result<Placement, RulesError> {
    if row.kind != PlacementKind::Link {
        return Ok(row.clone());
    }
    let observation = fs.read(row.id, &row.to_placement().1)?;
    let internal = registry.internal_placement(row.id)?;
    resolve_observed(row, &observation, internal)
}

fn resolve_observed(
    row: &Placement,
    observation: &Observation,
    internal: Option<Placement>,
) -> Result<Placement, RulesError> {
    if row.kind != PlacementKind::Link {
        return Ok(row.clone());
    }
    let reason = match observation {
        Observation::IntactLink => {
            return internal
                .filter(|internal| {
                    internal.id == row.id
                        && internal.kind == PlacementKind::Internal
                        && internal.head == DEFAULT_BRANCH
                })
                .ok_or_else(|| {
                    PlacementRefusal {
                        path: row.path.to_path_buf(),
                        reason: RefusalReason::MissingInternal,
                    }
                    .into()
                });
        }
        Observation::Missing => RefusalReason::Gone,
        Observation::Directory | Observation::NotAFile => RefusalReason::NotAFile,
        Observation::File { .. } | Observation::OtherLink { .. } => RefusalReason::LinkReplaced,
    };
    Err(PlacementRefusal {
        path: row.path.to_path_buf(),
        reason,
    }
    .into())
}

fn require_working(row: &Placement) -> Result<(), RulesError> {
    if row.kind == PlacementKind::Link {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::Link,
        }
        .into());
    }
    Ok(())
}

pub(super) fn read_working(
    row: &Placement,
    fs: &impl PlacementOps,
) -> Result<Observation, RulesError> {
    require_working(row)?;
    let observation = fs.read(row.id, &row.to_placement().1)?;
    let reason = match observation {
        Observation::File { .. } => return Ok(observation),
        Observation::Missing => RefusalReason::Gone,
        Observation::Directory | Observation::NotAFile => RefusalReason::NotAFile,
        Observation::IntactLink | Observation::OtherLink { .. } => RefusalReason::Unusable,
    };
    Err(PlacementRefusal {
        path: row.path.to_path_buf(),
        reason,
    }
    .into())
}

#[cfg(test)]
mod tests;
