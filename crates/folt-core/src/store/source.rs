use std::io::{self, SeekFrom};
use std::path::Path;

use crate::fsx::TempFile;
use crate::home::is_regular_file;
use crate::id::ObjectId;
use crate::object::{self, ObjectHasher, ObjectKind};

use super::probe::{MAGIC_SNIFF_BYTES, compression_probe_is_worthwhile};
use super::{ReadSeek, StoreError};

/// Reads the source once and returns the object id of its content.
pub(super) fn hash_source(
    source: &mut dyn ReadSeek,
    kind: ObjectKind,
    length: u64,
    source_path: &Path,
    buffer: &mut [u8],
) -> Result<ObjectId, StoreError> {
    let mut hash = ObjectHasher::new(kind, length);
    let mut count = 0u64;
    loop {
        let n = source.read(buffer).map_err(|source| StoreError::Io {
            path: source_path.into(),
            source,
        })?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .ok_or(StoreError::SourceChanged)?;
        if count > length {
            return Err(StoreError::SourceChanged);
        }
        hash.update(&buffer[..n]);
    }
    if count != length {
        return Err(StoreError::SourceChanged);
    }
    Ok(hash.finish())
}

/// Returns whether the destination already holds the object; refuses a non-file.
pub(super) fn existing_object(dest: &Path) -> Result<bool, StoreError> {
    match is_regular_file(dest) {
        Ok(true) => Ok(true),
        Ok(false) => Err(StoreError::NotRegularFile(dest.to_path_buf())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(StoreError::Io {
            path: dest.to_path_buf(),
            source,
        }),
    }
}

/// Decides whether to compress, then rewinds the source to its start.
pub(super) fn choose_compression(
    input: &mut dyn ReadSeek,
    length: u64,
    source_path: &Path,
) -> Result<bool, StoreError> {
    let mut head = [0; MAGIC_SNIFF_BYTES];
    let n = input.read(&mut head).map_err(|source| StoreError::Io {
        path: source_path.into(),
        source,
    })?;
    let compressed = !object::already_compressed(&head[..n])
        && compression_probe_is_worthwhile(&mut *input, length, source_path)?;
    input
        .seek(SeekFrom::Start(0))
        .map_err(|source| StoreError::Io {
            path: source_path.into(),
            source,
        })?;
    Ok(compressed)
}

/// Publishes the temporary file, or removes it when another writer won the race.
pub(super) fn publish_or_discard(temp: TempFile, dest: &Path) -> Result<(), StoreError> {
    let temp_path = temp.path().to_path_buf();
    match is_regular_file(dest) {
        Ok(true) => temp.remove().map_err(|source| StoreError::Io {
            path: temp_path,
            source,
        }),
        Ok(false) => Err(StoreError::NotRegularFile(dest.to_path_buf())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            temp.publish(dest).map_err(|source| StoreError::Io {
                path: dest.to_path_buf(),
                source,
            })
        }
        Err(source) => Err(StoreError::Io {
            path: dest.to_path_buf(),
            source,
        }),
    }
}
