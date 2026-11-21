use super::{FileSystem, Signature, TempFile, ensure_single, signature, temp_name};
use crate::config::names::TEMP_TAG_NAME;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Replace an unchanged regular file through a synced sibling temporary file.
/// `published` runs immediately after rename, before the directory sync, so
/// callers can roll back even when the final durability step fails.
pub(crate) fn atomic_rewrite(
    fs: &dyn FileSystem,
    path: &Path,
    bytes: &[u8],
    expected: &Signature,
    published: impl FnOnce(Signature),
) -> io::Result<()> {
    let check = || {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || signature(&metadata) != *expected {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file changed"));
        }
        if metadata.permissions().readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only file",
            ));
        }
        ensure_single(fs.link_count(path)?)?;
        Ok(metadata)
    };
    let metadata = check()?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary =
        TempFile::sibling_at(&parent.join(temp_name(&format!(".{TEMP_TAG_NAME}-"), '-')))?;
    let mut output = create_rewrite_temp(temporary.path(), &metadata.permissions())?;
    let result = (|| {
        output.write_all(bytes)?;
        output.sync_all()?;
        check()?;
        let fallback_signature = signature(&output.metadata()?);
        // Known limit: a rename cannot be conditional on content, so an edit made
        // between the check above and this rename is replaced. See "Known limit:
        // the write window" in docs/dev/library/register.md.
        temporary.publish(path)?;
        let metadata = fs.rewrite_metadata(&output, path);
        // Rename may change ctime. Prefer the post-rename signature, but always
        // record publication before propagating a metadata failure. The fallback
        // lets rollback restore the file or report its path if the signature no
        // longer matches; a published name must never be silently forgotten.
        published(
            metadata
                .as_ref()
                .map(signature)
                .unwrap_or(fallback_signature),
        );
        metadata?;
        fs.sync_directory(parent)
    })();
    drop(output);
    result
}

fn create_rewrite_temp(path: &Path, permissions: &fs::Permissions) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(permissions.mode());
    }
    let output = options.open(path)?;
    if let Err(error) = output.set_permissions(permissions.clone()) {
        drop(output);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
