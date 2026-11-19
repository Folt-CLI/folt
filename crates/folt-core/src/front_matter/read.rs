use std::borrow::Cow;

use crate::config::names::{OWNED_ID, OWNED_NAME};
use crate::id::FolioId;

use super::error::{FrontMatterError, Result};
use super::keys::{node, unresolved_key_checks};
use super::locate::locate;
use super::scalar::{meaningful, scalar, supported, text};

pub(super) fn read_scalar<'a>(bytes: &'a [u8], key: &str) -> Result<Option<Cow<'a, str>>> {
    let Some(block) = locate(bytes)? else {
        return Ok(None);
    };
    unresolved_key_checks(bytes, &block)?;
    let Some(n) = node(&block, key)? else {
        return Ok(None);
    };
    supported(bytes, n)?;
    let continuation = text(&bytes[n.continuation.clone()])?;
    if continuation.lines().any(meaningful) {
        return Err(FrontMatterError::Unsupported);
    }
    let spelling = text(&bytes[n.value.clone()])?;
    let value = scalar(spelling)?;
    if value.is_empty() {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(Some(value))
}

/// Reads a scalar name, decoding minimal single/double quoted forms.
pub fn read_name(bytes: &[u8]) -> Result<Option<Cow<'_, str>>> {
    read_scalar(bytes, OWNED_NAME)
}
/// Reads the canonical lowercase hyphenated Folio id.
pub fn read_id(bytes: &[u8]) -> Result<Option<FolioId>> {
    read_scalar(bytes, OWNED_ID)?
        .map(|s| FolioId::parse(&s).map_err(|_| FrontMatterError::InvalidValue))
        .transpose()
}
