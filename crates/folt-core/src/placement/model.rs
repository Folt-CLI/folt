use super::PlacementKind;
use crate::fsx::Signature;
use crate::id::ObjectId;
use std::path::PathBuf;

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
