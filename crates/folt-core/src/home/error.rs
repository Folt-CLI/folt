use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::config::settings::SettingsError;
use crate::layout::LayoutVersion;

/// Errors that can occur while resolving or opening a Folt home.
#[derive(Debug)]
pub enum HomeError {
    NoHomeDirectory,
    InvalidFoltHome(OsString),
    InvalidSettings(SettingsError),
    CannotCreate {
        path: PathBuf,
        source: io::Error,
    },
    WriteFailed {
        path: PathBuf,
        source: io::Error,
    },
    NotAHome {
        path: PathBuf,
    },
    WritableByOthers {
        path: PathBuf,
        mode: u32,
    },
    InvalidFormatVersion {
        path: PathBuf,
        found: String,
    },
    MissingFormatVersion {
        path: PathBuf,
    },
    NewerFormatVersion {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    OlderFormatVersion {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    /// A registry failure, boxed because the registry error type lives above this layer.
    Registry(Box<dyn Error + Send + Sync>),
    Locked {
        path: PathBuf,
    },
    NotRegularFile {
        path: PathBuf,
    },
    LockIo {
        path: PathBuf,
        source: io::Error,
    },
}

fn may_be_in_use(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }

    #[cfg(windows)]
    {
        /// Windows sharing violation error code (dimensionless).
        const ERROR_SHARING_VIOLATION: i32 = 32;
        /// Windows lock violation error code (dimensionless).
        const ERROR_LOCK_VIOLATION: i32 = 33;
        matches!(
            error.raw_os_error(),
            Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
        )
    }
    #[cfg(not(windows))]
    {
        false
    }
}

impl fmt::Display for HomeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHomeDirectory => write!(
                formatter,
                "could not determine the home directory; set FOLT_HOME"
            ),
            Self::InvalidSettings(source) => fmt::Display::fmt(source, formatter),
            Self::InvalidFoltHome(value) => {
                write!(formatter, "FOLT_HOME must be an absolute path: {value:?}")
            }
            Self::CannotCreate { path, source } => {
                write!(
                    formatter,
                    "cannot create Folt home {}: {source}",
                    path.display()
                )
            }
            Self::WriteFailed { path, source } => {
                if may_be_in_use(source) {
                    write!(
                        formatter,
                        "cannot write {}: {source}. The file may be open in another application or not writable; close other applications using it, check its permissions and try again.",
                        path.display()
                    )
                } else {
                    write!(formatter, "cannot write {}: {source}", path.display())
                }
            }
            Self::NotAHome { path } => {
                write!(formatter, "not a Folt home directory: {}", path.display())
            }
            Self::WritableByOthers { path, mode } => write!(
                formatter,
                "refusing Folt home {} with mode {mode:04o}: group or others can write to it; remove group and other write permission (chmod go-w) from that folder (chmod 700 on that folder makes it private)",
                path.display()
            ),
            Self::InvalidFormatVersion { path, found } => write!(
                formatter,
                "Folt registry {} contains invalid home format version '{found}'",
                path.display()
            ),
            Self::MissingFormatVersion { path } => write!(
                formatter,
                "Folt registry {} is missing its home format version",
                path.display()
            ),
            Self::NewerFormatVersion { found, supported } => write!(
                formatter,
                "Folt home format version {found} is newer than supported version {supported}"
            ),
            Self::OlderFormatVersion { found, supported } => write!(
                formatter,
                "Folt home format version {found} has an unsupported older major version (supported: {supported})"
            ),
            Self::Registry(source) => write!(formatter, "{source}"),
            Self::Locked { path } => {
                write!(formatter, "Folt home is locked: {}", path.display())
            }
            Self::NotRegularFile { path } => {
                write!(
                    formatter,
                    "Folt home lock is not a regular file: {}",
                    path.display()
                )
            }
            Self::LockIo { path, source } => {
                write!(
                    formatter,
                    "cannot lock Folt home file {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for HomeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CannotCreate { source, .. }
            | Self::WriteFailed { source, .. }
            | Self::LockIo { source, .. } => Some(source),
            Self::InvalidSettings(source) => Some(source),
            Self::Registry(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<SettingsError> for HomeError {
    fn from(error: SettingsError) -> Self {
        Self::InvalidSettings(error)
    }
}
