use std::path::Path;

use super::register_destination;

#[test]
fn directory_resolution_failure_keeps_root_destination() {
    let root = Path::new("/");
    assert!(root.parent().is_none());
    assert_eq!(register_destination(true, root, false), root);
}
