use std::path::{Path, PathBuf};

use folt_core::home::{Home, HomeLock};

/// Must be used whenever an expected message or plan text contains a path the
/// test created under a temporary directory.
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

/// Owns a private Folt home and removes it when the test finishes.
pub struct TempHome {
    pub home: Home,
    directory: tempfile::TempDir,
}

impl TempHome {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let home = Home::open_or_create(directory.path().join("home")).unwrap();
        Self { home, directory }
    }

    pub fn path(&self) -> &Path {
        self.home.path()
    }

    pub fn root(&self) -> &Path {
        self.directory.path()
    }

    pub fn lock(&self) -> HomeLock {
        self.home.try_lock().unwrap()
    }
}

/// Resolves suite data independently of the process working directory.
pub fn fixture(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("FOLT_TESTS_DIR"))
        .join("fixtures")
        .join(relative)
}
