use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use crate::config::formats::{APPLICATION_ID, SQLITE_HEADER};
use crate::config::names::{HOME_ENTRIES, REGISTRY_FILE};

use super::HomeError;

pub(super) fn recognize_home(path: &Path) -> Result<(), HomeError> {
    let entries = fs::read_dir(path).map_err(|source| HomeError::CannotCreate {
        path: path.to_path_buf(),
        source,
    })?;
    let mut allowed = true;
    for entry in entries {
        let entry = entry.map_err(|source| HomeError::CannotCreate {
            path: path.to_path_buf(),
            source,
        })?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| HOME_ENTRIES.contains(&name))
        {
            allowed = false;
            break;
        }
    }
    if allowed || has_folt_application_id(&path.join(REGISTRY_FILE)) {
        Ok(())
    } else {
        Err(HomeError::NotAHome {
            path: path.to_path_buf(),
        })
    }
}

pub(crate) fn is_regular_file(path: &Path) -> io::Result<bool> {
    Ok(fs::metadata(path)?.is_file())
}

/// SQLite header prefix length needed for the application id, in bytes.
const SQLITE_HEADER_LEN: usize = 72;
/// Application id byte offset in the SQLite header.
const APPLICATION_ID_OFFSET: usize = 68;

fn has_folt_application_id(path: &Path) -> bool {
    if !is_regular_file(path).unwrap_or(false) {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0; SQLITE_HEADER_LEN];
    if file.read_exact(&mut header).is_err() || &header[..16] != SQLITE_HEADER {
        return false;
    }
    u32::from_be_bytes([
        header[APPLICATION_ID_OFFSET],
        header[APPLICATION_ID_OFFSET + 1],
        header[APPLICATION_ID_OFFSET + 2],
        header[APPLICATION_ID_OFFSET + 3],
    ]) == APPLICATION_ID
}
