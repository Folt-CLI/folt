//! On-disk format identifiers.

/// Folt SQLite application identifier (32-bit integer).
pub(crate) const APPLICATION_ID: u32 = 0x464f_4c54;

/// Current SQLite user_version (schema revision number).
pub(crate) const SCHEMA_VERSION: i64 = 5;

/// SQLite file signature (16 bytes).
pub(crate) const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Object identifier algorithm name (ASCII text).
pub const OBJECT_ALGORITHM: &str = "sha256";
