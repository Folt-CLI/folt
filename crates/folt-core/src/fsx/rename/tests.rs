use super::*;

#[test]
fn rename_preserves_existing_destinations() {
    for directory in [false, true] {
        let dir = crate::test_support::TempDir::new().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::write(&source, b"source").unwrap();
        if directory {
            std::fs::create_dir(&destination).unwrap();
        } else {
            std::fs::write(&destination, b"destination").unwrap();
        }
        assert_eq!(
            rename_no_replace(&source, &destination).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(source).unwrap(), b"source");
        if !directory {
            assert_eq!(std::fs::read(destination).unwrap(), b"destination");
        }
    }
}
