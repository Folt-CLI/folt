use std::path::Path;

use crate::config::limits::SCAN_WINDOW;

use super::super::{applies_to, scan_window};

#[test]
fn markdown_extensions_are_case_insensitive_and_use_only_the_extension() {
    for path in ["a.md", "a.MD", "a.markdown", "a.Markdown"] {
        assert!(applies_to(Path::new(path)), "{path}");
    }
    for path in ["a.txt", "a", "a.md.bak", ".md"] {
        assert!(!applies_to(Path::new(path)), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_extensions_do_not_apply() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    assert!(!applies_to(Path::new(OsStr::from_bytes(b"a.\xff"))));
}

#[test]
fn window_retains_the_leading_bytes_up_to_the_bound() {
    for length in [0, 1, SCAN_WINDOW, SCAN_WINDOW + 1, SCAN_WINDOW * 3] {
        let bytes: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
        let window = scan_window(&bytes);
        let expected_length = if length <= SCAN_WINDOW {
            length
        } else {
            SCAN_WINDOW
        };
        assert_eq!(window, &bytes[..expected_length]);
        assert_eq!(window.as_ptr(), bytes.as_ptr());
    }
}
