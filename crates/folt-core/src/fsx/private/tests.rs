#[cfg(unix)]
#[test]
fn builders_create_private_entries() {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::test_support::TempDir::new().unwrap();
    let path = dir.path().join("private");
    private_directory_builder().create(&path).unwrap();
    let file = private_file_options()
        .write(true)
        .create_new(true)
        .open(path.join("file"))
        .unwrap();
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}
