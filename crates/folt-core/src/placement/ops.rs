use super::{
    Drift, Observation, PlacementError, PlacementKind, PlacementSpec, Written, classify, needs_hash,
};
use crate::id::ObjectId;
use crate::store::ObjectStore;
use std::path::PathBuf;

/// PlacementKind-specific placement operations. Implementations perform filesystem work.
pub trait PlacementOps {
    /// Returns the canonical derived link target spelling used by create and read.
    fn link_target(&self, id: crate::id::FolioId) -> PathBuf;

    /// Creates a placement; copies read the supplied commit content blob.
    fn create(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
    ) -> Result<Written, PlacementError>;

    /// Reads facts about a placement path.
    fn read(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
    ) -> Result<Observation, PlacementError>;

    /// Refreshes a placement; copies read the supplied commit content blob.
    /// When supplied, copies recheck the expected observation before replacement.
    fn refresh(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
        objects: &ObjectStore,
        content: ObjectId,
        expected: Option<&Observation>,
    ) -> Result<Written, PlacementError>;

    /// Syncs the parent directory before recording a previously published conversion.
    fn sync_parent(&self, placement: &PlacementSpec) -> Result<(), PlacementError>;

    /// Replaces an existing entry with the requested kind after rechecking its facts.
    fn convert(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
        to_kind: PlacementKind,
        objects: &ObjectStore,
        content: ObjectId,
        expected: &Observation,
    ) -> Result<Written, PlacementError>;

    /// Rewrites an intact relative link to its canonical absolute target.
    /// Absolute links and entries that are not intact links are left untouched.
    fn normalize_link(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
    ) -> Result<(), PlacementError>;

    /// Moves an entry without replacing an existing destination.
    fn relocate(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
        to: &std::path::Path,
        expected: &Observation,
    ) -> Result<Written, PlacementError>;

    /// Removes an entry after rechecking its observation.
    fn unlink(
        &self,
        id: crate::id::FolioId,
        placement: &PlacementSpec,
        expected: &Observation,
    ) -> Result<(), PlacementError>;

    /// Hashes a placement file without retaining its contents.
    fn hash_placement(&self, placement: &PlacementSpec) -> Result<ObjectId, PlacementError>;

    /// Uses supplied facts to detect drift, requesting hashes when needed.
    fn detect_drift(
        &self,
        placement: &PlacementSpec,
        observation: &Observation,
        placement_hash: Option<ObjectId>,
    ) -> Result<Drift, PlacementError> {
        if needs_hash(placement, observation) && placement_hash.is_none() {
            return Err(PlacementError::HashRequired);
        }
        Ok(classify(placement, observation, placement_hash))
    }
}
