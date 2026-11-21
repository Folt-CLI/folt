use super::*;
use crate::test_support::TempDir;

#[test]
fn syncs_a_real_directory() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().to_path_buf();
    let result = sync_directory(&path);
    result.unwrap();
}

#[cfg(unix)]
#[test]
fn propagates_directory_open_errors() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("missing");
    assert_eq!(
        sync_directory(&path).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}
