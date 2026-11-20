use std::ops::Range;

use crate::config::limits::SCAN_WINDOW;
use crate::config::names::OWNED;

use super::error::{FrontMatterError, Result};
use super::keys::{node, unresolved_key_checks};
use super::locate::locate;
use super::scalar::{supported, text};
use super::scan::explicit_key;
use super::tree::Block;

mod block;
mod flow;
pub(super) mod references;
mod scalar;
mod set_reference;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: Vec<u8>,
}

/// Applies one edit without I/O. Rejects an out-of-bounds or reversed range.
pub fn apply_edit(bytes: &[u8], edit: &Edit) -> Result<Vec<u8>> {
    if edit.range.start > edit.range.end || edit.range.end > bytes.len() {
        return Err(FrontMatterError::InvalidValue);
    }
    let mut result = Vec::with_capacity(bytes.len() - edit.range.len() + edit.replacement.len());
    result.extend_from_slice(&bytes[..edit.range.start]);
    result.extend_from_slice(&edit.replacement);
    result.extend_from_slice(&bytes[edit.range.end..]);
    Ok(result)
}

fn edit_checks(bytes: &[u8], block: &Block) -> Result<()> {
    unresolved_key_checks(bytes, block)?;
    for n in &block.nodes {
        let spelling = text(&bytes[n.key_range.clone()])?;
        if explicit_key(spelling).is_some() {
            continue;
        }
        let spelling = spelling.trim();
        // Keep escaped quoted spellings opaque when editing foreign nodes.
        if spelling.starts_with(['\'', '"']) && spelling.contains('\\') {
            return Err(FrontMatterError::Ambiguous);
        }
    }
    for key in OWNED {
        if let Some(n) = node(block, key)? {
            supported(bytes, n)?;
        }
    }
    Ok(())
}
pub(super) fn edit(range: Range<usize>, replacement: impl Into<Vec<u8>>) -> Edit {
    Edit {
        range,
        replacement: replacement.into(),
    }
}
// Replacing a value must retain YAML separation even when its original span
// started immediately after the colon (for example, an empty block header).
fn value_edit(bytes: &[u8], range: Range<usize>, value: impl AsRef<[u8]>) -> Edit {
    let mut replacement = Vec::new();
    if range.start > 0 && bytes[range.start - 1] == b':' {
        replacement.push(b' ');
    }
    replacement.extend_from_slice(value.as_ref());
    if bytes.get(range.end) == Some(&b'#') {
        replacement.push(b' ');
    }
    edit(range, replacement)
}
// Validate only the bounded prefix of the virtual result, including enough
// bytes past the scan limit to finish a closing delimiter.
pub(super) fn checked_edit(bytes: &[u8], edit: Edit) -> Result<Option<Edit>> {
    let prefix: Vec<_> = bytes[..edit.range.start]
        .iter()
        .chain(&edit.replacement)
        .chain(&bytes[edit.range.end..])
        .take(SCAN_WINDOW)
        .copied()
        .collect();
    locate(&prefix)?;
    Ok(Some(edit))
}

fn insert(
    bytes: &[u8],
    block: Option<&Block>,
    key: &str,
    value: &str,
    create: bool,
) -> Result<Option<Edit>> {
    if let Some(block) = block {
        let position = block
            .nodes
            .iter()
            .rev()
            .find(|n| OWNED.contains(&n.key.as_str()))
            .map_or(block.body.end, |n| n.range.end);
        checked_edit(
            bytes,
            edit(
                position..position,
                format!("{key}: {value}{}", block.line_ending).into_bytes(),
            ),
        )
    } else if create {
        let position = if bytes.starts_with(b"\xef\xbb\xbf") {
            3
        } else {
            0
        };
        checked_edit(
            bytes,
            edit(
                position..position,
                format!("---\n{key}: {value}\n---\n").into_bytes(),
            ),
        )
    } else {
        Ok(None)
    }
}
