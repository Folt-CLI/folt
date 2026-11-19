//! Encodes and decodes canonical frames and stored headers; full stored objects live elsewhere.

use std::io::Read;

use crate::config::limits::MAX_ZSTD_WINDOW_LOG;

use super::compress::{UNCOMPRESSED, ZSTD};
use super::stream::BoxReader;
use super::{ObjectError, ObjectKind};

/// Frame header capacity estimate in bytes.
const FRAME_HEADER_ESTIMATE: usize = 22;
/// Maximum frame header bytes read before the body; distinct from FRAME_SLACK's body-limit allowance.
const FRAME_HEADER_MAX: usize = 128;

/// Reads at most `limit` decoded body bytes, trusting the stored object id.
/// Unlike ObjectReader, this path neither hashes the prefix nor reads the tail.
pub fn read_prefix<R: Read + 'static>(
    stored: R,
    kind: ObjectKind,
    limit: usize,
) -> Result<Vec<u8>, ObjectError> {
    let (reader, header) = read_stored_header(stored)?;
    let length = decode_header(&header, kind)?.min(limit as u64);
    let mut bytes = Vec::new();
    reader
        .take(length)
        .read_to_end(&mut bytes)
        .map_err(ObjectError::Io)?;
    if bytes.len() as u64 != length {
        return Err(ObjectError::Truncated);
    }
    Ok(bytes)
}

/// Reads the stored codec and canonical header without verifying the body or hash.
pub fn peek_kind<R: Read + 'static>(stored: R) -> Result<ObjectKind, ObjectError> {
    let (_, header) = read_stored_header(stored)?;
    let kind = match header.split(|b| *b == b' ').next() {
        Some(b"blob") => ObjectKind::Blob,
        Some(b"commit") => ObjectKind::Commit,
        Some(b"tag") => ObjectKind::Tag,
        _ => return Err(ObjectError::InvalidFrame),
    };
    decode_header(&header, kind)?;
    Ok(kind)
}

pub(super) fn read_stored_header<R: Read + 'static>(
    mut stored: R,
) -> Result<(BoxReader, Vec<u8>), ObjectError> {
    let mut codec = [0];
    stored.read_exact(&mut codec).map_err(ObjectError::Io)?;
    let mut reader: BoxReader = match codec[0] {
        UNCOMPRESSED => Box::new(stored),
        ZSTD => {
            let mut decoder = zstd::stream::read::Decoder::new(stored).map_err(ObjectError::Io)?;
            decoder
                .window_log_max(MAX_ZSTD_WINDOW_LOG)
                .map_err(ObjectError::Io)?;
            Box::new(decoder)
        }
        value => return Err(ObjectError::UnknownCodec(value)),
    };
    let mut header = Vec::new();
    for _ in 0..FRAME_HEADER_MAX {
        let mut byte = [0];
        if reader.read(&mut byte).map_err(ObjectError::Io)? == 0 {
            break;
        }
        if byte[0] == 0 {
            return Ok((reader, header));
        }
        header.push(byte[0]);
    }
    Err(ObjectError::InvalidFrame)
}

pub(super) fn decode_header(header: &[u8], expected: ObjectKind) -> Result<u64, ObjectError> {
    let mut split = header.split(|b| *b == b' ');
    let kind = split.next().ok_or(ObjectError::InvalidFrame)?;
    let length = split.next().ok_or(ObjectError::InvalidFrame)?;
    if split.next().is_some()
        || kind != expected.as_bytes()
        || length.is_empty()
        || (length.len() > 1 && length[0] == b'0')
        || !length.iter().all(u8::is_ascii_digit)
    {
        return Err(ObjectError::InvalidFrame);
    }
    std::str::from_utf8(length)
        .map_err(|_| ObjectError::InvalidFrame)?
        .parse()
        .map_err(|_| ObjectError::InvalidFrame)
}

pub fn encode(kind: ObjectKind, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(kind.as_bytes().len() + FRAME_HEADER_ESTIMATE + body.len());
    bytes.extend_from_slice(kind.as_bytes());
    bytes.push(b' ');
    bytes.extend_from_slice(body.len().to_string().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(body);
    bytes
}

pub fn decode(framed: &[u8], expected: ObjectKind, limit: u64) -> Result<Vec<u8>, ObjectError> {
    let nul = framed
        .iter()
        .position(|b| *b == 0)
        .ok_or(ObjectError::InvalidFrame)?;
    let header = &framed[..nul];
    let mut split = header.split(|b| *b == b' ');
    let kind = split.next().ok_or(ObjectError::InvalidFrame)?;
    let length = split.next().ok_or(ObjectError::InvalidFrame)?;
    if split.next().is_some()
        || kind != expected.as_bytes()
        || length.is_empty()
        || (length.len() > 1 && length[0] == b'0')
        || !length.iter().all(u8::is_ascii_digit)
    {
        return Err(ObjectError::InvalidFrame);
    }
    let length = std::str::from_utf8(length)
        .map_err(|_| ObjectError::InvalidFrame)?
        .parse::<u64>()
        .map_err(|_| ObjectError::InvalidFrame)?;
    if length > limit {
        return Err(ObjectError::LimitExceeded);
    }
    let body = &framed[nul + 1..];
    if body.len() as u64 != length {
        return Err(ObjectError::InvalidFrame);
    }
    Ok(body.to_vec())
}
