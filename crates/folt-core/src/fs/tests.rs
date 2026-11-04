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
fn sync_failure_matches_canonical_paths_after_directory_creation() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    let alias = root.join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let target = alias.join("directory");
    with_sync_failure_after(&target, 1, || {
        std::fs::create_dir(&target).unwrap();
        let canonical = target.canonicalize().unwrap();
        sync_directory(&canonical).unwrap();
        let error = sync_directory(&canonical).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("injected directory sync failure")
        );
    });
    sync_directory(&target).unwrap();
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
