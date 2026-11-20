use std::borrow::Cow;
use std::ops::Range;

use super::error::{FrontMatterError, Result};
use super::keys::resolved_explicit_key;
use super::scalar::{scalar, text};
use super::scan::{colon, explicit_key, flow_end, line, punctuation, top_level, trim_end};

/// A top-level node. Spans include original spelling and whitespace; `value`
/// excludes spacing after the colon and a trailing comment on its final line.
/// Spans are byte offsets into the input; all of a node's ranges lie within
/// the block body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub key: String,
    pub range: Range<usize>,
    pub key_range: Range<usize>,
    pub value: Range<usize>,
    pub comment: Range<usize>,
    pub continuation: Range<usize>,
    pub(super) ambiguous: bool,
    key_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub range: Range<usize>,
    pub body: Range<usize>,
    pub nodes: Vec<Node>,
    /// Bytes before the first key (comments and blank lines).
    pub preamble: Range<usize>,
    pub line_ending: &'static str,
}

impl Block {
    /// Physical column-zero key spellings, including those inside opaque values.
    pub(super) fn key_ranges(&self) -> impl Iterator<Item = &Range<usize>> {
        self.nodes.iter().flat_map(|node| &node.key_ranges)
    }
}

pub(super) fn build(bytes: &[u8], block: &mut Block) -> Result<()> {
    let mut pos = block.body.start;
    let mut opaque_until = pos;
    while pos < block.body.end {
        let current = line(bytes, pos, block.body.end);
        let raw = text(&bytes[pos..current.end])?;
        let top = top_level(raw);
        let key_end = top.then(|| colon(raw)).flatten();
        if pos < opaque_until {
            if let Some(end) = key_end
                && let Some(node) = block.nodes.last_mut()
            {
                node.key_ranges.push(pos..pos + end);
            }
            pos = current.next;
            continue;
        }
        if top {
            let complex = explicit_key(raw);
            if key_end.is_some() || complex.is_some() {
                if let Some(previous) = block.nodes.last_mut() {
                    previous.range.end = pos;
                    previous.continuation.end = pos;
                } else {
                    block.preamble.end = pos;
                }
                let end = key_end
                    .unwrap_or_else(|| punctuation(raw, '#').first().copied().unwrap_or(raw.len()));
                let spelling = raw[..end].trim_end();
                let key = if let Some(key) = explicit_key(spelling) {
                    scalar(key.trim())
                        .map(Cow::into_owned)
                        .unwrap_or_else(|_| key.trim().into())
                } else {
                    scalar(spelling)
                        .map(Cow::into_owned)
                        .unwrap_or_else(|_| spelling.into())
                };
                let mut value_start = pos + key_end.map_or(raw.len(), |n| n + 1);
                while value_start < current.end && matches!(bytes[value_start], b' ' | b'\t') {
                    value_start += 1;
                }
                let comments = punctuation(text(&bytes[value_start..current.end])?, '#');
                let comment = comments.first().map_or(current.end, |n| value_start + n);
                let value_end = trim_end(bytes, comment, value_start);
                block.nodes.push(Node {
                    key,
                    key_range: pos..pos + end,
                    range: pos..block.body.end,
                    value: value_start..value_end,
                    comment: comment..current.end,
                    continuation: current.next..block.body.end,
                    ambiguous: complex.is_some() || spelling.starts_with(['\'', '"']),
                    key_ranges: key_end.map(|end| pos..pos + end).into_iter().collect(),
                });
                // A flow value may wrap over several lines. Its final closing
                // brace, rather than its first line, determines the value end.
                if matches!(bytes.get(value_start), Some(b'{' | b'[')) {
                    let tail = text(&bytes[value_start..block.body.end])?;
                    if let Some(end) = flow_end(tail) {
                        let absolute = value_start + end;
                        let final_line = line(bytes, absolute, block.body.end);
                        let node = block
                            .nodes
                            .last_mut()
                            .ok_or(FrontMatterError::InvalidValue)?;
                        node.value.end = absolute;
                        node.comment = absolute..final_line.end;
                        node.continuation.start = final_line.next;
                        opaque_until = final_line.next;
                    }
                }
            }
        }
        pos = current.next;
    }
    // Continuation spans are now complete. Use each resolved explicit key for
    // all lookups, including duplicate checks and owned-key insertion order.
    // Unresolved keys retain their spelling and are refused by the key checks.
    for index in 0..block.nodes.len() {
        let n = &block.nodes[index];
        if explicit_key(text(&bytes[n.key_range.clone()])?).is_some()
            && let Ok(key) = resolved_explicit_key(bytes, n, block)
        {
            block.nodes[index].key = key;
        }
    }
    Ok(())
}
