use crate::ops::facts::{Behind, PlacementFacts, needs_hash};
use crate::ops::rules_error::RulesError;
use crate::placement::PlacementOps;
use crate::ref_name::{RefKind, RefName};
use crate::refs::{RefError, RefStore};
use crate::registry::{Placement, Registry};

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

/// Collects one working file, hashing when it has no base or a recorded size matches.
pub fn gather(
    row: &Placement,
    fs: &impl PlacementOps,
    refs: &RefStore,
    registry: &Registry,
) -> Result<PlacementFacts, RulesError> {
    let row = &resolve::resolve_working(row, registry, fs)?;
    let observation = resolve::read_working(row, fs)?;
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

pub mod judge;
pub mod resolve;

#[cfg(test)]
mod tests;
