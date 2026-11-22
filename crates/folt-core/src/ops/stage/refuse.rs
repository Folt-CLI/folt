use crate::placement::ConflictState;
use crate::registry;

/// Conflict state belongs to the selected copy's row.
pub(crate) fn is_conflicted(row: &registry::Placement) -> bool {
    row.conflict.is_some()
}

/// Every recorded marker line must occur as a whole line, in recorded order.
pub(crate) fn markers_remain(state: &ConflictState, bytes: &[u8]) -> bool {
    let mut lines = bytes.split_inclusive(|byte| *byte == b'\n');
    state
        .marker_lines
        .iter()
        .all(|marker| lines.any(|line| line == marker))
}

/// A placement has a stage when its staged field is set.
pub(crate) fn is_staged(row: &registry::Placement) -> bool {
    row.staged.is_some()
}

#[cfg(test)]
mod tests;
