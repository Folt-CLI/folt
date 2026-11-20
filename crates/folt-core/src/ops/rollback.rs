//! Restores files and removes owned artifacts when an operation does not complete.
use std::fs;
use std::path::{Path, PathBuf};

use crate::fsx::{FileSystem, Signature};

pub(crate) struct Rollback<'a> {
    fs: &'a dyn FileSystem,
    rewritten: Vec<(PathBuf, Vec<u8>, Signature)>,
    directories: Vec<PathBuf>,
    markers: Vec<(PathBuf, Signature)>,
    armed: bool,
}

impl<'a> Rollback<'a> {
    pub(crate) fn new(fs: &'a dyn FileSystem) -> Self {
        Self {
            fs,
            rewritten: Vec::new(),
            directories: Vec::new(),
            markers: Vec::new(),
            armed: true,
        }
    }

    pub(crate) fn record_marker(&mut self, path: PathBuf, signature: Signature) {
        self.markers.push((path, signature));
    }

    pub(crate) fn record_rewrite(
        &mut self,
        path: PathBuf,
        original: Vec<u8>,
        signature: Signature,
    ) {
        if let Some((_, marker_signature)) =
            self.markers.iter_mut().find(|(marker, _)| marker == &path)
        {
            *marker_signature = signature.clone();
        }
        self.rewritten.push((path, original, signature));
    }

    pub(crate) fn record_directory(&mut self, path: PathBuf) {
        self.directories.push(path);
    }

    pub(crate) fn owns_marker(&self, path: &Path) -> bool {
        self.markers.iter().any(|(marker, _)| marker == path)
    }

    pub(crate) fn commit(mut self) {
        self.armed = false;
    }

    /// Explicit failure also removes Folio directories; unwinding leaves them for recovery.
    pub(crate) fn fail(mut self) -> Vec<String> {
        self.armed = false;
        let mut failures = Vec::new();
        self.restore_files(&mut failures);
        self.remove_directories(&mut failures);
        self.remove_markers(&mut failures);
        failures
    }

    fn restore_files(&mut self, failures: &mut Vec<String>) {
        for (path, original, signature) in std::mem::take(&mut self.rewritten).into_iter().rev() {
            if !fs::symlink_metadata(&path)
                .is_ok_and(|meta| meta.is_file() && crate::fsx::signature(&meta) == signature)
            {
                failures.push(format!(
                    "{} keeps the name: file changed after our write",
                    path.display()
                ));
                continue;
            }
            let mut restoration_published = false;
            if let Err(error) =
                crate::fsx::atomic_rewrite(self.fs, &path, &original, &signature, |restored| {
                    restoration_published = true;
                    if let Some((_, marker_signature)) =
                        self.markers.iter_mut().find(|(marker, _)| marker == &path)
                    {
                        *marker_signature = restored;
                    }
                })
            {
                if restoration_published {
                    failures.push(format!(
                        "{}: restoration completed but the directory sync failed; original bytes are back, durability of the restore is not confirmed: {error}",
                        path.display()
                    ));
                } else {
                    failures.push(format!(
                        "{} keeps the name: restore failed: {error}",
                        path.display()
                    ));
                }
            }
        }
    }

    fn remove_directories(&mut self, failures: &mut Vec<String>) {
        for directory in std::mem::take(&mut self.directories) {
            if let Err(error) = crate::fsx::remove_dir_all(&directory) {
                failures.push(format!("{}: {error}", directory.display()));
            }
        }
    }

    fn remove_markers(&mut self, failures: &mut Vec<String>) {
        for (marker, signature) in std::mem::take(&mut self.markers) {
            if fs::symlink_metadata(&marker)
                .is_ok_and(|meta| meta.is_file() && crate::fsx::signature(&meta) == signature)
                && let Err(error) = crate::fsx::remove_file(&marker)
            {
                failures.push(format!("{}: {error}", marker.display()));
            }
        }
    }
}

impl Drop for Rollback<'_> {
    fn drop(&mut self) {
        if self.armed {
            // Drop cannot report failures. Explicit failure returns them to the caller.
            self.armed = false;
            let mut failures = Vec::new();
            self.restore_files(&mut failures);
            self.remove_markers(&mut failures);
        }
    }
}

#[cfg(test)]
mod tests;
