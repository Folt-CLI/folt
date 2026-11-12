//! Typed runtime settings.

use std::{error::Error, ffi::OsString, fmt, time::Duration};

use serde::Deserialize;

use crate::config::env::Environment;
use crate::config::limits::{
    DEFAULT_BATCH_TOTAL_BYTES, DEFAULT_BUSY_TIMEOUT, DEFAULT_LARGE_FILE_BYTES,
};
use crate::config::names::{
    ENV_FOLT_BATCH_TOTAL_BYTES, ENV_FOLT_BUSY_TIMEOUT_MS, ENV_FOLT_LARGE_FILE_BYTES,
};

/// Values read from the Folt home's `config.toml` before resolution.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct FileSettings {
    pub username: Option<String>,
    pub email: Option<String>,
    /// Files at or above this size need explicit registration confirmation.
    pub large_file_bytes: Option<u64>,
}

/// Resolved runtime settings with Environment and defaults applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    /// The configured author name (UTF-8 text).
    pub username: Option<String>,
    /// The configured author email (UTF-8 text).
    pub email: Option<String>,
    /// Registration confirmation threshold in bytes (inclusive).
    pub large_file_bytes: u64,
    /// Batch confirmation threshold in bytes (strictly exceeded).
    pub batch_total_bytes: u64,
    /// Registry busy timeout as a duration, resolved from milliseconds.
    pub busy_timeout: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: None,
            email: None,
            large_file_bytes: DEFAULT_LARGE_FILE_BYTES,
            batch_total_bytes: DEFAULT_BATCH_TOTAL_BYTES,
            busy_timeout: DEFAULT_BUSY_TIMEOUT,
        }
    }
}

impl Settings {
    /// Applies Environment overrides, file values and defaults, validating overrides.
    pub fn resolve(environment: &Environment, file: FileSettings) -> Result<Self, SettingsError> {
        let bytes_expected = "a non-negative integer";
        let large_file_bytes = parse_override(
            &environment.large_file_bytes,
            ENV_FOLT_LARGE_FILE_BYTES,
            bytes_expected,
            u64::MAX,
        )?;
        let batch_total_bytes = parse_override(
            &environment.batch_total_bytes,
            ENV_FOLT_BATCH_TOTAL_BYTES,
            bytes_expected,
            u64::MAX,
        )?;
        let busy_timeout_ms = parse_override(
            &environment.busy_timeout_ms,
            ENV_FOLT_BUSY_TIMEOUT_MS,
            "a whole number of milliseconds from 0 to 2147483647",
            i32::MAX as u64,
        )?;
        Ok(Self {
            username: file.username,
            email: file.email,
            large_file_bytes: large_file_bytes
                .or(file.large_file_bytes)
                .unwrap_or(DEFAULT_LARGE_FILE_BYTES),
            batch_total_bytes: batch_total_bytes.unwrap_or(DEFAULT_BATCH_TOTAL_BYTES),
            busy_timeout: busy_timeout_ms.map_or(DEFAULT_BUSY_TIMEOUT, Duration::from_millis),
        })
    }
}

fn parse_override(
    value: &Option<OsString>,
    variable: &'static str,
    expected: &'static str,
    maximum: u64,
) -> Result<Option<u64>, SettingsError> {
    let Some(value) = value.as_ref().filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = value
        .to_str()
        .filter(|text| text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<u64>().ok())
        .filter(|number| *number <= maximum);
    parsed.map(Some).ok_or_else(|| SettingsError::Invalid {
        variable,
        value: value.clone(),
        expected,
    })
}

/// An invalid Environment override, including its variable name and value.
#[derive(Debug)]
pub enum SettingsError {
    Invalid {
        variable: &'static str,
        value: OsString,
        expected: &'static str,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid {
                variable,
                value,
                expected,
            } => {
                write!(formatter, "{variable} must be {expected}: {value:?}")
            }
        }
    }
}

impl Error for SettingsError {}

#[cfg(test)]
mod tests;
