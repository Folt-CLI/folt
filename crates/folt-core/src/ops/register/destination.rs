use std::path::Path;

use crate::config::names::MARKER_FILE;

use super::plan::RegisterEntry;

pub(super) fn register_destination(
    is_directory: bool,
    path: &Path,
    resolution_succeeded: bool,
) -> &Path {
    if is_directory {
        if resolution_succeeded {
            destination_path(path)
        } else {
            // A failed resolution leaves the request path, not a marker path.
            path
        }
    } else {
        destination_path(path)
    }
}

pub(super) fn is_marker(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(MARKER_FILE))
}

pub(super) fn destination_path(path: &Path) -> &Path {
    if is_marker(path)
        && let Some(parent) = path.parent()
    {
        // Marker spelling may change during replan; its directory must not.
        parent
    } else {
        path
    }
}

pub(super) fn destination(entry: &RegisterEntry) -> &Path {
    destination_path(&entry.path)
}

#[cfg(test)]
mod tests;
