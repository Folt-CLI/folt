use crate::history::RefStore;
use crate::home::{Home, HomeLock};
use crate::ops::rules_error::RulesError;
use crate::ops::stage::{stage, unstage};
use crate::placement::PlacementOps;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

impl Home {
    /// Forwards to [`crate::ops::stage::stage`], which owns the rules.
    pub fn stage(
        &self,
        row: &Placement,
        fs: &impl PlacementOps,
        objects: &ObjectStore,
        refs: &RefStore,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        stage(row, fs, objects, refs, registry, lock)
    }

    /// Forwards to [`crate::ops::stage::unstage`], which owns the rules.
    pub fn unstage(
        &self,
        row: &Placement,
        fs: &impl PlacementOps,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), RulesError> {
        unstage(row, fs, registry, lock)
    }
}
