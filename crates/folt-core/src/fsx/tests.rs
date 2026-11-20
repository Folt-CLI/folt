use super::*;
use crate::test_support::TempDir;
use std::fs;

#[test]
fn real_sync_matches_module() {
    let dir = TempDir::new().unwrap();
    sync::sync_directory(dir.path()).unwrap();
    RealFileSystem.sync_directory(dir.path()).unwrap();
    let missing = dir.path().join("missing");
    assert_eq!(
        RealFileSystem
            .sync_directory(&missing)
            .map_err(|e| e.kind()),
        sync::sync_directory(&missing).map_err(|e| e.kind())
    );
}

#[test]
fn real_rename_matches_module() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let destination = dir.path().join("destination");
    fs::write(&source, b"body").unwrap();
    RealFileSystem
        .rename_no_replace(&source, &destination)
        .unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"body");
    rename::rename_no_replace(&destination, &source).unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"body");
    fs::write(&destination, b"foreign").unwrap();
    assert_eq!(
        RealFileSystem
            .rename_no_replace(&source, &destination)
            .unwrap_err()
            .kind(),
        rename::rename_no_replace(&source, &destination)
            .unwrap_err()
            .kind()
    );
}

#[test]
fn real_symlink_matches_module() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target");
    fs::write(&target, b"body").unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let module = links::create_symlink(&target, &first);
    let real = RealFileSystem.create_symlink(&target, &second);
    assert_eq!(
        module.as_ref().err().map(|e| e.kind()),
        real.as_ref().err().map(|e| e.kind())
    );
    if module.is_ok() {
        assert_eq!(
            fs::read_link(first).unwrap(),
            fs::read_link(second).unwrap()
        );
    }
}

#[test]
fn real_link_count_matches_module() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("file");
    fs::write(&path, b"body").unwrap();
    assert_eq!(
        RealFileSystem.link_count(&path).unwrap(),
        links::link_count(&path).unwrap()
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        RealFileSystem.link_count(&path).unwrap_err().kind(),
        links::link_count(&path).unwrap_err().kind()
    );
}

#[test]
fn accepts_a_filesystem_implementation() {
    fn sync(fs: &impl FileSystem, path: &Path) -> io::Result<()> {
        fs.sync_directory(path)
    }
    let dir = TempDir::new().unwrap();
    sync(&RealFileSystem, dir.path()).unwrap();
}
