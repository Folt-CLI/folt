use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::RegistryError;

pub(super) fn on_disk_spelling(parent: &Path, name: &OsStr) -> Result<PathBuf, RegistryError> {
    let name_path = Path::new(name);
    let Some(text) = name.to_str() else {
        return Ok(name_path.to_path_buf());
    };
    let has_letter = text.chars().any(char::is_alphabetic);
    if !has_letter || !case_insensitive_probe(parent, text) {
        return Ok(name_path.to_path_buf());
    }
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(text)
        {
            return Ok(entry.file_name().into());
        }
    }
    Ok(name_path.to_path_buf())
}

fn case_insensitive_probe(parent: &Path, name: &str) -> bool {
    let flipped: String = name
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
    flipped != name && fs::symlink_metadata(parent.join(flipped)).is_ok()
}
