use crate::config::names::OWNED_REFERENCES;

use super::super::error::{FrontMatterError, Result};
use super::super::keys::node;
use super::super::reference::{Reference, references};
use super::super::scalar::text;
use super::super::tree::Block;
use super::block::{append_reference, set_block_reference};
use super::flow::set_flow_reference;
use super::references::render_reference;
use super::scalar::plain;
use super::{Edit, edit_checks};

impl Block {
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
