use super::super::{HistoryError, HistoryStores, Refusal, gather_file};
use crate::config::names::DEFAULT_BRANCH;
use crate::object::Commit;
use crate::ops::facts::PlacementFacts;
use crate::ops::working::gather;
use crate::placement::{PlacementKind, PlacementOps};
use crate::registry::{Placement, Registry};

/// Facts for one commit, including the placements that can identify an advanced tip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitFacts {
    pub working: PlacementFacts,
    pub link: Option<Placement>,
    pub internal: Option<PlacementFacts>,
    pub placements: Vec<Placement>,
    pub tip_commit: Option<Commit>,
}

/// The home lock must cover gathering, judging and applying history operations.
/// `object_limit` bounds commit reads independently of blob size.
pub fn gather_commit(
    row: &Placement,
    fs: &impl PlacementOps,
    stores: &HistoryStores<'_>,
    registry: &Registry,
) -> Result<CommitFacts, HistoryError> {
    let HistoryStores {
        objects,
        refs,
        object_limit,
    } = *stores;
    let link = (row.kind == PlacementKind::Link).then(|| row.clone());
    let working = gather(row, fs, refs, registry)?;
    let row = &working.placement;
    let placements = registry.placements_of_head(row.id, &row.head)?;
    let internal = if row.kind == PlacementKind::Copy && row.head == DEFAULT_BRANCH {
        let internal = placements
            .iter()
            .find(|row| row.kind == PlacementKind::Internal)
            .ok_or(Refusal::MissingInternal)?;
        Some(gather_file(internal, fs, refs)?)
    } else {
        None
    };
    let tip_commit = working
        .tip
        .map(|id| objects.get_commit(id, object_limit))
        .transpose()?;
    Ok(CommitFacts {
        working,
        link,
        internal,
        placements,
        tip_commit,
    })
}
