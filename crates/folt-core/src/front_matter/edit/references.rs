use crate::config::names::OWNED_REFERENCES;

use super::super::error::{FrontMatterError, Result};
use super::super::keys::node;
use super::super::reference::{Reference, references};
use super::super::scalar::{meaningful, text};
use super::super::scan::line;
use super::super::tree::{Block, Node};
use super::scalar::{plain, quoted};
use super::{Edit, checked_edit, edit, edit_checks, insert};

impl Block {
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
}
pub(in super::super) fn render_reference(r: &Reference<'_>, ending: &str) -> Result<String> {
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
