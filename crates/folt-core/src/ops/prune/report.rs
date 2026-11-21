use super::judge::judge;
use super::model::{
    DroppedPlacement, KeptPlacement, PruneCounts, PruneFacts, PrunePlan, PruneReport,
};
use crate::placement::PlacementKind;
use crate::registry::Placement;
use std::collections::HashSet;

impl PrunePlan {
    pub fn report(&self) -> PruneReport {
        report(&self.facts, &self.all_rows, true)
    }
}

pub(super) fn report(facts: &[PruneFacts], all_rows: &[Placement], dry_run: bool) -> PruneReport {
    let mut kept = Vec::new();
    let mut dropped_rows = Vec::new();
    let mut warnings = Vec::new();
    for facts in facts {
        if let Some(reason) = judge(facts) {
            if reason != "in sync"
                && reason != "matches stage"
                && facts.row.kind != PlacementKind::Internal
            {
                warnings.push(format!(
                    "{}: {reason}",
                    facts.row.path.to_path_buf().display()
                ));
            }
            kept.push(KeptPlacement {
                path: facts.row.path.clone(),
                kind: facts.row.kind,
                reason,
            });
        } else {
            dropped_rows.push(&facts.row);
        }
    }
    let paths: HashSet<_> = dropped_rows.iter().map(|row| &row.path).collect();
    let retained: HashSet<_> = all_rows
        .iter()
        .filter(|row| !paths.contains(&row.path))
        .filter_map(|row| row.staged.map(|stage| (row.id, stage.hash)))
        .collect();
    let mut seen = HashSet::new();
    let unreachable: Vec<_> = dropped_rows
        .iter()
        .filter_map(|row| row.staged.map(|stage| (row.id, stage.hash)))
        .filter(|stage| !retained.contains(stage))
        .filter(|stage| seen.insert(*stage))
        .collect();
    let dropped: Vec<_> = dropped_rows
        .iter()
        .map(|row| DroppedPlacement {
            path: row.path.clone(),
            kind: row.kind,
            stage: row.staged.map(|stage| stage.hash),
            stage_unreachable: row.kind == PlacementKind::Copy
                && row
                    .staged
                    .is_some_and(|stage| seen.contains(&(row.id, stage.hash))),
        })
        .collect();
    if !dropped.is_empty() {
        warnings
            .push("a dropped row's file may still exist elsewhere; rebind is out of scope".into());
    }
    PruneReport {
        counts: PruneCounts {
            in_scope: facts.len(),
            dropped: dropped.len(),
            kept: kept.len(),
        },
        dropped,
        kept,
        unreachable_staged_blobs: unreachable,
        warnings,
        dry_run,
    }
}
