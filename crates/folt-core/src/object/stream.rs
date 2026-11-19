//! Streams stored object bodies and verifies their hashes; header parsing lives in frame.

use std::io::{Read, Write};

use crate::config::limits::ZSTD_LEVEL;
use crate::id::ObjectId;

use super::compress::{UNCOMPRESSED, ZSTD};
use super::frame::{decode_header, read_stored_header};
use super::{ObjectError, ObjectHasher, ObjectKind};

/// Streams a framed body into its stored representation while hashing it.
pub struct ObjectWriter<W: Write> {
    output: Option<W>,
    encoder: Option<zstd::Encoder<'static, W>>,
    hasher: ObjectHasher,
    expected: u64,
    written: u64,
}

impl<W: Write> ObjectWriter<W> {
    pub fn new(
        mut output: W,
        kind: ObjectKind,
        length: u64,
        compressed: bool,
    ) -> Result<Self, ObjectError> {
        let header = [kind.as_bytes(), b" ", length.to_string().as_bytes(), &[0]].concat();
        let hasher = ObjectHasher::new(kind, length);
        if compressed {
            output.write_all(&[ZSTD]).map_err(ObjectError::Io)?;
            let mut encoder = zstd::Encoder::new(output, ZSTD_LEVEL).map_err(ObjectError::Io)?;
            encoder.write_all(&header).map_err(ObjectError::Io)?;
            Ok(Self {
                output: None,
                encoder: Some(encoder),
                hasher,
                expected: length,
                written: 0,
            })
        } else {
            output
                .write_all(&[UNCOMPRESSED])
                .and_then(|()| output.write_all(&header))
                .map_err(ObjectError::Io)?;
            Ok(Self {
                output: Some(output),
                encoder: None,
                hasher,
                expected: length,
                written: 0,
            })
        }
    }
    pub fn write_body(&mut self, bytes: &[u8]) -> Result<(), ObjectError> {
        self.written = self
            .written
            .checked_add(bytes.len() as u64)
            .ok_or(ObjectError::InvalidFrame)?;
        if self.written > self.expected {
            return Err(ObjectError::InvalidFrame);
        }
        self.hasher.update(bytes);
        if let Some(encoder) = &mut self.encoder {
            encoder.write_all(bytes).map_err(ObjectError::Io)
        } else {
            self.output
                .as_mut()
                .ok_or(ObjectError::InvalidFrame)?
                .write_all(bytes)
                .map_err(ObjectError::Io)
        }
    }
    pub fn finish(mut self) -> Result<(W, ObjectId), ObjectError> {
        if self.written != self.expected {
            return Err(ObjectError::Truncated);
        }
        let output = if let Some(encoder) = self.encoder.take() {
            encoder.finish().map_err(ObjectError::Io)?
        } else {
            self.output.take().ok_or(ObjectError::InvalidFrame)?
        };
        Ok((output, self.hasher.finish()))
    }
}

/// Streams the verified body; callers must treat output as provisional until `finish` succeeds.
pub struct ObjectReader {
    reader: BoxReader,
    hasher: ObjectHasher,
    expected: u64,
    read: u64,
    id: ObjectId,
}

impl ObjectReader {
    pub fn new<R: Read + 'static>(
        stored: R,
        id: ObjectId,
        kind: ObjectKind,
    ) -> Result<Self, ObjectError> {
        let (reader, header) = read_stored_header(stored)?;
        let expected = decode_header(&header, kind)?;
        Ok(Self {
            reader,
            hasher: ObjectHasher::new(kind, expected),
            expected,
            read: 0,
            id,
        })
    }
    pub fn read_body(&mut self, output: &mut [u8]) -> Result<usize, ObjectError> {
        let allowance = output
            .len()
            .min(usize::try_from(self.expected.saturating_sub(self.read)).unwrap_or(usize::MAX));
        if allowance == 0 {
            let mut extra = [0];
            if self.reader.read(&mut extra).map_err(ObjectError::Io)? != 0 {
                return Err(ObjectError::InvalidFrame);
            }
            return Ok(0);
        }
        let count = self
            .reader
            .read(&mut output[..allowance])
            .map_err(ObjectError::Io)?;
        self.read += count as u64;
        self.hasher.update(&output[..count]);
        Ok(count)
    }
    pub fn finish(mut self) -> Result<(), ObjectError> {
        if self.read != self.expected {
            return Err(ObjectError::Truncated);
        }
        let mut extra = [0];
        if self.reader.read(&mut extra).map_err(ObjectError::Io)? != 0 {
            return Err(ObjectError::InvalidFrame);
        }
        if self.hasher.finish() != self.id {
            return Err(ObjectError::HashMismatch);
        }
        Ok(())
    }
}

pub(super) type BoxReader = Box<dyn Read>;
