use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::config::limits::{
    COMPRESSION_PROBE_MIN_SIZE, COMPRESSION_SAMPLE_SIZE, COMPRESSION_SAMPLES,
};
use crate::home::is_regular_file;
use crate::id::ObjectId;
use crate::object::{self, ObjectKind};

use super::{ObjectStore, StoreError};

/// Prefix length in bytes read to sniff compressed magic.
pub(super) const MAGIC_SNIFF_BYTES: usize = 8;

impl ObjectStore {
    pub fn contains(&self, id: ObjectId) -> Result<bool, StoreError> {
        match fs::metadata(self.path(id)) {
            Ok(meta) => Ok(meta.is_file()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(StoreError::Io {
                path: self.root.clone(),
                source,
            }),
        }
    }
    /// Reads only the codec and header; this does not verify the object's body or hash.
    pub fn kind_of(&self, id: ObjectId) -> Result<ObjectKind, StoreError> {
        let path = self.path(id);
        if !is_regular_file(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })? {
            return Err(StoreError::NotRegularFile(path));
        }
        let file = File::open(&path).map_err(|source| StoreError::Io { path, source })?;
        object::peek_kind(file).map_err(StoreError::Object)
    }
}

pub(super) fn compression_probe_is_worthwhile(
    file: &mut (impl Read + Seek + ?Sized),
    length: u64,
    path: &Path,
) -> Result<bool, StoreError> {
    if length as usize <= COMPRESSION_PROBE_MIN_SIZE {
        return Ok(true);
    }
    let sample_size = COMPRESSION_SAMPLE_SIZE.min((length / COMPRESSION_SAMPLES as u64) as usize);
    let starts = [
        0,
        (length - sample_size as u64) / 2,
        length - sample_size as u64,
    ];
    let mut samples = Vec::with_capacity(COMPRESSION_SAMPLES);
    for start in starts {
        file.seek(SeekFrom::Start(start))
            .map_err(|source| StoreError::Io {
                path: path.into(),
                source,
            })?;
        let mut sample = vec![0; sample_size];
        file.read_exact(&mut sample)
            .map_err(|source| StoreError::Io {
                path: path.into(),
                source,
            })?;
        samples.push(sample);
    }
    object::compression_samples_are_worthwhile([&samples[0], &samples[1], &samples[2]])
        .map_err(StoreError::Object)
}
