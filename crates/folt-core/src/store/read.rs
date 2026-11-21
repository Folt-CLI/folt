use std::fs::File;
use std::io::Write;

use crate::config::limits::STREAM_BUFFER_SIZE;
use crate::home::is_regular_file;
use crate::id::ObjectId;
use crate::object::{self, ObjectError, ObjectKind, ObjectReader};

use super::{ObjectStore, StoreError};

impl ObjectStore {
    pub fn get_commit(&self, id: ObjectId, limit: u64) -> Result<object::Commit, StoreError> {
        Ok(object::Commit::parse_body(&self.get(
            id,
            ObjectKind::Commit,
            limit,
        )?)?)
    }
    pub fn get_tag(&self, id: ObjectId, limit: u64) -> Result<object::Tag, StoreError> {
        Ok(object::Tag::parse_body(&self.get(
            id,
            ObjectKind::Tag,
            limit,
        )?)?)
    }
    /// The destination may contain provisional bytes if verification fails; publish it only after `Ok`.
    pub fn get_to<W: Write>(
        &self,
        id: ObjectId,
        kind: ObjectKind,
        mut output: W,
    ) -> Result<(), StoreError> {
        let path = self.path(id);
        if !is_regular_file(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })? {
            return Err(StoreError::NotRegularFile(path));
        }
        let file = File::open(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?;
        let mut reader = ObjectReader::new(file, id, kind)?;
        let mut buffer = vec![0; STREAM_BUFFER_SIZE];
        loop {
            let n = reader.read_body(&mut buffer)?;
            if n == 0 {
                break;
            }
            output
                .write_all(&buffer[..n])
                .map_err(|source| StoreError::Io {
                    path: path.clone(),
                    source,
                })?;
        }
        reader.finish()?;
        Ok(())
    }
    /// Returns a bounded decoded prefix without rehashing or verifying the tail.
    pub fn get_prefix(
        &self,
        id: ObjectId,
        kind: ObjectKind,
        limit: usize,
    ) -> Result<Vec<u8>, StoreError> {
        let path = self.path(id);
        if !is_regular_file(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })? {
            return Err(StoreError::NotRegularFile(path));
        }
        let file = File::open(&path).map_err(|source| StoreError::Io { path, source })?;
        Ok(object::read_prefix(file, kind, limit)?)
    }
    pub fn get(&self, id: ObjectId, kind: ObjectKind, limit: u64) -> Result<Vec<u8>, StoreError> {
        let path = self.path(id);
        if !is_regular_file(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })? {
            return Err(StoreError::NotRegularFile(path));
        }
        let file = File::open(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?;
        let mut reader = ObjectReader::new(file, id, kind)?;
        let mut out = Vec::new();
        let mut buffer =
            vec![0; STREAM_BUFFER_SIZE.min(usize::try_from(limit).unwrap_or(usize::MAX).max(1))];
        loop {
            let count = reader.read_body(&mut buffer)?;
            if count == 0 {
                break;
            }
            if out.len() as u64 + count as u64 > limit {
                return Err(StoreError::Object(ObjectError::LimitExceeded));
            }
            out.extend_from_slice(&buffer[..count]);
        }
        reader.finish()?;
        Ok(out)
    }
}
