use std::{fmt, str::FromStr};

use crate::config::limits::{
    HEX_ID_LEN, PIN_COMMIT_OFFSET, PIN_NAME_LEN, REF_NAME_MAX, UUID_TEXT_LEN,
};
use crate::id::{FolioId, ObjectId};

use super::LowerRefError;

/// A portable ref name. Retention names are constructed with `pin_name`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RefName(pub(super) String);

impl RefName {
    pub fn parse(value: &str) -> Result<Self, LowerRefError> {
        if value.is_empty()
            || value.len() > REF_NAME_MAX
            || value.contains("..")
            || (value.len() == HEX_ID_LEN && value.bytes().all(|b| b.is_ascii_hexdigit()))
            || !value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-/".contains(&b))
            || value.split('/').any(|part| {
                part.is_empty()
                    || part.starts_with(['.', '-'])
                    || part.ends_with('.')
                    || part.ends_with(".lock")
                    || device_name(part)
            })
        {
            return Err(LowerRefError::InvalidName(value.into()));
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn pin_parts(&self) -> Result<(FolioId, ObjectId), LowerRefError> {
        parse_pin(&self.0)
    }
}
impl FromStr for RefName {
    type Err = LowerRefError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl fmt::Display for RefName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
fn device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part);
    matches!(stem, "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// The fixed retention filename is 101 bytes, independently of the user name limit.
pub fn pin_name(parent: FolioId, commit: ObjectId) -> RefName {
    RefName(format!("{parent}-{commit}"))
}
/// Splits a canonical retention filename into its parent Folio and commit.
pub fn parse_pin(value: &str) -> Result<(FolioId, ObjectId), LowerRefError> {
    if value.len() != PIN_NAME_LEN || value.as_bytes()[UUID_TEXT_LEN] != b'-' || !value.is_ascii() {
        return Err(LowerRefError::InvalidName(value.into()));
    }
    let parent = FolioId::parse(&value[..UUID_TEXT_LEN])
        .map_err(|_| LowerRefError::InvalidName(value.into()))?;
    let commit = value[PIN_COMMIT_OFFSET..]
        .parse()
        .map_err(|_| LowerRefError::InvalidName(value.into()))?;
    Ok((parent, commit))
}
