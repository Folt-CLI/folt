use crate::home::{Home, HomeLock};
use crate::ops::prune::{
    PruneError, PruneOptions, PrunePlan, PruneReport, apply, gather, prune_placements,
};
use crate::placement::PlacementOps;
use crate::placement_path::{PlacementPath, RegistryError};
use crate::registry::Registry;

impl Home {
    /// Forwards to [`crate::ops::prune::prune_placements`], which owns the rules.
    pub fn prune_placements(
        &self,
        registry: &mut Registry,
        fs: &impl PlacementOps,
        lock: &HomeLock,
        scope: &[PlacementPath],
        options: PruneOptions,
    ) -> Result<PruneReport, PruneError> {
        prune_placements(registry, fs, lock, scope, options)
    }

    /// Forwards to [`crate::ops::prune::gather`], which owns the rules.
    pub fn gather_prune(
        &self,
        registry: &Registry,
        fs: &impl PlacementOps,
        scope: &[PlacementPath],
    ) -> Result<PrunePlan, RegistryError> {
        gather(registry, fs, scope)
    }

    /// Forwards to [`crate::ops::prune::apply`], which owns the rules.
    pub fn apply_prune(
        &self,
        plan: &PrunePlan,
        fs: &impl PlacementOps,
        registry: &mut Registry,
        lock: &HomeLock,
        confirm_mass_deletion: bool,
    ) -> Result<PruneReport, PruneError> {
        apply(plan, fs, registry, lock, confirm_mass_deletion)
    }
}
