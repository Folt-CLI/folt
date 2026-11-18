use std::path::{Path, PathBuf};

use crate::config::names::{CONTENT_FILE, FOLIOS_DIR};
use crate::home::Home;
use crate::id::FolioId;

/// Returns the path of `folio.main` in the Folio directory of that Folio id.
pub fn content_path(home: &Home, id: FolioId) -> PathBuf {
    home.path()
        .join(FOLIOS_DIR)
        .join(id.to_string())
        .join(CONTENT_FILE)
}

pub(crate) fn former_home(target: &Path, id: FolioId) -> Option<PathBuf> {
    let suffix = PathBuf::from(FOLIOS_DIR)
        .join(id.to_string())
        .join(CONTENT_FILE);
    let home = target.parent()?.parent()?.parent()?;
    (target.is_absolute() && home.join(suffix) == target).then(|| home.to_path_buf())
}
