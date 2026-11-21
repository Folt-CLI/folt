use crate::history::RefStore;
use crate::home::{Home, HomeLock};
use crate::ops::checkout::{CheckoutError, CheckoutStores, checkout_placement, resolve};
use crate::ops::placement::move_::MoveTarget;
use crate::placement::PlacementOps;
use crate::ref_name::RefName;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;

impl Home {
    /// Forwards to [`crate::ops::checkout::resolve`], which owns the rules.
    pub fn resolve_checkout_target(
        &self,
        refs: &RefStore,
        objects: &ObjectStore,
        branch: &RefName,
        object_limit: u64,
    ) -> Result<MoveTarget, CheckoutError> {
        resolve(refs, objects, branch, object_limit)
    }

    /// Forwards to [`crate::ops::checkout::checkout_placement`], which owns the rules.
    #[allow(clippy::too_many_arguments)]
    pub fn checkout_placement(
        &self,
        row: &Placement,
        branch: &RefName,
        discard: bool,
        fs: &impl PlacementOps,
        stores: &CheckoutStores<'_>,
        registry: &mut Registry,
        lock: &HomeLock,
    ) -> Result<(), CheckoutError> {
        checkout_placement(row, branch, discard, fs, stores, registry, lock)
    }
}
