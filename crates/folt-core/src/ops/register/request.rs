use std::path::{Path, PathBuf};

use crate::folio::name::FolioName;

/// One path and its optional explicit name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterRequest {
    pub path: PathBuf,
    pub name: Option<FolioName>,
}
impl<P: AsRef<Path>> From<P> for RegisterRequest {
    fn from(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            name: None,
        }
    }
}
impl RegisterRequest {
    pub fn new(path: impl AsRef<Path>, name: FolioName) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            name: Some(name),
        }
    }
}
