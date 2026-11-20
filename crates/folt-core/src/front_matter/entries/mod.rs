use std::ops::Range;

use super::error::{FrontMatterError, Result};
use super::reference::flow::mapping;
use super::scalar::{meaningful, text};
use super::scan::line;
use super::tree::Node;

mod block;

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
