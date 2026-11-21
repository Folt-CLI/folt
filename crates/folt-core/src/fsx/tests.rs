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

#[test]
fn mutate_removals_match_std() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("entry");
    fs::write(&path, b"body").unwrap();
    remove_file(&path).unwrap();
    assert!(!path.exists());
    assert_eq!(
        remove_file(&path).unwrap_err().kind(),
        fs::remove_file(&path).unwrap_err().kind()
    );
    fs::create_dir(&path).unwrap();
    remove_dir(&path).unwrap();
    assert!(!path.exists());
    assert_eq!(
        remove_dir(&path).unwrap_err().kind(),
        fs::remove_dir(&path).unwrap_err().kind()
    );
    fs::create_dir_all(path.join("nested")).unwrap();
    fs::write(path.join("nested/file"), b"body").unwrap();
    remove_dir_all(&path).unwrap();
    assert!(!path.exists());
    fs::write(&path, b"body").unwrap();
    assert_eq!(
        remove_dir_all(&path).unwrap_err().kind(),
        fs::remove_dir_all(&path).unwrap_err().kind()
    );
}

#[test]
fn mutate_create_dir_all_matches_std() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/directory");
    create_dir_all(&path).unwrap();
    assert!(path.is_dir());
    create_dir_all(&path).unwrap();
    let file = path.join("file");
    fs::write(&file, b"body").unwrap();
    assert_eq!(
        create_dir_all(file.join("child")).unwrap_err().kind(),
        fs::create_dir_all(file.join("child")).unwrap_err().kind()
    );
}

#[test]
fn mutate_rename_and_hard_link_match_std() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let destination = dir.path().join("destination");
    fs::write(&source, b"body").unwrap();
    rename(&source, &destination).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&destination).unwrap(), b"body");
    assert_eq!(
        rename(&source, &destination).unwrap_err().kind(),
        fs::rename(&source, &destination).unwrap_err().kind()
    );
    hard_link(&destination, &source).unwrap();
    fs::write(&source, b"changed").unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"changed");
    assert_eq!(
        hard_link(&destination, &source).unwrap_err().kind(),
        fs::hard_link(&destination, &source).unwrap_err().kind()
    );
}

#[test]
fn mutate_permissions_and_sync_match_std() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("file");
    let file = fs::File::create(&path).unwrap();
    let original = file.metadata().unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    set_permissions(&path, readonly.clone()).unwrap();
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    fs::set_permissions(&path, original.clone()).unwrap();
    set_file_permissions(&file, readonly).unwrap();
    assert!(file.metadata().unwrap().permissions().readonly());
    set_file_permissions(&file, original.clone()).unwrap();
    sync_file(&file).unwrap();
    file.sync_all().unwrap();
    let missing = dir.path().join("missing");
    assert_eq!(
        set_permissions(&missing, original.clone())
            .unwrap_err()
            .kind(),
        fs::set_permissions(&missing, original).unwrap_err().kind()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn mutate_file_methods_preserve_errors() {
    // procfs rejects permission changes even for the file's owner.
    let file = fs::File::open("/proc/self/status").unwrap();
    let permissions = file.metadata().unwrap().permissions();
    assert_eq!(
        set_file_permissions(&file, permissions.clone())
            .unwrap_err()
            .kind(),
        file.set_permissions(permissions).unwrap_err().kind()
    );
    assert_eq!(
        sync_file(&file).unwrap_err().kind(),
        file.sync_all().unwrap_err().kind()
    );
}
