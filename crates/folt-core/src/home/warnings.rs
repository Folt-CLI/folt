use std::fmt;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::HomeError;

pub(super) fn home_warnings(_path: &Path) -> Result<Vec<HomeWarning>, HomeError> {
    #[cfg(unix)]
    {
        let path = _path;
        let mode = fs::metadata(path)
            .map_err(|source| HomeError::CannotCreate {
                path: path.to_path_buf(),
                source,
            })?
            .mode()
            & 0o7777;
        if mode & 0o022 != 0 {
            return Err(HomeError::WritableByOthers {
                path: path.to_path_buf(),
                mode,
            });
        }
        if mode & 0o044 != 0 {
            return Ok(vec![HomeWarning::ReadableByOthers {
                path: path.to_path_buf(),
                mode,
            }]);
        }
    }
    Ok(Vec::new())
}

/// A nonfatal concern found when opening a Folt home.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeWarning {
    ReadableByOthers { path: PathBuf, mode: u32 },
}

impl fmt::Display for HomeWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadableByOthers { path, mode } => write!(
                formatter,
                "Folt home {} has mode {mode:04o}; group or others can read its contents; chmod 700 on that folder makes it private",
                path.display()
            ),
        }
    }
}
