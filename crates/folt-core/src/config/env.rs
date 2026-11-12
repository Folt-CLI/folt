//! The only reader of the process environment, the OS home directory and the
//! current directory; every other module receives values from it.

#![expect(
    clippy::disallowed_methods,
    reason = "Environment is the only reader of the process environment, the OS home directory and the current directory"
)]

use std::{ffi::OsString, io, path::PathBuf};

use crate::config::names::{
    ENV_FOLT_BATCH_TOTAL_BYTES, ENV_FOLT_BUSY_TIMEOUT_MS, ENV_FOLT_HOME, ENV_FOLT_LARGE_FILE_BYTES,
};

/// Values read from the process environment and the OS home directory.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Environment {
    /// The value of `FOLT_HOME` when set.
    pub folt_home: Option<OsString>,
    /// The value of `FOLT_LARGE_FILE_BYTES` when set.
    pub large_file_bytes: Option<OsString>,
    /// The value of `FOLT_BATCH_TOTAL_BYTES` when set.
    pub batch_total_bytes: Option<OsString>,
    /// The value of `FOLT_BUSY_TIMEOUT_MS` when set.
    pub busy_timeout_ms: Option<OsString>,
    /// The OS home directory when known.
    pub platform_home: Option<PathBuf>,
}

impl Environment {
    /// Captures `FOLT_HOME`, `FOLT_LARGE_FILE_BYTES`, `FOLT_BATCH_TOTAL_BYTES`,
    /// `FOLT_BUSY_TIMEOUT_MS` and the OS home directory without reading the current directory.
    pub fn capture() -> Self {
        Self {
            folt_home: std::env::var_os(ENV_FOLT_HOME),
            large_file_bytes: std::env::var_os(ENV_FOLT_LARGE_FILE_BYTES),
            batch_total_bytes: std::env::var_os(ENV_FOLT_BATCH_TOTAL_BYTES),
            busy_timeout_ms: std::env::var_os(ENV_FOLT_BUSY_TIMEOUT_MS),
            platform_home: std::env::home_dir(),
        }
    }

    /// Reads the current directory at the call, returning its result unchanged.
    pub fn current_dir() -> io::Result<PathBuf> {
        std::env::current_dir()
    }
}
