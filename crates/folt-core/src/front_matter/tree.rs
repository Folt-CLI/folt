use std::borrow::Cow;
use std::ops::Range;

use super::scan::{colon, explicit_key, flow_end, line, punctuation, top_level, trim_end};
use super::{FrontMatterError, Result, mapping, meaningful, resolved_explicit_key, scalar, text};

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

/// One mapping entry, retaining source spans independently of its spelling.
pub(super) struct Entry<'a> {
    pub(super) range: Range<usize>,
    pub(super) key_range: Range<usize>,
    pub(super) value: Range<usize>,
    pub(super) fields: Option<Vec<(&'a str, &'a str)>>,
}

pub(super) struct Entries<'a> {
    bytes: &'a [u8],
    pos: usize,
    end: usize,
    indent: Option<usize>,
    flow: Option<std::vec::IntoIter<Entry<'a>>>,
}

/// Visits block and flow entries in source order, without decoding their fields.
pub(super) fn entries<'a>(bytes: &'a [u8], n: &Node) -> Result<Entries<'a>> {
    let value = text(&bytes[n.value.clone()])?;
    let flow = if value.starts_with('{') {
        let suffix = text(&bytes[n.comment.clone()])?.trim();
        if (!suffix.is_empty() && !suffix.starts_with('#'))
            || text(&bytes[n.continuation.clone()])?
                .lines()
                .any(meaningful)
        {
            return Err(FrontMatterError::Unsupported);
        }
        let mut entries = Vec::new();
        for (key, value) in mapping(value)? {
            let key_range = super::scan::span(bytes, key);
            let value = super::scan::span(bytes, value);
            entries.push(Entry {
                range: key_range.start..value.end,
                key_range,
                value,
                fields: None,
            });
        }
        Some(entries.into_iter())
    } else {
        if !value.is_empty() {
            return Err(FrontMatterError::Unsupported);
        }
        None
    };
    Ok(Entries {
        bytes,
        pos: n.continuation.start,
        end: n.range.end,
        indent: None,
        flow,
    })
}

impl<'a> Iterator for Entries<'a> {
    type Item = Result<Entry<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(flow) = &mut self.flow {
            return flow.next().map(Ok);
        }
        while self.pos < self.end {
            let current = line(self.bytes, self.pos, self.end);
            match text(&self.bytes[self.pos..current.end]) {
                Ok(raw) if !meaningful(raw) => self.pos = current.next,
                Err(error) => {
                    self.pos = self.end;
                    return Some(Err(error));
                }
                Ok(_) => {
                    let entry = self.block_entry();
                    if entry.is_err() {
                        self.pos = self.end;
                    }
                    return Some(entry);
                }
            }
        }
        None
    }
}

impl<'a> Entries<'a> {
    fn block_entry(&mut self) -> Result<Entry<'a>> {
        let bytes = self.bytes;
        let pos = self.pos;
        let current = line(bytes, pos, self.end);
        let raw = text(&bytes[pos..current.end])?;
        let spaces = raw.len() - raw.trim_start_matches(' ').len();
        if spaces == 0 || *self.indent.get_or_insert(spaces) != spaces {
            return Err(FrontMatterError::Unsupported);
        }
        let clean = &raw[spaces..];
        let c = colon(clean).ok_or(FrontMatterError::InvalidValue)?;
        let key_range = super::scan::span(bytes, clean[..c].trim());
        let start = pos + spaces + c + 1;
        let start = start
            + bytes[start..current.end]
                .iter()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
        let comment = punctuation(text(&bytes[start..current.end])?, '#')
            .first()
            .map_or(current.end, |i| start + i);
        let mut end = trim_end(bytes, comment, start);
        self.pos = current.next;
        let mut entry_end = self.pos;
        if bytes.get(start) == Some(&b'{') {
            end = start
                + flow_end(text(&bytes[start..self.end])?).ok_or(FrontMatterError::InvalidValue)?;
            let final_line = line(bytes, end, self.end);
            let suffix = text(&bytes[end..final_line.end])?.trim();
            if !suffix.is_empty() && !suffix.starts_with('#') {
                return Err(FrontMatterError::Unsupported);
            }
            self.pos = final_line.next;
            entry_end = self.pos;
        }
        let fields = if start == end {
            let (fields, last) = self.nested_fields(spaces)?;
            entry_end = entry_end.max(last);
            Some(fields)
        } else {
            None
        };
        Ok(Entry {
            range: pos..entry_end,
            key_range,
            value: start..end,
            fields,
        })
    }

    fn nested_fields(&mut self, spaces: usize) -> Result<(Vec<(&'a str, &'a str)>, usize)> {
        let mut fields = Vec::new();
        let mut indent = None;
        let mut end = self.pos;
        while self.pos < self.end {
            let child = line(self.bytes, self.pos, self.end);
            let raw = text(&self.bytes[self.pos..child.end])?;
            if !meaningful(raw) {
                self.pos = child.next;
                continue;
            }
            let spaces_here = raw.len() - raw.trim_start_matches(' ').len();
            if spaces_here <= spaces {
                break;
            }
            if *indent.get_or_insert(spaces_here) != spaces_here {
                return Err(FrontMatterError::Unsupported);
            }
            let clean = raw.trim();
            let c = colon(clean).ok_or(FrontMatterError::InvalidValue)?;
            let value = clean[c + 1..].trim();
            let comment = punctuation(value, '#')
                .first()
                .copied()
                .unwrap_or(value.len());
            fields.push((clean[..c].trim(), value[..comment].trim()));
            self.pos = child.next;
            end = self.pos;
        }
        Ok((fields, end))
    }
}
