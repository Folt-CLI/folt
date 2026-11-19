//! Opens complete stored objects and verifies their hashes; streaming lives elsewhere.

use std::io::{self, Read};

use crate::config::limits::{FRAME_SLACK, MAX_ZSTD_WINDOW_LOG};
use crate::id::ObjectId;

use super::compress::{UNCOMPRESSED, ZSTD};
use super::{ObjectError, ObjectKind, decode, hash_object};

pub fn open_stored(stored: &[u8], expected: ObjectId, limit: u64) -> Result<Vec<u8>, ObjectError> {
    let (&codec, payload) = stored.split_first().ok_or(ObjectError::InvalidFrame)?;
    let framed = match codec {
        UNCOMPRESSED => {
            if payload.len() as u64 > limit.saturating_add(FRAME_SLACK) {
                return Err(ObjectError::LimitExceeded);
            }
            payload.to_vec()
        }
        ZSTD => {
            let cursor = io::Cursor::new(payload);
            let mut decoder = zstd::stream::read::Decoder::new(cursor).map_err(ObjectError::Io)?;
            decoder
                .window_log_max(MAX_ZSTD_WINDOW_LOG)
                .map_err(ObjectError::Io)?;
            let mut output = Vec::new();
            decoder
                .take(limit.saturating_add(FRAME_SLACK).saturating_add(1))
                .read_to_end(&mut output)
                .map_err(ObjectError::Io)?;
            if output.len() as u64 > limit.saturating_add(FRAME_SLACK) {
                return Err(ObjectError::LimitExceeded);
            }
            output
        }
        other => return Err(ObjectError::UnknownCodec(other)),
    };
    let (kind, body) = parse_frame(&framed, limit)?;
    if hash_object(kind, &body) != expected {
        return Err(ObjectError::HashMismatch);
    }
    Ok(framed)
}

fn parse_frame(framed: &[u8], limit: u64) -> Result<(ObjectKind, Vec<u8>), ObjectError> {
    for kind in [ObjectKind::Blob, ObjectKind::Commit, ObjectKind::Tag] {
        if let Ok(body) = decode(framed, kind, limit) {
            return Ok((kind, body));
        }
    }
    Err(ObjectError::InvalidFrame)
}
