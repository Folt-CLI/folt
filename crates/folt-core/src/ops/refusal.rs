use std::error::Error;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefusalReason {
    DestinationExists,
    DestinationRegistered,
    InsideSource,
    MixedHomes,
    Internal,
    MissingInternal,
    LinkReplaced,
    SameKind,
    CopyBranch,
    ConvertGone,
    OtherLink,
    Staged,
    InternalEdited,
    Link,
    InternalBranch,
    Edited,
    Gone,
    NotAFile,
    Unusable,
    PlacementChanged,
    RegistryChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementRefusal {
    pub path: PathBuf,
    pub reason: RefusalReason,
}

impl fmt::Display for PlacementRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.reason {
            RefusalReason::MissingInternal => "missing internal working-file row",
            RefusalReason::LinkReplaced => {
                "link was replaced; use the kind-change command (#33) to switch kind on purpose"
            }
            RefusalReason::Internal => "internal placements cannot be changed by this operation",
            RefusalReason::DestinationExists => "destination already exists",
            RefusalReason::DestinationRegistered => "destination is registered",
            RefusalReason::InsideSource => "destination is inside the source",
            RefusalReason::MixedHomes => "internal paths do not share the expected former home",
            RefusalReason::SameKind => "placement already has the requested kind",
            RefusalReason::CopyBranch => {
                "conversion requires main; copies on other branches are tracked in #73"
            }
            RefusalReason::ConvertGone => {
                "placement is missing; move/remove/repair is tracked in #38"
            }
            RefusalReason::OtherLink => {
                "placement points elsewhere; use the kind-change command (#33) to retarget on purpose"
            }
            RefusalReason::Staged => {
                "copy has a recorded stage; commit or checkout main with discard first"
            }
            RefusalReason::InternalEdited => {
                "internal working file holds edits; commit or checkout main with discard first (#119)"
            }
            RefusalReason::Link => "links always show folio.main",
            RefusalReason::InternalBranch => {
                "internal working files must stay on the default branch"
            }
            RefusalReason::Edited => "copy has edits; discard is required to lose them",
            RefusalReason::Gone => "placement is missing",
            RefusalReason::NotAFile => "placement is not a regular file",
            RefusalReason::Unusable => "placement is unusable",
            RefusalReason::PlacementChanged => "placement changed since the move was judged",
            RefusalReason::RegistryChanged => "registry row changed since the move was judged",
        };
        write!(
            f,
            "cannot change placement {}: {reason}",
            self.path.display()
        )
    }
}

impl Error for PlacementRefusal {}
