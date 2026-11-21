use std::collections::HashMap;

use crate::placement_path::{PlacementPath, path_bytes};

use super::super::destination::{destination, is_marker};
use super::super::refusal::{RegisterRefusalReason, refuse_name};
use super::super::report::report;
use super::{RegisterPlan, RegisterStatus};

pub(in crate::ops::register) fn block_changed(
    previous: &RegisterPlan,
    mut current: RegisterPlan,
    batch_total_bytes: u64,
) -> RegisterPlan {
    let mut previous_by_destination = HashMap::<_, Vec<_>>::new();
    for entry in &previous.entries {
        previous_by_destination
            .entry(PlacementPath::from_stored(path_bytes(destination(entry))))
            .or_default()
            .push(entry);
    }
    for entry in &mut current.entries {
        let destination = PlacementPath::from_stored(path_bytes(destination(entry)));
        let request_moved = previous
            .destinations
            .iter()
            .zip(&current.destinations)
            .any(|(old, new)| new == &destination && old != new);
        if request_moved {
            let path = entry.path.clone();
            refuse_name(entry, RegisterRefusalReason::DestinationChanged { path });
            continue;
        }
        let old = previous_by_destination
            .get(&destination)
            .and_then(|candidates| {
                // Distinct case-variant marker files retain their full path identities.
                // Accept a changed marker spelling only for an unambiguous destination.
                candidates
                    .iter()
                    .copied()
                    .find(|old| old.path == entry.path)
                    .or(match candidates.as_slice() {
                        [old] if is_marker(&old.path) && is_marker(&entry.path) => Some(*old),
                        _ => None,
                    })
            });
        let Some(old) = old else {
            let path = entry.path.clone();
            refuse_name(entry, RegisterRefusalReason::DestinationChanged { path });
            continue;
        };
        if matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) && old.prefix.is_some()
            && old.signature.is_some()
            && (old.signature != entry.signature || old.prefix != entry.prefix)
        {
            refuse_name(entry, RegisterRefusalReason::FileChanged);
        }
    }
    report(
        current.entries,
        current.requests,
        current.destinations,
        batch_total_bytes,
    )
}
