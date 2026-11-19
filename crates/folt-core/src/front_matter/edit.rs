use std::ops::Range;

use super::scan::{colon, explicit_key, line, punctuation, scan_punctuation};
use super::{
    Block, Edit, FrontMatterError, Node, Reference, ReferenceNode, Result, locate, mapping,
    meaningful, node, read_scalar, references, scalar, supported, text, unresolved_key_checks,
};
use crate::config::limits::SCAN_WINDOW;
use crate::config::names::{OWNED, OWNED_ID, OWNED_NAME, OWNED_REFERENCES};
use crate::id::FolioId;

impl Block {
    /// Sets or removes the name. `None` means no edit is needed or block creation
    /// was disabled. Values are written as double-quoted strings, but must still
    /// pass the plain-token safety check; full name validation remains the caller's
    /// responsibility. An existing value produces an edit even when it is equal.
    pub fn set_name(
        block: Option<&Block>,
        bytes: &[u8],
        value: Option<&str>,
        create_block: bool,
    ) -> Result<Option<Edit>> {
        if let Some(value) = value {
            plain(value)?;
        }
        let value = value.map(|value| format!("\"{value}\""));
        let value = value.as_deref();
        if let Some(block) = block {
            edit_checks(bytes, block)?;
            if let Some(n) = node(block, OWNED_NAME)? {
                read_scalar(bytes, OWNED_NAME)?;
                return checked_edit(
                    bytes,
                    match value {
                        Some(value) => value_edit(bytes, n.value.clone(), value.as_bytes()),
                        None => edit(n.range.start..n.continuation.start, Vec::new()),
                    },
                );
            }
        }
        value.map_or(Ok(None), |value| {
            insert(bytes, block, OWNED_NAME, value, create_block)
        })
    }
    /// Sets or removes the id, creating a missing block only when requested.
    pub fn set_id(
        block: Option<&Block>,
        bytes: &[u8],
        value: Option<FolioId>,
        create_block: bool,
    ) -> Result<Option<Edit>> {
        let value = value.map(|id| id.to_string());
        let value = value.as_deref();
        if let Some(value) = value {
            plain(value)?;
        }
        if let Some(block) = block {
            edit_checks(bytes, block)?;
            if let Some(n) = node(block, OWNED_ID)? {
                read_scalar(bytes, OWNED_ID)?;
                return checked_edit(
                    bytes,
                    match value {
                        Some(value) => value_edit(bytes, n.value.clone(), value.as_bytes()),
                        None => edit(n.range.start..n.continuation.start, Vec::new()),
                    },
                );
            }
        }
        value.map_or(Ok(None), |value| {
            insert(bytes, block, OWNED_ID, value, create_block)
        })
    }
    /// Replaces the entire references value, including nested block entries.
    pub fn set_references(
        block: Option<&Block>,
        bytes: &[u8],
        values: &[Reference<'_>],
        create_block: bool,
    ) -> Result<Option<Edit>> {
        let ending = block.map_or("\n", |b| b.line_ending);
        let mut rendered = String::new();
        let mut seen = Vec::new();
        for r in values {
            if seen.contains(&r.alias.as_ref()) {
                return Err(FrontMatterError::Duplicate);
            }
            seen.push(r.alias.as_ref());
            rendered.push_str(&render_reference(r, ending)?);
        }
        if let Some(block) = block {
            edit_checks(bytes, block)?;
            if let Some(n) = node(block, OWNED_REFERENCES)? {
                let entries = references(bytes, n)?;
                if values.is_empty() {
                    return checked_edit(bytes, remove_references(bytes, n));
                }
                // Flow entry spans end inside the collection; include its closing
                // delimiter and header line when converting to block entries.
                let end = entries
                    .last()
                    .map_or(n.continuation.start, |e| e.range.end)
                    .max(n.continuation.start);
                // Keep the header comment and trailing standalone comments. Entry
                // values are rewritten together into the canonical quoted block form.
                let mut replacement = bytes[n.value.end..n.continuation.start].to_vec();
                replacement.extend_from_slice(rendered.as_bytes());
                return checked_edit(bytes, edit(n.value.start..end, replacement));
            }
        }
        if values.is_empty() {
            return Ok(None);
        }
        // insert supplies the final line ending itself.
        insert(
            bytes,
            block,
            OWNED_REFERENCES,
            &format!("{ending}{}", rendered.trim_end_matches(['\n', '\r'])),
            create_block,
        )
    }
    /// Sets or removes one alias, writing quoted block mappings.
    pub fn set_reference(
        block: Option<&Block>,
        bytes: &[u8],
        alias: &str,
        value: Option<&Reference<'_>>,
        create_block: bool,
    ) -> Result<Option<Edit>> {
        plain(alias)?;
        if value.is_some_and(|r| r.alias != alias) {
            return Err(FrontMatterError::InvalidValue);
        }
        if let Some(block) = block {
            edit_checks(bytes, block)?;
            if let Some(n) = node(block, OWNED_REFERENCES)? {
                let entries = references(bytes, n)?;
                let selected = entries.iter().position(|e| e.reference.alias == alias);
                let rendered = value
                    .map(|r| render_reference(r, block.line_ending))
                    .transpose()?;
                let collection = text(&bytes[n.value.clone()])?;
                if collection.starts_with('{') {
                    return set_flow_reference(bytes, n, &entries, selected, value, collection);
                }
                let indent = entries.first().map_or(2, |entry| {
                    bytes[entry.range.start..]
                        .iter()
                        .take_while(|b| **b == b' ')
                        .count()
                });
                let rendered = rendered.map(|s| {
                    s.split_inclusive(block.line_ending)
                        .map(|line| format!("{}{}", " ".repeat(indent), &line[2..]))
                        .collect::<String>()
                });
                if let Some(index) = selected {
                    return set_block_reference(
                        bytes,
                        block,
                        &entries[index],
                        value,
                        rendered,
                        indent,
                    );
                }
                return append_reference(bytes, n, &entries, rendered);
            }
        }
        match value {
            Some(value) => {
                Self::set_references(block, bytes, std::slice::from_ref(value), create_block)
            }
            None => Ok(None),
        }
    }
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
fn plain(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._/".contains(&b))
    {
        return Err(FrontMatterError::InvalidValue);
    }
    scalar(value)?;
    Ok(())
}

fn quoted(value: &str) -> Result<String> {
    if value.is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
pub(super) fn render_reference(r: &Reference<'_>, ending: &str) -> Result<String> {
    plain(&r.alias)?;
    if r.name.is_some() == r.id.is_some() {
        return Err(FrontMatterError::InvalidValue);
    }
    let mut output = format!("  {}:{ending}", r.alias);
    if let Some(name) = &r.name {
        output.push_str(&format!("    name: {}{ending}", quoted(name)?));
    }
    if let Some(id) = r.id {
        output.push_str(&format!("    id: \"{id}\"{ending}"));
    }
    if let Some(pin) = &r.pin {
        output.push_str(&format!("    pin: {}{ending}", quoted(pin)?));
    }
    Ok(output)
}
// Standalone comments and blank lines survive removal of their surrounding key.
fn remove_references(bytes: &[u8], node: &Node) -> Edit {
    let mut replacement = Vec::new();
    let mut pos = node.continuation.start;
    while pos < node.range.end {
        let l = line(bytes, pos, node.range.end);
        if !meaningful(text(&bytes[pos..l.end]).unwrap_or("")) {
            replacement.extend_from_slice(&bytes[pos..l.next]);
        }
        pos = l.next;
    }
    edit(node.range.clone(), replacement)
}

// A single entry inside a flow collection cannot contain a block mapping.
fn render_flow_reference(r: &Reference<'_>) -> Result<String> {
    render_reference(r, "\n")?;
    let mut fields = Vec::new();
    if let Some(name) = &r.name {
        fields.push(format!("name: {}", quoted(name)?));
    }
    if let Some(id) = r.id {
        fields.push(format!("id: \"{id}\""));
    }
    if let Some(pin) = &r.pin {
        fields.push(format!("pin: {}", quoted(pin)?));
    }
    Ok(format!("{}: {{{}}}", r.alias, fields.join(", ")))
}

fn set_flow_reference(
    bytes: &[u8],
    n: &Node,
    entries: &[ReferenceNode<'_>],
    selected: Option<usize>,
    value: Option<&Reference<'_>>,
    collection: &str,
) -> Result<Option<Edit>> {
    // Flow collections must stay flow collections. Only the selected
    // key/value and, on removal, one separator belong to the edit.
    let parts = mapping(collection)?;
    if let Some(index) = selected {
        let start = entries[index].key_range.start;
        let end = entries[index].value.end;
        if let Some(value) = value {
            return checked_edit(bytes, edit(start..end, render_flow_reference(value)?));
        }
        let commas: Vec<_> = punctuation(&collection[1..collection.len() - 1], ',')
            .into_iter()
            .map(|n| n + 1)
            .collect();
        let (range, replacement) = if index < commas.len() {
            let comma = n.value.start + commas[index];
            (start..comma + 1, bytes[end..comma].to_vec())
        } else if index > 0 {
            let comma = n.value.start + commas[index - 1];
            (comma..end, bytes[comma + 1..start].to_vec())
        } else {
            (start..end, Vec::new())
        };
        return checked_edit(bytes, edit(range, replacement));
    }
    if let Some(value) = value {
        let position = n.value.end - 1;
        let has_separator = parts.last().is_some_and(|(_, old)| {
            let end = old.as_ptr() as usize - collection.as_ptr() as usize + old.len();
            !punctuation(&collection[end..collection.len() - 1], ',').is_empty()
        });
        let separator = if entries.is_empty() || has_separator {
            ""
        } else {
            ", "
        };
        return checked_edit(
            bytes,
            edit(
                position..position,
                format!("{separator}{}", render_flow_reference(value)?),
            ),
        );
    }
    Ok(None)
}

fn set_block_reference(
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

fn append_reference(
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
