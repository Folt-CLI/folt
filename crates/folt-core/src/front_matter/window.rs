use crate::config::limits::SCAN_WINDOW;

/// The bounded prefix used to inspect front matter.
pub(crate) fn scan_window(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.len().min(SCAN_WINDOW)]
}
