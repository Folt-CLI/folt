mod error;
mod ops;

pub use error::PlacementError;
pub use ops::PlacementOps;

use crate::fsx::Signature;
use crate::id::ObjectId;
use crate::placement_path;
use std::path::{Path, PathBuf};

/// The operation that left a copy with a conflict state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictOperation {
    Merge,
    Rebase,
}

/// Conflict state belongs to one copy, independently of other placements of its Folio.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictState {
    pub operation: ConflictOperation,
    pub other: ObjectId,
    pub marker_lines: Vec<Vec<u8>>,
    pub pre_merge: ObjectId,
}

/// The way a Folio is presented at a placement path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementKind {
    Link,
    Copy,
    Internal,
}

impl PlacementKind {
    /// The default for a user placement is a copy; Internal is never chosen by a user.
    pub const DEFAULT: Self = Self::Copy;

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Copy => "copy",
            Self::Internal => "internal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "link" => Some(Self::Link),
            "copy" => Some(Self::Copy),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }

    /// Parses a user placement kind from an option or setting value.
    pub fn parse_user(value: &str) -> Option<Self> {
        match value {
            "link" => Some(Self::Link),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }
}

/// The in-memory placement record (`PlacementSpec`). Persistence is handled by the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementSpec {
    pub path: PathBuf,
    pub kind: PlacementKind,
    pub head: String,
    pub commit_id: Option<ObjectId>,
    pub base: Option<PlacementBase>,
}

/// Caller supplied facts about the placement path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observation {
    Missing,
    IntactLink,
    OtherLink { target: PathBuf },
    File { signature: Signature },
    Directory,
    NotAFile,
}

/// The outcome of comparing a copy with its base or observing a link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Drift {
    InSync,
    PlacementEdited,
    Gone,
    Detached,
    Unusable,
}

/// Base facts captured while creating or refreshing a placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Written {
    pub base: Option<PlacementBase>,
}

/// Content and file metadata captured for a copy placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementBase {
    pub hash: ObjectId,
    pub signature: Signature,
}

/// A blob captured from a working file for the next commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagedBlob {
    pub hash: ObjectId,
    pub size: u64,
}

/// Whether the caller must hash content before classifying drift.
pub fn needs_hash(placement: &PlacementSpec, observation: &Observation) -> bool {
    placement.kind != PlacementKind::Link
        && matches!(observation, Observation::File { signature }
            if placement.base.as_ref().is_none_or(|base| base.signature.size == signature.size))
}

/// Classifies a placement using facts and hashes supplied by the caller.
pub fn classify(
    placement: &PlacementSpec,
    observation: &Observation,
    placement_hash: Option<ObjectId>,
) -> Drift {
    match observation {
        Observation::Missing => Drift::Gone,
        Observation::Directory | Observation::NotAFile => Drift::Unusable,
        Observation::IntactLink if placement.kind == PlacementKind::Link => Drift::InSync,
        Observation::OtherLink { .. } | Observation::File { .. }
            if placement.kind == PlacementKind::Link =>
        {
            Drift::Detached
        }
        Observation::File { signature } => {
            let Some(base) = &placement.base else {
                return if placement_hash.is_some() {
                    Drift::PlacementEdited
                } else {
                    Drift::Unusable
                };
            };
            if signature.size != base.signature.size {
                return Drift::PlacementEdited;
            }
            match placement_hash {
                Some(hash) if hash == base.hash => Drift::InSync,
                Some(_) => Drift::PlacementEdited,
                None => Drift::Unusable,
            }
        }
        Observation::IntactLink | Observation::OtherLink { .. } => Drift::Unusable,
    }
}

/// Resolves existing ancestors even when a former home has been moved or removed.
pub(crate) fn resolve_existing_ancestor(input: &Path) -> Result<PathBuf, PlacementError> {
    placement_path::resolve_existing_ancestor(input).map_err(|source| {
        PlacementError::OperationFailed {
            path: input.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
mod tests;
