use crate::folio::name::FolioName;
use crate::home::{Home, HomeLock};
use crate::id::{FolioId, ObjectId};
use crate::ops::history::{CommitMetadata, HistoryStores};
use crate::ops::rename::{RenameError, RenamePlan, apply_rename, plan_rename};
use crate::placement_fs::PlacementFs;
use crate::placement_path::RegistryError;
use crate::registry::Registry;

impl Home {
    /// Forwards to [`crate::ops::rename::plan_rename`], which owns the rules.
    pub fn plan_rename(
        &self,
        old: &FolioName,
        new: &FolioName,
        fs: &PlacementFs<'_>,
        stores: &HistoryStores<'_>,
        registry: &Registry,
    ) -> Result<RenamePlan, RenameError> {
        plan_rename(old, new, fs, stores, registry)
    }

    /// Forwards to [`crate::ops::rename::apply_rename`], which owns the rules.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_rename(
        &self,
        plan: &RenamePlan,
        metadata: CommitMetadata,
        fs: &PlacementFs<'_>,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
        is_live: &mut dyn FnMut(FolioId, &str) -> Result<bool, RegistryError>,
    ) -> Result<Option<ObjectId>, RenameError> {
        apply_rename(plan, metadata, fs, stores, registry, lock, is_live)
    }
}
