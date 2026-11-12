use super::*;
use crate::config::names::CONTENT_FILE;
use crate::test_support::TempHome;

mod links_and_conversion;
mod markers;
mod name_writes;
mod rollback;

#[test]
fn directory_resolution_failure_keeps_root_destination() {
    let root = Path::new("/");
    assert!(root.parent().is_none());
    assert_eq!(register_destination(true, root, false), root);
}

fn resolved_path(path: impl AsRef<Path>) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

fn injected() -> RegistryError {
    RegistryError::Io(io::Error::other("injected failure"))
}

#[test]
fn permission_error_maps_to_unreadable() {
    assert_eq!(
        io_refusal(&io::Error::from(io::ErrorKind::PermissionDenied)),
        RegisterRefusalReason::Unreadable
    );
}
