use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_name(prefix: &str, separator: char) -> String {
    format!(
        "{prefix}{}{separator}{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    )
}

/// Creation permissions for a temporary file.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TempMode {
    Private,
    Default,
}

/// Owns a temporary file until it is published or kept in place.
pub(crate) struct TempFile {
    path: PathBuf,
    file: Option<File>,
    armed: bool,
}

impl TempFile {
    // Preserve the attempted path so callers can report creation failures there.
    pub(crate) fn create(
        directory: &Path,
        prefix: &str,
        separator: char,
        mode: TempMode,
    ) -> Result<Self, (PathBuf, io::Error)> {
        loop {
            let path = directory.join(temp_name(prefix, separator));
            match Self::create_at(&path, mode) {
                Ok(file) => return Ok(file),
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err((path, source)),
            }
        }
    }

    pub(crate) fn create_at(path: &Path, mode: TempMode) -> io::Result<Self> {
        let mut options = match mode {
            TempMode::Private => super::private_file_options(),
            TempMode::Default => OpenOptions::new(),
        };
        let file = options.write(true).create_new(true).open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            armed: true,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    #[expect(clippy::expect_used, reason = "only consuming methods close the file")]
    pub(crate) fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("temporary file is open")
    }

    pub(crate) fn sync(&mut self) -> io::Result<()> {
        self.file_mut().sync_all()
    }

    pub(crate) fn publish(mut self, destination: &Path) -> io::Result<()> {
        drop(self.file.take());
        fs::rename(&self.path, destination)?;
        self.armed = false;
        Ok(())
    }

    pub(crate) fn keep(mut self) {
        drop(self.file.take());
        self.armed = false;
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Close first so Windows can remove the file; cleanup is best effort.
        drop(self.file.take());
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests;
