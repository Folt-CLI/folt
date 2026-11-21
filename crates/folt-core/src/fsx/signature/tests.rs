use super::*;

#[test]
fn signature_captures_file_metadata() {
    let dir = crate::test_support::TempDir::new().unwrap();
    let path = dir.path().join("file");
    fs::write(&path, b"body").unwrap();
    let metadata = fs::metadata(path).unwrap();
    let captured = signature(&metadata);
    assert_eq!(captured.size, 4);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(captured.inode, Some(metadata.ino()));
        assert_eq!(
            captured.mtime,
            Some(
                i128::from(metadata.mtime()) * NANOS_PER_SECOND + i128::from(metadata.mtime_nsec())
            )
        );
        assert_eq!(
            captured.ctime,
            Some(
                i128::from(metadata.ctime()) * NANOS_PER_SECOND + i128::from(metadata.ctime_nsec())
            )
        );
    }
}
