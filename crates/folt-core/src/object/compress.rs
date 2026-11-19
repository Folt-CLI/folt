//! Selects compression and encodes stored payloads; canonical framing lives elsewhere.

use std::io::Write;

use crate::config::limits::{
    COMPRESSION_PROBE_MIN_SAVING_PERCENT, COMPRESSION_PROBE_MIN_SIZE, COMPRESSION_SAMPLE_SIZE,
    COMPRESSION_SAMPLES, ZSTD_LEVEL,
};

use super::ObjectError;

pub(super) const UNCOMPRESSED: u8 = 0x00;
pub(super) const ZSTD: u8 = 0x01;
// These body prefixes identify formats that are already compressed.
const ALREADY_COMPRESSED_MAGIC: &[&[u8]] = &[
    b"\x1f\x8b",           // gzip
    b"\x28\xb5\x2f\xfd",   // zstd
    b"\xfd7zXZ\x00",       // xz
    b"BZh",                // bzip2
    b"PK\x03\x04",         // zip, docx, jar
    b"7z\xbc\xaf\x27\x1c", // 7z
    b"\x89PNG\r\n\x1a\n",  // PNG
    b"\xff\xd8\xff",       // JPEG
    b"GIF87a",
    b"GIF89a",
];

/// Adds a codec marker and stores a framed object, probing likely savings before compression.
pub fn compress(framed: &[u8]) -> Result<Vec<u8>, ObjectError> {
    use super::compression_probe_is_worthwhile;

    let body_start = framed
        .iter()
        .position(|byte| *byte == 0)
        .map(|index| index + 1);
    if body_start.is_some_and(|start| {
        ALREADY_COMPRESSED_MAGIC
            .iter()
            .any(|magic| framed[start..].starts_with(magic))
    }) || (framed.len() > COMPRESSION_PROBE_MIN_SIZE
        && !compression_probe_is_worthwhile(framed)?)
    {
        return store_uncompressed(framed);
    }

    let mut encoder = zstd::Encoder::new(Vec::new(), ZSTD_LEVEL).map_err(ObjectError::Io)?;
    encoder.write_all(framed).map_err(ObjectError::Io)?;
    let compressed = encoder.finish().map_err(ObjectError::Io)?;
    if compressed.len() < framed.len() {
        let mut stored = Vec::with_capacity(compressed.len() + 1);
        stored.push(ZSTD);
        stored.extend_from_slice(&compressed);
        Ok(stored)
    } else {
        store_uncompressed(framed)
    }
}

fn store_uncompressed(framed: &[u8]) -> Result<Vec<u8>, ObjectError> {
    let mut stored = Vec::with_capacity(framed.len() + 1);
    stored.push(UNCOMPRESSED);
    stored.extend_from_slice(framed);
    Ok(stored)
}

pub(crate) fn already_compressed(bytes: &[u8]) -> bool {
    ALREADY_COMPRESSED_MAGIC
        .iter()
        .any(|magic| bytes.starts_with(magic))
}

pub(crate) fn compression_probe_is_worthwhile(framed: &[u8]) -> Result<bool, ObjectError> {
    let sample_size = COMPRESSION_SAMPLE_SIZE.min(framed.len() / COMPRESSION_SAMPLES);
    let starts = [
        0,
        (framed.len() - sample_size) / 2,
        framed.len() - sample_size,
    ];
    compression_samples_are_worthwhile(starts.map(|start| &framed[start..start + sample_size]))
}

pub(crate) fn compression_samples_are_worthwhile(
    samples: [&[u8]; COMPRESSION_SAMPLES],
) -> Result<bool, ObjectError> {
    let mut sample_bytes = 0;
    let mut compressed_bytes = 0;
    for sample in samples {
        let compressed = zstd::encode_all(sample, 1).map_err(ObjectError::Io)?;
        sample_bytes += sample.len();
        compressed_bytes += compressed.len();
    }
    Ok(compressed_bytes * 100 < sample_bytes * (100 - COMPRESSION_PROBE_MIN_SAVING_PERCENT))
}
