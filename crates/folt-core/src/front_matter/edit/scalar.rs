use crate::config::names::{OWNED_ID, OWNED_NAME};
use crate::id::FolioId;

use super::super::error::{FrontMatterError, Result};
use super::super::keys::node;
use super::super::read::read_scalar;
use super::super::scalar::scalar;
use super::super::tree::Block;
use super::{Edit, checked_edit, edit, edit_checks, insert, value_edit};

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
}
pub(super) fn plain(value: &str) -> Result<()> {
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

pub(super) fn quoted(value: &str) -> Result<String> {
    if value.is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
