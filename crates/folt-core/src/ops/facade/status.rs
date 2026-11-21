use std::borrow::Borrow;

use crate::history::{RefError, RefStore};
use crate::home::Home;
use crate::id::FolioId;
use crate::ops::facts::{PlacementFacts, gather};
use crate::ops::rules_error::RulesError;
use crate::ops::status::{
    PlacementStatus, WorkingStatus, gather_status, judge_status, judge_working,
};
use crate::ops::working;
use crate::placement::PlacementOps;
use crate::registry::{Placement, Registry};

impl Home {
    /// Forwards to [`crate::ops::facts::gather`], which owns the rules.
    pub fn gather_facts<R: Borrow<RefStore>>(
        &self,
        placements: &[Placement],
        fs: &impl PlacementOps,
        open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
    ) -> Result<Vec<PlacementFacts>, RulesError> {
        gather(placements, fs, open_refs)
    }

    /// Forwards to [`crate::ops::working::gather`], which owns the rules.
    pub fn gather_working(
        &self,
        row: &Placement,
        fs: &impl PlacementOps,
        refs: &RefStore,
        registry: &Registry,
    ) -> Result<PlacementFacts, RulesError> {
        working::gather(row, fs, refs, registry)
    }

    /// Forwards to [`crate::ops::status::gather_status`], which owns the rules.
    pub fn gather_status<R: Borrow<RefStore>>(
        &self,
        placements: &[Placement],
        fs: &impl PlacementOps,
        open_refs: impl FnMut(FolioId) -> Result<R, RefError>,
        registry: &Registry,
    ) -> Result<Vec<PlacementStatus>, RulesError> {
        gather_status(placements, fs, open_refs, registry)
    }

    /// Forwards to [`crate::ops::status::judge_status`], which owns the rules.
    pub fn judge_status(&self, facts_set: &[PlacementFacts]) -> Vec<PlacementStatus> {
        judge_status(facts_set)
    }

    /// Forwards to [`crate::ops::status::judge_working`], which owns the rules.
    pub fn judge_working(&self, facts: &PlacementFacts) -> WorkingStatus {
        judge_working(facts)
    }
}
