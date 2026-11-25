//! Pure three-way merge of content bytes, without history or file-system access.

use crate::config::limits::MERGE_BINARY_PROBE_BYTES;

mod render;
mod text;

/// The content produced by a three-way merge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeResult {
    /// All changes were combined without conflict.
    Clean(Vec<u8>),
    /// Text includes conflict markers; binary conflicts have no merged bytes.
    Conflicted {
        /// Rendered text, or `None` for binary content.
        merged: Option<Vec<u8>>,
        /// Exact conflict marker lines, including terminators, in file order.
        marker_lines: Vec<Vec<u8>>,
    },
}

/// Combine two versions relative to the supplied merge base bytes.
///
/// Whole-file equality rules run before the binary test. Otherwise a NUL in
/// any input's probe prefix produces a binary conflict. Text is merged by line,
/// preserving input terminators. Lines are diffed with Histogram, bounded by
/// input size, so equal inputs give equal results.
pub fn merge3(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeResult {
    if ours == base {
        return MergeResult::Clean(theirs.to_vec());
    }
    if theirs == base {
        return MergeResult::Clean(ours.to_vec());
    }
    if ours == theirs {
        return MergeResult::Clean(ours.to_vec());
    }
    if [base, ours, theirs].into_iter().any(is_binary) {
        MergeResult::Conflicted {
            merged: None,
            marker_lines: Vec::new(),
        }
    } else {
        text::merge(base, ours, theirs)
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(MERGE_BINARY_PROBE_BYTES).any(|b| *b == 0)
}

#[cfg(test)]
mod tests;
