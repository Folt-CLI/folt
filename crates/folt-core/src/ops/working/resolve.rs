use crate::config::names::DEFAULT_BRANCH;
use crate::ops::refusal::{Refusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{Observation, PlacementKind, PlacementOps};
use crate::registry::{Placement, Registry};

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

pub(super) fn resolve_observed(
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
                    Refusal {
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
    Err(Refusal {
        path: row.path.to_path_buf(),
        reason,
    }
    .into())
}

fn require_working(row: &Placement) -> Result<(), RulesError> {
    if row.kind == PlacementKind::Link {
        return Err(Refusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::Link,
        }
        .into());
    }
    Ok(())
}

pub(crate) fn read_working(
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
    Err(Refusal {
        path: row.path.to_path_buf(),
        reason,
    }
    .into())
}
