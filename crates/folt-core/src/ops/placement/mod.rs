use crate::ops::facts::PlacementFacts;
use crate::ops::refusal::{PlacementRefusal, RefusalReason};
use crate::ops::rules_error::RulesError;
use crate::placement::{Observation, PlacementOps};
use crate::registry::{self, Registry};

pub(super) fn check_row(row: &registry::Placement, registry: &Registry) -> Result<(), RulesError> {
    if registry.placement_at(&row.path)?.as_ref() != Some(row) {
        return Err(PlacementRefusal {
            path: row.path.to_path_buf(),
            reason: RefusalReason::RegistryChanged,
        }
        .into());
    }
    Ok(())
}

pub(super) fn check_file(facts: &PlacementFacts, fs: &impl PlacementOps) -> Result<(), RulesError> {
    let (id, placement) = facts.placement.to_placement();
    if fs.read(id, &placement)? != facts.observation
        || (facts.placement_hash.is_some()
            && matches!(facts.observation, Observation::File { .. })
            && Some(fs.hash_placement(&placement)?) != facts.placement_hash)
    {
        return Err(PlacementRefusal {
            path: placement.path,
            reason: RefusalReason::PlacementChanged,
        }
        .into());
    }
    Ok(())
}

pub mod convert;
pub mod move_;
pub mod rehome;
pub mod relocate;
pub mod remove;
pub mod repair;
