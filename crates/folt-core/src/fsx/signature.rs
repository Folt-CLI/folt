use std::fs;

/// File metadata captured with a copy's base and checked before applying a move.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub size: u64,
    pub mtime: Option<i128>,
    pub ctime: Option<i128>,
    pub inode: Option<u64>,
}

/// Nanoseconds per second (nanoseconds).
#[cfg(unix)]
const NANOS_PER_SECOND: i128 = 1_000_000_000;
/// Windows-to-Unix epoch offset in FILETIME ticks.
#[cfg(windows)]
const WINDOWS_TO_UNIX_EPOCH_TICKS: i128 = 116_444_736_000_000_000;
/// Nanoseconds per FILETIME tick (nanoseconds).
#[cfg(windows)]
const NANOS_PER_TICK: i128 = 100;
pub fn signature(metadata: &fs::Metadata) -> Signature {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    #[cfg(unix)]
    let (mtime, ctime, inode) = (
        Some(metadata.mtime() as i128 * NANOS_PER_SECOND + metadata.mtime_nsec() as i128),
        Some(metadata.ctime() as i128 * NANOS_PER_SECOND + metadata.ctime_nsec() as i128),
        Some(metadata.ino()),
    );
    #[cfg(windows)]
    let (mtime, ctime, inode) = {
        use std::os::windows::fs::MetadataExt;
        // FILETIME uses 100 ns ticks since 1601; registry timestamps use Unix ns.
        let unix_ns = |ticks| (i128::from(ticks) - WINDOWS_TO_UNIX_EPOCH_TICKS) * NANOS_PER_TICK;
        (
            Some(unix_ns(metadata.last_write_time())),
            Some(unix_ns(metadata.creation_time())),
            None,
        )
    };
    Signature {
        size: metadata.len(),
        mtime,
        ctime,
        inode,
    }
}

#[cfg(test)]
mod tests;
