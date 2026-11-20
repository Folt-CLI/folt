use super::super::error::{FrontMatterError, Result};
use super::super::reference::{Reference, ReferenceNode};
use super::super::scalar::{meaningful, text};
use super::super::scan::{colon, line, punctuation, scan_punctuation};
use super::super::tree::{Block, Node};
use super::{Edit, checked_edit, edit};

pub(super) fn set_block_reference(
    bytes: &[u8],
    block: &Block,
    entry: &ReferenceNode<'_>,
    value: Option<&Reference<'_>>,
    rendered: Option<String>,
    indent: usize,
) -> Result<Option<Edit>> {
    let mut rendered = rendered.unwrap_or_default();
    let header = line(bytes, entry.range.start, entry.range.end);
    let raw = text(&bytes[entry.range.start..header.end])?;
    let header_value = &raw[colon(raw).ok_or(FrontMatterError::InvalidValue)? + 1..];
    let nested = header_value.trim().is_empty() || header_value.trim_start().starts_with('#');
    let flow_comments = if nested {
        Vec::new()
    } else {
        scan_punctuation(text(&bytes[entry.range.clone()])?, '#', true)
            .into_iter()
            .map(|offset| entry.range.start + offset)
            .collect::<Vec<_>>()
    };
    let header_comment = if nested {
        punctuation(raw, '#').first().copied()
    } else {
        flow_comments
            .first()
            .filter(|pos| **pos < header.end)
            .map(|pos| pos - entry.range.start)
    };
    if let Some(comment) = header_comment {
        let comment = &raw[comment..];
        if value.is_some() {
            let end = rendered
                .find(block.line_ending)
                .ok_or(FrontMatterError::InvalidValue)?;
            rendered.insert_str(end, &format!(" {comment}"));
        } else {
            rendered = format!("{}{comment}{}", " ".repeat(indent), block.line_ending);
        }
    }
    let mut preserved = Vec::new();
    // Comments and blank lines between fields/entries are document
    // bytes, even when they fall inside the selected entry's span.
    let mut pos = header.next;
    while pos < entry.range.end {
        let l = line(bytes, pos, entry.range.end);
        let raw = text(&bytes[pos..l.end])?;
        if !nested {
            if let Some(comment) = flow_comments
                .iter()
                .find(|offset| **offset >= pos && **offset < l.end)
            {
                let spaces = if value.is_some() { indent + 2 } else { indent };
                preserved.extend_from_slice(
                    format!(
                        "{}{}{}",
                        " ".repeat(spaces),
                        text(&bytes[*comment..l.end])?,
                        block.line_ending
                    )
                    .as_bytes(),
                );
            } else if raw.trim().is_empty() {
                preserved.extend_from_slice(&bytes[pos..l.next]);
            }
        } else if !meaningful(raw) {
            preserved.extend_from_slice(&bytes[pos..l.next]);
        } else if let Some(comment) = punctuation(raw, '#').first() {
            let comment = &raw[*comment..];
            let key = raw[..colon(raw).ok_or(FrontMatterError::InvalidValue)?].trim();
            let field = format!("{}{}{key}:", block.line_ending, " ".repeat(indent + 2));
            if let Some(start) = rendered.find(&field) {
                let start = start + field.len();
                let end = start
                    + rendered[start..]
                        .find(block.line_ending)
                        .ok_or(FrontMatterError::InvalidValue)?;
                rendered.insert_str(end, &format!(" {comment}"));
            } else {
                // Removed fields keep their original indentation;
                // removing the alias moves comments to entry level.
                let spaces = if value.is_some() {
                    raw.len() - raw.trim_start_matches(' ').len()
                } else {
                    indent
                };
                preserved.extend_from_slice(
                    format!("{}{comment}{}", " ".repeat(spaces), block.line_ending).as_bytes(),
                );
            }
        }
        pos = l.next;
    }
    let mut replacement = rendered.into_bytes();
    replacement.extend_from_slice(&preserved);
    checked_edit(bytes, edit(entry.range.clone(), replacement))
}

pub(super) fn append_reference(
    bytes: &[u8],
    n: &Node,
    entries: &[ReferenceNode<'_>],
    rendered: Option<String>,
) -> Result<Option<Edit>> {
    if let Some(rendered) = rendered {
        let position = entries.last().map_or(n.continuation.start, |e| e.range.end);
        return checked_edit(bytes, edit(position..position, rendered));
    }
    Ok(None)
}
