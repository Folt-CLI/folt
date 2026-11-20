use std::borrow::Borrow;

use crate::id::FolioId;
use crate::ops::facts::{self, Behind, PlacementFacts};
use crate::ops::rules_error::RulesError;
use crate::placement::{self, Drift, Observation, PlacementKind, PlacementOps};
use crate::refs::{RefError, RefStore};
use crate::registry::{self, Registry};

use crate::ops::working::{self, judge::judge_working, resolve::resolve_working};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementStatus {
    pub placement: registry::Placement,
    pub drift: Drift,
    /// Links always show `folio.main` and have no recorded-commit comparison.
    pub behind: Option<Behind>,
    pub staged: working::Staged,
    pub edit: working::Edit,
}

/// Include internal-row facts to report an intact link's shared edit and stage.
pub fn judge_status(facts_set: &[PlacementFacts]) -> Vec<PlacementStatus> {
    facts_set
        .iter()
        .map(|facts| {
            let (_, placement) = facts.placement.to_placement();
            let working_facts = if placement.kind == PlacementKind::Link
                && facts.observation == Observation::IntactLink
            {
                facts_set
                    .iter()
                    .find(|internal| {
                        internal.placement.id == facts.placement.id
                            && internal.placement.kind == PlacementKind::Internal
                    })
                    .unwrap_or(facts)
            } else {
                facts
            };
            let working = judge_working(working_facts);
            let behind = (placement.kind != PlacementKind::Link).then_some(working.behind);
            PlacementStatus {
                placement: facts.placement.clone(),
                drift: placement::classify(&placement, &facts.observation, facts.placement_hash),
                behind,
                staged: working.staged,
                edit: working.edit,
            }
        })
        .collect()
}

/// Collects status for the selected paths, including each intact link's shared edits.
pub fn gather_status<R: Borrow<RefStore>>(
    placements: &[registry::Placement],
    fs: &impl PlacementOps,
    mut open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
    registry: &Registry,
) -> Result<Vec<PlacementStatus>, RulesError> {
    let mut facts = facts::gather(placements, fs, &mut open_refs)?;
    for row in placements
        .iter()
        .filter(|row| row.kind == PlacementKind::Link)
    {
        let internal = resolve_working(row, registry, fs)?;
        let refs = open_refs(row.id)?;
        facts.push(working::gather(&internal, fs, refs.borrow(), registry)?);
    }
    let mut status = judge_status(&facts);
    status.truncate(placements.len());
    Ok(status)
}

#[cfg(test)]
mod tests;
