//! Typed runtime settings.

use serde::Deserialize;

use crate::config::limits::DEFAULT_LARGE_FILE_BYTES;

/// Settings read from the Folt home's `config.toml`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Settings {
    pub username: Option<String>,
    pub email: Option<String>,
    /// Files at or above this size need explicit registration confirmation.
    pub large_file_bytes: Option<u64>,
}

impl Settings {
    pub fn large_file_bytes(&self) -> u64 {
        self.large_file_bytes.unwrap_or(DEFAULT_LARGE_FILE_BYTES)
    }
}
