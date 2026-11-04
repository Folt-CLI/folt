//! Validated Folio labels.
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;

use crate::config::limits::{FOLIO_NAME_MAX, HEX_ID_LEN};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FolioName(String);

impl FolioName {
    /// Slugs a UTF-8 path stem, preserving ASCII case and allowed punctuation.
    pub fn from_stem(stem: impl AsRef<OsStr>) -> Result<Self, StemError> {
        let stem = stem.as_ref().to_str().ok_or(StemError::NonUtf8)?;
        let mut slug = String::new();
        let mut replacing_run = false;
        for ch in stem.chars() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                slug.push(ch);
                replacing_run = false;
            } else if !replacing_run {
                slug.push('-');
                replacing_run = true;
            }
        }
        let slug = slug.trim_matches(['.', '-']);
        // Slugging produces only ASCII, so the byte limit is a character boundary.
        let slug = slug[..slug.len().min(FOLIO_NAME_MAX)].trim_end_matches(['.', '-']);
        if slug.is_empty() {
            return Err(StemError::Empty);
        }
        if slug.len() == HEX_ID_LEN && slug.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StemError::HexDigest);
        }
        Ok(Self(slug.into()))
    }

    /// Appends a numeric suffix, shortening the base to keep the 64-byte limit.
    pub fn with_suffix(&self, n: u32) -> Self {
        let suffix = format!("_{n}");
        let base = &self.0[..self.0.len().min(FOLIO_NAME_MAX - suffix.len())];
        let base = base.trim_end_matches(['.', '-']);
        Self(format!("{base}{suffix}"))
    }

    pub fn parse(value: &str) -> Result<Self, FolioNameError> {
        if !(1..=FOLIO_NAME_MAX).contains(&value.len()) {
            return Err(FolioNameError::Length);
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(FolioNameError::Character);
        }
        if value.starts_with(['.', '-']) || value.ends_with(['.', '-']) {
            return Err(FolioNameError::Edge);
        }
        if value.len() == HEX_ID_LEN && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(FolioNameError::HexDigest);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for FolioName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for FolioName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolioNameError {
    Length,
    Character,
    Edge,
    HexDigest,
}
impl fmt::Display for FolioNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Length => "Folio name must contain 1 to 64 bytes",
            Self::Character => "Folio name allows only A-Z, a-z, 0-9, '.', '_' and '-'; @, #, :, / and whitespace are reserved",
            Self::Edge => "Folio name must not start or end with '.' or '-'",
            Self::HexDigest => "Folio name must not be a 64-digit hexadecimal string",
        })
    }
}
impl Error for FolioNameError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StemError {
    NonUtf8,
    Empty,
    HexDigest,
}

impl fmt::Display for StemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonUtf8 => "Path stem is not UTF-8; provide --name",
            Self::Empty => "Path stem produces an empty Folio name; provide --name",
            Self::HexDigest => {
                "Path stem produces a 64-digit hexadecimal Folio name; provide --name"
            }
        })
    }
}

impl Error for StemError {}

#[cfg(test)]
mod tests;
