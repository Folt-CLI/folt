use crate::id::{FolioId, ObjectId};
use crate::placement::{Observation, PlacementKind};
use crate::placement_path::{PlacementPath, RegistryError};
use crate::registry::Placement;
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PruneOptions {
    pub apply: bool,
    pub confirm_mass_deletion: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneFacts {
    pub row: Placement,
    pub observation: Result<Observation, String>,
    pub parent_exists: Result<bool, String>,
    pub hash: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DroppedPlacement {
    pub path: PlacementPath,
    pub kind: PlacementKind,
    /// The staged object held by this copy, if any.
    pub stage: Option<ObjectId>,
    /// A copy's stage has no reference in the surviving row set.
    pub stage_unreachable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeptPlacement {
    pub path: PlacementPath,
    pub kind: PlacementKind,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PruneCounts {
    pub in_scope: usize,
    pub dropped: usize,
    pub kept: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneReport {
    /// In a dry run these are the rows that would be dropped.
    pub dropped: Vec<DroppedPlacement>,
    pub kept: Vec<KeptPlacement>,
    /// Distinct (Folio, blob) stages held only by dropped rows, including twins.
    pub unreachable_staged_blobs: Vec<(FolioId, ObjectId)>,
    pub warnings: Vec<String>,
    pub counts: PruneCounts,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct PrunePlan {
    pub(super) facts: Vec<PruneFacts>,
    pub(super) all_rows: Vec<Placement>,
    pub(super) scope: Vec<PlacementPath>,
}

#[derive(Debug)]
pub enum PruneError {
    Registry(RegistryError),
    ConfirmationRequired { dropped: usize, in_scope: usize },
}

impl fmt::Display for PruneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(f),
            Self::ConfirmationRequired { dropped, in_scope } => write!(
                f,
                "prune would drop {dropped} of {in_scope} rows in scope; explicit mass-deletion confirmation is required"
            ),
        }
    }
}

impl Error for PruneError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::ConfirmationRequired { .. } => None,
        }
    }
}

impl From<RegistryError> for PruneError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}
