use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::config::names::MARKER_FILE;
use crate::config::settings::Settings;
use crate::placement_path::PlacementPath;
use crate::registry::Registry;

use super::error::RegisterError;
use super::plan::{NameWrite, RegisterEntry, RegisterStatus};
use super::refusal::{RegisterRefusalReason, io_refusal};

pub(super) fn inspect_entry(
    home: &Path,
    registry: &Registry,
    settings: &Settings,
    input: &Path,
    path: PathBuf,
) -> Result<RegisterEntry, RegisterError> {
    let mut entry = RegisterEntry {
        path: path.clone(),
        status: RegisterStatus::New,
        size: 0,
        creates_marker: false,
        name: None,
        name_source: None,
        name_write: NameWrite::None,
        signature: None,
        prefix: None,
    };
    let refuse = |mut entry: RegisterEntry, reason| {
        entry.status = RegisterStatus::Refused(reason);
        entry
    };
    if path.starts_with(home) {
        return Ok(refuse(entry, RegisterRefusalReason::InsideHome));
    }
    if let Ok(key) = PlacementPath::resolve(input)
        && registry.placement_at(&key)?.is_some()
    {
        entry.status = RegisterStatus::AlreadyRegistered;
        return Ok(entry);
    }
    for ancestor in path.ancestors().skip(1) {
        let marker = ancestor.join(MARKER_FILE);
        if marker != path
            && let Ok(key) = PlacementPath::resolve(&marker)
            && registry.placement_at(&key)?.is_some()
        {
            return Ok(refuse(entry, RegisterRefusalReason::InsideDirectoryFolio));
        }
    }
    let named = match fs::symlink_metadata(input) {
        Ok(meta) => meta,
        Err(error) => return Ok(refuse(entry, io_refusal(&error))),
    };
    if named.is_symlink() {
        return Ok(refuse(entry, RegisterRefusalReason::Symlink));
    }
    let metadata = if named.is_dir() {
        match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                entry.creates_marker = true;
                return Ok(entry);
            }
            Err(error) => return Ok(refuse(entry, io_refusal(&error))),
        }
    } else {
        named
    };
    if metadata.is_symlink() {
        return Ok(refuse(entry, RegisterRefusalReason::Symlink));
    }
    if !metadata.is_file() {
        return Ok(refuse(entry, RegisterRefusalReason::NotAFile));
    }
    // Opening a regular file checks access without reading its bytes.
    if File::open(&path).is_err() {
        return Ok(refuse(entry, RegisterRefusalReason::Unreadable));
    }
    entry.signature = Some(crate::fsx::signature(&metadata));
    entry.size = metadata.len();
    if entry.size >= settings.large_file_bytes {
        entry.status = RegisterStatus::NeedsConfirmation { size: entry.size };
    }
    Ok(entry)
}
