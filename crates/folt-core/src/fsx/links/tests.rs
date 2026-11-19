use super::*;
use crate::test_support::TempDir;

#[test]
fn single_link_is_required() {
    assert!(ensure_single(1).is_ok());
    for count in [0, 2, 3, u64::MAX] {
        let error = ensure_single(count).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "source has other hard links");
    }
}

#[test]
fn missing_link_count_returns_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing");
    assert_eq!(
        link_count(&path).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        require_single_link(&path).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}
