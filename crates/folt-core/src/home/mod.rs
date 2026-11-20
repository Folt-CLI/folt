use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::env::Environment;

pub mod config_file;
mod error;
mod locate;
mod lock;
mod open;
mod recognize;
mod warnings;

pub use error::HomeError;
pub use locate::resolve_home;
pub use lock::HomeLock;
pub(crate) use recognize::is_regular_file;
pub use warnings::HomeWarning;

/// A resolved Folt home directory.
///
/// The home and every Folio directory are trusted: whoever can change them can
/// change everything Folt writes. A process running as the same user is not
/// defended against. On Unix, the home is created private, a home writable by
/// group or others is refused, and a home readable by them produces a warning.
/// On other platforms this trust boundary is documentation only.
pub struct Home {
    path: PathBuf,
    warnings: Vec<HomeWarning>,
    environment: Environment,
    busy_timeout: Duration,
}

impl Home {
    /// Resolves the home path using the process environment, without touching
    /// the filesystem.
    ///
    /// The platform home directory may change behavior on Windows, and redirected
    /// Windows profiles are not handled.
    pub fn locate() -> Result<PathBuf, HomeError> {
        resolve_home(&Environment::capture())
    }

    pub(crate) fn environment(&self) -> &Environment {
        &self.environment
    }

    pub(crate) fn busy_timeout(&self) -> Duration {
        self.busy_timeout
    }

    /// Returns the home directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns nonfatal warnings found when opening the home.
    pub fn warnings(&self) -> &[HomeWarning] {
        &self.warnings
    }

    /// Acquires the exclusive home lock, waiting until it is available.
    ///
    /// Waits forever if this process already holds the lock, so keep one guard
    /// per process and pass it down.
    /// Share `&HomeLock` for operations that are safe side by side: object puts,
    /// `create_folio` and registry writes. Exclusive operations take `&mut HomeLock`.
    /// If threads need both, wrap the guard in a [`std::sync::RwLock`] (read for
    /// puts, write for exclusive operations) or a [`std::sync::Mutex`].
    pub fn lock(&self) -> Result<HomeLock, HomeError> {
        self.acquire_lock(false)
    }

    /// Attempts to acquire the exclusive home lock without waiting.
    pub fn try_lock(&self) -> Result<HomeLock, HomeError> {
        self.acquire_lock(true)
    }
}

#[cfg(test)]
mod tests;
