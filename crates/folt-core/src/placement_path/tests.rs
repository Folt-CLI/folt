use super::*;
use crate::fsx::{FileSystem, RealFileSystem, hard_links_unsupported};
use crate::test_support::{TempDir, expected_path};

#[test]
fn directory_resolution_appends_marker_only_when_requested() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("directory");
    fs::create_dir(&path).unwrap();
    assert_eq!(
        PlacementPath::resolve(&path).unwrap().to_path_buf(),
        expected_path(&path.join(MARKER_FILE))
    );
    assert_eq!(
        PlacementPath::resolve_directory(&path)
            .unwrap()
            .to_path_buf(),
        expected_path(&path)
    );
}

#[test]
fn existing_ancestor_resolution_retains_missing_parent_and_leaf() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("missing-parent").join("missing-leaf");
    assert_eq!(
        resolve_existing_ancestor(&path).unwrap(),
        fs::canonicalize(directory.path())
            .unwrap()
            .join("missing-parent")
            .join("missing-leaf")
    );
    assert!(matches!(
        PlacementPath::resolve(&path),
        Err(RegistryError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound
    ));
}

#[test]
fn only_existing_ancestor_resolution_follows_final_symlink() {
    let directory = TempDir::new().unwrap();
    let target = directory.path().join("target");
    fs::write(&target, b"content").unwrap();
    let path = directory.path().join("symlink");
    if let Err(error) = RealFileSystem.create_symlink(&target, &path) {
        if hard_links_unsupported(&error) {
            return;
        }
        panic!("cannot create symbolic link: {error}");
    }
    assert_eq!(
        PlacementPath::resolve(&path).unwrap().to_path_buf(),
        expected_path(&path)
    );
    assert_eq!(
        PlacementPath::resolve_directory(&path)
            .unwrap()
            .to_path_buf(),
        expected_path(&path)
    );
    assert_eq!(
        resolve_existing_ancestor(&path).unwrap(),
        fs::canonicalize(&target).unwrap()
    );
}

#[test]
fn directory_prefix_preserves_resolved_bytes() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("directory");
    fs::create_dir(&path).unwrap();
    let resolved = PlacementPath::resolve_directory(path).unwrap();
    assert_eq!(PlacementPrefix::from(&resolved).0, resolved.as_bytes());
}

#[test]
fn existing_ancestor_resolution_reports_input_below_regular_file() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("file");
    fs::write(&file, b"content").unwrap();
    let input = file.join("missing-parent").join("leaf");
    let expected_kind = fs::canonicalize(&input).unwrap_err().kind();
    let PlacementError::OperationFailed { path, source } =
        resolve_existing_ancestor(&input).unwrap_err()
    else {
        panic!("wrong error");
    };
    assert_eq!(path, input);
    assert_eq!(source.kind(), expected_kind);
}

#[test]
fn resolution_without_parent_keeps_invalid_path_errors() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().ancestors().last().unwrap();
    assert!(matches!(
        PlacementPath::resolve_directory(root),
        Err(RegistryError::InvalidPath(path)) if path == root
    ));
    // Resolve appends a marker for a directory, including the filesystem root.
    assert_eq!(
        PlacementPath::resolve(root).unwrap().to_path_buf(),
        root.join(MARKER_FILE)
    );
}
