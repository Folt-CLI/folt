use std::borrow::Cow;
use std::ops::Range;

use crate::config::names::OWNED_REFERENCES;
use crate::front_matter::entries::entries;
use crate::id::FolioId;

use super::error::{FrontMatterError, Result};
use super::keys::{node, unresolved_key_checks};
use super::locate::locate;
use super::scalar::{scalar, supported, text};
use super::tree::Node;
use flow::mapping;

pub(super) mod flow;

/// An alias with exactly one of name or canonical id, and an optional opaque
/// pin. Name grammar and commit validation belong to callers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference<'a> {
    pub alias: Cow<'a, str>,
    pub name: Option<Cow<'a, str>>,
    pub id: Option<FolioId>,
    pub pin: Option<Cow<'a, str>>,
}
pub(super) struct ReferenceNode<'a> {
    pub(super) reference: Reference<'a>,
    pub(super) range: Range<usize>,
    pub(super) key_range: Range<usize>,
    pub(super) value: Range<usize>,
}

fn reference<'a>(
    alias: &'a str,
    value: &'a str,
    fields: Option<Vec<(&'a str, &'a str)>>,
) -> Result<Reference<'a>> {
    let alias = scalar(alias)?;
    if alias.is_empty() {
        return Err(FrontMatterError::InvalidValue);
    }
    let mut result = Reference {
        alias,
        name: None,
        id: None,
        pin: None,
    };
    let fields = if let Some(fields) = fields {
        fields
    } else if value.trim_start().starts_with('{') {
        mapping(value)?
    } else {
        scalar(value)?;
        return Err(FrontMatterError::InvalidValue);
    };
    let mut seen = Vec::new();
    for (key, value) in fields {
        if seen.contains(&key) {
            return Err(FrontMatterError::Duplicate);
        }
        seen.push(key);
        let value = scalar(value)?;
        if value.is_empty() {
            return Err(FrontMatterError::InvalidValue);
        }
        match key {
            "name" => result.name = Some(value),
            "id" => {
                result.id =
                    Some(FolioId::parse(&value).map_err(|_| FrontMatterError::InvalidValue)?)
            }
            "pin" => result.pin = Some(value),
            _ => return Err(FrontMatterError::Unsupported),
        }
    }
    if result.name.is_some() == result.id.is_some() {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(result)
}
pub(super) fn references<'a>(bytes: &'a [u8], n: &Node) -> Result<Vec<ReferenceNode<'a>>> {
    supported(bytes, n)?;
    let mut result = Vec::new();
    for entry in entries(bytes, n)? {
        let entry = entry?;
        result.push(ReferenceNode {
            reference: reference(
                text(&bytes[entry.key_range.clone()])?,
                text(&bytes[entry.value.clone()])?,
                entry.fields,
            )?,
            range: entry.range,
            key_range: entry.key_range,
            value: entry.value,
        });
    }
    let mut aliases = Vec::new();
    for entry in &result {
        if aliases.contains(&entry.reference.alias.as_ref()) {
            return Err(FrontMatterError::Duplicate);
        }
        aliases.push(entry.reference.alias.as_ref());
    }
    Ok(result)
}
/// Reads block or flow reference mappings; absent metadata yields no entries.
pub fn read_references(bytes: &[u8]) -> Result<Vec<Reference<'_>>> {
    let Some(block) = locate(bytes)? else {
        return Ok(Vec::new());
    };
    unresolved_key_checks(bytes, &block)?;
    let Some(n) = node(&block, OWNED_REFERENCES)? else {
        return Ok(Vec::new());
    };
    Ok(references(bytes, n)?
        .into_iter()
        .map(|n| n.reference)
        .collect())
}
