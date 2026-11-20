use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::placement_path::RegistryError;

pub(super) fn resolved_path(path: impl AsRef<Path>) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

pub(super) fn injected() -> RegistryError {
    RegistryError::Io(io::Error::other("injected failure"))
}
