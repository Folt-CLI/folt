use super::*;
use crate::test_support::TempDir;

#[cfg(unix)]
#[test]
fn sync_failure_matches_canonical_paths_after_directory_creation() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    let alias = root.join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let target = alias.join("directory");
    let fs = FaultFileSystem::new().sync_failure(&target, 1);
    std::fs::create_dir(&target).unwrap();
    let canonical = target.canonicalize().unwrap();
    fs.sync_directory(&canonical).unwrap();
    let error = fs.sync_directory(&canonical).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected directory sync failure")
    );
    RealFileSystem.sync_directory(&target).unwrap();
}

#[test]
fn sync_failure_counts_only_successful_matching_calls_and_keeps_failing() {
    let directory = TempDir::new().unwrap();
    let target = directory.path().join("target");
    let fs = FaultFileSystem::new().sync_failure(&target, 2);
    fs.sync_directory(directory.path()).unwrap();
    #[cfg(unix)]
    assert_eq!(
        fs.sync_directory(&target).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    std::fs::create_dir(&target).unwrap();
    fs.sync_directory(&target).unwrap();
    fs.sync_directory(&target).unwrap();
    for _ in 0..2 {
        assert_eq!(
            fs.sync_directory(&target).unwrap_err().to_string(),
            "injected directory sync failure"
        );
    }
    fs.sync_directory(directory.path()).unwrap();
    FaultFileSystem::new().sync_directory(&target).unwrap();
}

#[test]
fn rewrite_metadata_fault_fires_once_for_the_matching_path() {
    let directory = TempDir::new().unwrap();
    let target = directory.path().join("target");
    let other = directory.path().join("other");
    std::fs::write(&target, b"body").unwrap();
    let output = File::open(&target).unwrap();
    let fs = FaultFileSystem::new().rewrite_metadata_failure(&target);
    fs.rewrite_metadata(&output, &other).unwrap();
    assert_eq!(
        fs.rewrite_metadata(&output, &target)
            .unwrap_err()
            .to_string(),
        "injected rewrite metadata failure"
    );
    assert_eq!(fs.rewrite_metadata(&output, &target).unwrap().len(), 4);
}
