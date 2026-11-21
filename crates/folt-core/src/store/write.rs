use std::fs::File;
use std::io::{self, Cursor, Read, Write};
use std::path::Path;

use crate::config::limits::STREAM_BUFFER_SIZE;
use crate::config::names::{TEMP_PREFIX_INPUT, TEMP_PREFIX_OBJECT};
use crate::fsx::{TempFile, TempMode};
use crate::home::HomeLock;
use crate::id::ObjectId;
use crate::object::{self, ObjectError, ObjectHasher, ObjectKind, ObjectWriter};

use super::source::{choose_compression, existing_object, hash_source, publish_or_discard};
use super::{ObjectStore, ReadSeek, StoreError};

impl ObjectStore {
    pub fn put(
        &self,
        lock: &HomeLock,
        kind: ObjectKind,
        body: &[u8],
    ) -> Result<ObjectId, StoreError> {
        self.put_source(
            lock,
            kind,
            body.len() as u64,
            || Ok(Box::new(Cursor::new(body))),
            Path::new("<slice>"),
        )
    }
    pub fn put_commit(
        &self,
        lock: &HomeLock,
        commit: &object::Commit,
    ) -> Result<ObjectId, StoreError> {
        let body = commit.encode_body()?;
        let id = self.put(lock, ObjectKind::Commit, &body)?;
        if id != object::hash_object(ObjectKind::Commit, &body) {
            return Err(ObjectError::HashMismatch.into());
        }
        Ok(id)
    }
    pub fn put_tag(&self, lock: &HomeLock, tag: &object::Tag) -> Result<ObjectId, StoreError> {
        self.put(lock, ObjectKind::Tag, &tag.encode_body()?)
    }
    pub fn put_file(
        &self,
        lock: &HomeLock,
        kind: ObjectKind,
        path: &Path,
    ) -> Result<ObjectId, StoreError> {
        let len = File::open(path)
            .map_err(|source| StoreError::Io {
                path: path.into(),
                source,
            })?
            .metadata()
            .map_err(|source| StoreError::Io {
                path: path.into(),
                source,
            })?
            .len();
        self.put_source(
            lock,
            kind,
            len,
            || {
                File::open(path)
                    .map(|file| Box::new(file) as Box<dyn ReadSeek>)
                    .map_err(|source| StoreError::Io {
                        path: path.into(),
                        source,
                    })
            },
            path,
        )
    }
    /// Stores a non-seekable reader through a temporary spool so it can be read twice.
    pub fn put_reader<R: Read>(
        &self,
        _lock: &HomeLock,
        kind: ObjectKind,
        length: u64,
        mut reader: R,
    ) -> Result<ObjectId, StoreError> {
        let mut spool = TempFile::create(&self.root, TEMP_PREFIX_INPUT, '.', TempMode::Default)
            .map_err(|(path, source)| StoreError::Io { path, source })?;
        let spool_path = spool.path().to_path_buf();
        let mut count = 0u64;
        let mut buffer = vec![0; STREAM_BUFFER_SIZE];
        loop {
            let n = reader.read(&mut buffer).map_err(|source| StoreError::Io {
                path: spool_path.clone(),
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
            spool
                .file_mut()
                .write_all(&buffer[..n])
                .map_err(|source| StoreError::Io {
                    path: spool_path.clone(),
                    source,
                })?;
        }
        if count != length {
            return Err(StoreError::SourceChanged);
        }
        spool.close();
        self.put_source(
            _lock,
            kind,
            length,
            || {
                File::open(&spool_path)
                    .map(|f| Box::new(f) as Box<dyn ReadSeek>)
                    .map_err(|source| StoreError::Io {
                        path: spool_path.clone(),
                        source,
                    })
            },
            &spool_path,
        )
    }

    fn put_source<'a, F>(
        &self,
        _lock: &HomeLock,
        kind: ObjectKind,
        length: u64,
        mut fresh: F,
        source_path: &Path,
    ) -> Result<ObjectId, StoreError>
    where
        F: FnMut() -> Result<Box<dyn ReadSeek + 'a>, StoreError>,
    {
        let mut buffer = vec![0; STREAM_BUFFER_SIZE];
        let id = hash_source(&mut *fresh()?, kind, length, source_path, &mut buffer)?;
        let dest = self.path(id);
        if existing_object(&dest)? {
            return Ok(id);
        }
        let mut input = fresh()?;
        let compressed = choose_compression(&mut *input, length, source_path)?;
        let temp =
            self.write_temporary_object(&mut *input, kind, length, compressed, id, &mut buffer)?;
        publish_or_discard(temp, &dest)?;
        Ok(id)
    }

    /// Writes the encoded object to a new temporary file, verifying the hash as it goes.
    fn write_temporary_object(
        &self,
        input: &mut dyn ReadSeek,
        kind: ObjectKind,
        length: u64,
        compressed: bool,
        id: ObjectId,
        buffer: &mut [u8],
    ) -> Result<TempFile, StoreError> {
        let mut temp = TempFile::create(&self.root, TEMP_PREFIX_OBJECT, '.', TempMode::Default)
            .map_err(|(path, source)| StoreError::Io { path, source })?;
        let temp_path = temp.path().to_path_buf();
        let out = temp.take_file().ok_or_else(|| StoreError::Io {
            path: temp_path,
            source: io::Error::other("temporary object file handle is missing"),
        })?;
        let write_result = (|| {
            let mut writer = ObjectWriter::new(out, kind, length, compressed)?;
            let mut verify = ObjectHasher::new(kind, length);
            let mut total = 0u64;
            loop {
                let n = input.read(buffer).map_err(ObjectError::Io)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                verify.update(&buffer[..n]);
                writer.write_body(&buffer[..n])?;
            }
            if total != length || verify.finish() != id {
                return Err(ObjectError::InvalidFrame);
            }
            let (mut out, _) = writer.finish()?;
            out.flush().map_err(ObjectError::Io)?;
            crate::fsx::sync_file(&out).map_err(ObjectError::Io)?;
            Ok(())
        })();
        if let Err(error) = write_result {
            return Err(StoreError::Object(error));
        }
        Ok(temp)
    }
}
