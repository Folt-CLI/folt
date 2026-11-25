use crate::placement::ConflictState;
use crate::registry;

/// Conflict state belongs to the selected copy's row.
pub(crate) fn is_conflicted(row: &registry::Placement) -> bool {
    row.conflict.is_some()
}

/// Any whole line equal to a recorded LF-inclusive conflict marker line remains.
/// Order and multiplicity do not matter; no recorded lines means no match.
pub(crate) fn markers_remain(state: &ConflictState, bytes: &[u8]) -> bool {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .any(|line| state.marker_lines.iter().any(|marker| line == marker))
}

/// A placement has a stage when its staged field is set.
pub(crate) fn is_staged(row: &registry::Placement) -> bool {
    row.staged.is_some()
}

#[cfg(test)]
mod tests;
