use crate::id::ObjectId;
use crate::ops::refusal as placement_refusal;
use std::{error::Error, fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryRefusal {
    NothingStaged,
    BranchMoved {
        tip: Option<ObjectId>,
        ahead: Vec<PathBuf>,
    },
    InternalEdited(PathBuf),
    MissingInternal,
    MissingBranch,
    Link(PathBuf),
    Unusable(PathBuf),
    RegistryChanged(PathBuf),
    PlacementChanged(PathBuf),
    PlacementsChanged,
    BranchInUse(Vec<PathBuf>),
    InvalidFacts,
    RebaseConflict(ObjectId),
    RebaseRoot(ObjectId),
    MergeConflict,
    MergeCrissCross,
    MergeAlreadyMerged,
    MergeNoBase,
    MergeEdited(PathBuf),
    MergeInvalid(String),
    TagMismatch,
    TagExists,
    Move(placement_refusal::PlacementRefusal),
}

impl From<placement_refusal::PlacementRefusal> for HistoryRefusal {
    fn from(error: placement_refusal::PlacementRefusal) -> Self {
        Self::Move(error)
    }
}

impl fmt::Display for HistoryRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingStaged => write!(f, "nothing staged"),
            Self::BranchMoved { tip, ahead } => {
                write!(f, "branch moved to {tip:?}")?;
                for path in ahead {
                    write!(f, "; ahead at {}", path.display())?;
                }
                Ok(())
            }
            Self::InternalEdited(path) => write!(
                f,
                "folio.main holds edits or a stage: {}; commit through a link or the internal row, or checkout main with discard",
                path.display()
            ),
            Self::MissingInternal => write!(f, "missing internal working-file row"),
            Self::MissingBranch => write!(f, "branch has no tip"),
            Self::Link(path) => write!(f, "cannot commit a link: {}", path.display()),
            Self::Unusable(path) => write!(f, "working file is unusable: {}", path.display()),
            Self::RegistryChanged(path) => write!(f, "registry row changed: {}", path.display()),
            Self::PlacementChanged(path) => write!(f, "working file changed: {}", path.display()),
            Self::PlacementsChanged => {
                write!(f, "branch placements changed since reset was judged")
            }
            Self::BranchInUse(paths) => write!(f, "branch is used by {} placement(s)", paths.len()),
            Self::InvalidFacts => write!(f, "inconsistent history facts"),
            Self::RebaseConflict(id) => write!(f, "replay conflicted at commit {id}"),
            Self::RebaseRoot(id) => write!(f, "cannot replay parentless commit {id}"),
            Self::MergeConflict => write!(f, "merge content conflicts"),
            Self::MergeCrissCross => write!(f, "merge has several merge bases"),
            Self::MergeAlreadyMerged => write!(f, "branch is already merged"),
            Self::MergeNoBase => write!(f, "branches have no merge base"),
            Self::MergeEdited(path) => write!(
                f,
                "merge requires a clean file without a stage: {}",
                path.display()
            ),
            Self::MergeInvalid(message) => write!(f, "merged content is invalid: {message}"),
            Self::TagMismatch => write!(f, "annotation name or target differs from the tag ref"),
            Self::TagExists => write!(f, "tag already exists"),
            Self::Move(error) => error.fmt(f),
        }
    }
}
impl Error for HistoryRefusal {}
