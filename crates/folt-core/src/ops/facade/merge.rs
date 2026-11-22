use crate::home::{Home, HomeLock};
use crate::id::ObjectId;
use crate::ops::history::{
    CommitMetadata, HistoryError, HistoryStores, MergeOutcome, MergeRequest, abort_merge,
    continue_merge, merge_branch,
};
use crate::placement::PlacementOps;
use crate::registry::{Placement, Registry};

impl Home {
    /// Forwards to [`crate::ops::history::merge_branch`], which owns the rules.
    pub fn merge_branch(
        &self,
        row: &Placement,
        request: MergeRequest<'_>,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<MergeOutcome, HistoryError> {
        merge_branch(row, request, fs, stores, registry, lock)
    }
}

impl Home {
    /// Reads the conflict state of this placement only.
    pub fn conflict_state(
        &self,
        path: &crate::placement_path::PlacementPath,
        registry: &Registry,
    ) -> Result<Option<crate::placement::ConflictState>, crate::placement_path::RegistryError> {
        crate::ops::history::conflict_state(path, registry)
    }
}

impl Home {
    /// Forwards to the owner of restoring a copy's pre-merge file.
    pub fn abort_merge(
        &self,
        row: &Placement,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<(), HistoryError> {
        abort_merge(row, fs, stores, registry, lock)
    }

    /// Forwards to the owner of committing a staged merge resolution.
    pub fn continue_merge(
        &self,
        row: &Placement,
        metadata: CommitMetadata,
        fs: &impl PlacementOps,
        stores: &HistoryStores<'_>,
        registry: &mut Registry,
        lock: &mut HomeLock,
    ) -> Result<ObjectId, HistoryError> {
        continue_merge(row, metadata, fs, stores, registry, lock)
    }
}
