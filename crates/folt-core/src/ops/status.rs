use std::borrow::Borrow;

use crate::id::FolioId;
use crate::ops::facts::{self, Behind, PlacementFacts};
use crate::ops::rules_error::RulesError;
use crate::placement::{self, Drift, Observation, PlacementKind, PlacementOps};
use crate::refs::{RefError, RefStore};
use crate::registry::{self, Registry};

use crate::id::ObjectId;
use crate::ops::working::{self, resolve_working};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edit {
    Clean,
    Edited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Staged {
    None,
    Staged,
    StagedThenModified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingStatus {
    pub edit: Edit,
    pub staged: Staged,
    pub behind: Behind,
}

/// Compares only the supplied observations; no commits or files are read.
pub fn judge_working(facts: &PlacementFacts) -> WorkingStatus {
    let row = &facts.placement;
    let matches = |hash: ObjectId, size: u64| {
        matches!(&facts.observation, Observation::File { signature } if signature.size == size)
            && facts.placement_hash == Some(hash)
    };
    let edit = if row
        .base
        .as_ref()
        .is_some_and(|base| matches(base.hash, base.signature.size))
    {
        Edit::Clean
    } else {
        Edit::Edited
    };
    let staged = match row.staged {
        None => Staged::None,
        Some(staged) if matches(staged.hash, staged.size) => Staged::Staged,
        Some(_) => Staged::StagedThenModified,
    };
    let behind = match (row.commit_id, facts.tip) {
        (None, None) => Behind::NoHistory,
        (None, Some(_)) => Behind::Unrecorded,
        (Some(_), None) => Behind::MissingTip,
        (Some(commit), Some(tip)) if commit == tip => Behind::UpToDate,
        (Some(_), Some(tip)) => Behind::Behind { tip },
    };
    WorkingStatus {
        edit,
        staged,
        behind,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementStatus {
    pub placement: registry::Placement,
    pub drift: Drift,
    /// Links always show `folio.main` and have no recorded-commit comparison.
    pub behind: Option<Behind>,
    pub staged: Staged,
    pub edit: Edit,
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
