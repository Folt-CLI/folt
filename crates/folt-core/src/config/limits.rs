//! Fixed sizes, thresholds, counts and timeouts.

/// Front matter scan limit in bytes; closing delimiter must start before it.
pub const SCAN_LIMIT: usize = 64 * 1024;

/// Extra bytes to recognize a delimiter crossing the scan limit.
pub(crate) const SCAN_SLACK: usize = 8;

/// Maximum front matter prefix read in bytes.
pub(crate) const SCAN_WINDOW: usize = SCAN_LIMIT + SCAN_SLACK;

/// Maximum folio-name length in ASCII bytes.
pub(crate) const FOLIO_NAME_MAX: usize = 64;

/// Object identifier length in hexadecimal ASCII bytes.
pub(crate) const HEX_ID_LEN: usize = 64;

/// Maximum user ref name length in ASCII bytes.
pub(crate) const REF_NAME_MAX: usize = 100;

/// Hyphenated Folio id length in ASCII bytes.
pub(crate) const UUID_TEXT_LEN: usize = 36;

/// Byte offset of the commit id after the Folio id and hyphen.
pub(crate) const PIN_COMMIT_OFFSET: usize = UUID_TEXT_LEN + 1;

/// Retention pin name length in ASCII bytes.
pub(crate) const PIN_NAME_LEN: usize = PIN_COMMIT_OFFSET + HEX_ID_LEN;

/// Maximum ref file read in bytes, including the newline.
pub(crate) const REF_FILE_MAX_BYTES: usize = HEX_ID_LEN + 1;

/// Measured streaming buffer size in bytes; bounds streaming object memory independent of object size.
pub(crate) const STREAM_BUFFER_SIZE: usize = 1024 * 1024;

/// Minimum framed size above which compression is probed, in bytes; small objects avoid probing cost.
pub(crate) const COMPRESSION_PROBE_MIN_SIZE: usize = 4 * 1024;

/// Maximum compression sample size in bytes; three bounded samples keep probing large objects cheap.
pub(crate) const COMPRESSION_SAMPLE_SIZE: usize = 64 * 1024;

/// Required saving in percent before full compression; at least this saving must be strictly exceeded.
pub(crate) const COMPRESSION_PROBE_MIN_SAVING_PERCENT: usize = 10;

/// Compression probe sample count; three bounded samples keep probing large objects cheap.
pub(crate) const COMPRESSION_SAMPLES: usize = 3;

/// Zstandard storage compression level (dimensionless).
pub(crate) const ZSTD_LEVEL: i32 = 3;

/// Maximum decoder window size as log2(bytes).
pub(crate) const MAX_ZSTD_WINDOW_LOG: u32 = 23;

/// Frame header allowance above the body limit in bytes.
pub(crate) const FRAME_SLACK: u64 = 128;

/// Default registration confirmation threshold in bytes.
pub const DEFAULT_LARGE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Default batch confirmation threshold in bytes (strictly exceeded).
pub const DEFAULT_BATCH_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// Maximum number of largest entries in a registration summary.
pub(crate) const SUMMARY_LARGEST: usize = 10;

/// Minimum drops requiring confirmation, also strictly more than half the scoped rows.
pub(crate) const PRUNE_CONFIRM_MIN: usize = 5;

/// Default SQLite busy timeout in seconds.
pub(crate) const DEFAULT_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
