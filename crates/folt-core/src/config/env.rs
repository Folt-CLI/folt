//! The only reader of the process environment, the OS home directory and the
//! current directory; every other module receives values from it.

#![expect(
    clippy::disallowed_methods,
    reason = "Environment is the only reader of the process environment, the OS home directory and the current directory"
)]

use std::{ffi::OsString, io, path::PathBuf};

use crate::config::names::ENV_FOLT_HOME;

/// Values read from the process environment and the OS home directory.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Environment {
    /// The value of `FOLT_HOME` when set.
    pub folt_home: Option<OsString>,
    /// The OS home directory when known.
    pub platform_home: Option<PathBuf>,
}

impl Environment {
    /// Captures `FOLT_HOME` and the OS home directory without reading the current directory.
    pub fn capture() -> Self {
        Self {
            folt_home: std::env::var_os(ENV_FOLT_HOME),
            platform_home: std::env::home_dir(),
        }
    }

    /// Reads the current directory at the call, returning its result unchanged.
    pub fn current_dir() -> io::Result<PathBuf> {
        std::env::current_dir()
    }
}
