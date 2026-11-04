//! Canonical path spelling for expected test output.
use std::path::{Path, PathBuf};

/// Must be used whenever an expected message or plan text contains a path the
/// test created under a temporary directory.
#[warn(clippy::unwrap_used)]
#[expect(
    clippy::unwrap_used,
    reason = "Expected paths refer to test-created parents and valid file names"
)]
pub fn expected_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap().join(path)
    };
    let parent = std::fs::canonicalize(absolute.parent().unwrap()).unwrap();
    let name = absolute.file_name().unwrap();
    if let Some(text) = name.to_str() {
        let flipped: String = text
            .chars()
            .map(|character| {
                if character.is_lowercase() {
                    character.to_uppercase().next().unwrap_or(character)
                } else if character.is_uppercase() {
                    character.to_lowercase().next().unwrap_or(character)
                } else {
                    character
                }
            })
            .collect();
        if text.chars().any(char::is_alphabetic)
            && flipped != text
            && std::fs::symlink_metadata(parent.join(flipped)).is_ok()
        {
            for entry in std::fs::read_dir(&parent).unwrap() {
                let entry = entry.unwrap();
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(text)
                {
                    return parent.join(entry.file_name());
                }
            }
        }
    }
    parent.join(name)
}
