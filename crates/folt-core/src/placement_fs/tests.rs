use super::*;
use crate::test_support::TempHome;

#[test]
fn rewrite_publication_signature_matches_renamed_file() {
    let home = TempHome::new().unwrap();
    let path = home.root.join("file.md");
    fs::write(&path, b"body").unwrap();
    let expected = signature(&fs::metadata(&path).unwrap());
    let mut published = None;
    atomic_rewrite(&path, b"rewritten body", &expected, |value| {
        published = Some(value);
    })
    .unwrap();
    assert_eq!(published, Some(signature(&fs::metadata(&path).unwrap())));
}

#[cfg(unix)]
#[test]
fn rewrite_temp_is_private_before_writing_and_rewrite_preserves_mode() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempHome::new().unwrap();
    let source = home.root.join("private.md");
    fs::write(&source, b"private content\n").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    let metadata = fs::metadata(&source).unwrap();
    let temporary = temporary_path(&home.root, "folt-name-test");
    let output = create_rewrite_temp(&temporary, &metadata.permissions()).unwrap();
    let created = output.metadata().unwrap();
    assert_eq!(created.len(), 0);
    assert_eq!(created.permissions().mode() & 0o7777, 0o600);
    drop(output);
    fs::remove_file(&temporary).unwrap();

    let rewritten = b"---\nfolio-name: \"private\"\n---\nprivate content\n";
    atomic_rewrite(&source, rewritten, &signature(&metadata), |_| {}).unwrap();
    assert_eq!(fs::read(&source).unwrap(), rewritten);
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o7777,
        0o600
    );
}

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
fn unreadable_link_count_refuses_conversion() {
    let home = TempHome::new().unwrap();
    let path = home.root.join("missing");
    let error = require_single_link(&path).unwrap_err();
    let PlacementError::OperationFailed {
        path: failed_path,
        source,
    } = error
    else {
        panic!("wrong error")
    };
    assert_eq!(failed_path, path);
    assert_eq!(source.kind(), io::ErrorKind::NotFound);
}

#[test]
fn no_clobber_rename_refuses_destinations_created_after_absence_check() {
    for directory in [false, true] {
        for target_directory in [false, true] {
            let home = TempHome::new().unwrap();
            let placement = home.source(directory);
            let fs = PlacementFs::new(&home.home);
            let id = FolioId::from_bytes([38; 16]);
            let observation = fs.read(id, &placement).unwrap();
            let to = home.root.join("destination");
            let result = fs.relocate_with_rename(id, &placement, &to, &observation, |from, to| {
                if target_directory {
                    std::fs::create_dir(to).unwrap();
                } else {
                    std::fs::write(to, b"foreign file").unwrap();
                }
                rename_no_replace(from, to)
            });
            assert!(result.is_err());
            assert_eq!(placement.path.is_dir(), directory);
            assert_eq!(to.is_dir(), target_directory);
            if !directory {
                assert_eq!(std::fs::read(&placement.path).unwrap(), b"source");
            }
            if !target_directory {
                assert_eq!(std::fs::read(to).unwrap(), b"foreign file");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn no_clobber_rename_refuses_a_link_created_after_absence_check() {
    let home = TempHome::new().unwrap();
    let placement = home.source(false);
    let fs = PlacementFs::new(&home.home);
    let id = FolioId::from_bytes([38; 16]);
    let observation = fs.read(id, &placement).unwrap();
    let to = home.root.join("destination");
    let result = fs.relocate_with_rename(id, &placement, &to, &observation, |from, to| {
        create_symlink(Path::new("foreign target"), to).unwrap();
        rename_no_replace(from, to)
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&placement.path).unwrap(), b"source");
    assert_eq!(std::fs::read_link(to).unwrap(), Path::new("foreign target"));
}

#[test]
fn cross_device_and_permission_errors_are_reported_without_moving() {
    for directory in [false, true] {
        for kind in [
            io::ErrorKind::CrossesDevices,
            io::ErrorKind::PermissionDenied,
        ] {
            let home = TempHome::new().unwrap();
            let placement = home.source(directory);
            let fs = PlacementFs::new(&home.home);
            let id = FolioId::from_bytes([38; 16]);
            let observation = fs.read(id, &placement).unwrap();
            let to = home.root.join("destination");
            let error = fs
                .relocate_with_rename(id, &placement, &to, &observation, |_, _| Err(kind.into()))
                .unwrap_err();
            if kind == io::ErrorKind::CrossesDevices {
                assert!(
                    error
                        .to_string()
                        .contains("moves across filesystems are not supported")
                );
            }
            let PlacementError::OperationFailed { source, .. } = error else {
                panic!("wrong error")
            };
            assert_eq!(source.kind(), kind);
            assert!(placement.path.exists());
            assert!(!to.exists());
        }
    }
    #[cfg(unix)]
    {
        let error = move_failure(
            Path::new("destination"),
            io::Error::from_raw_os_error(rustix::io::Errno::XDEV.raw_os_error()),
        );
        assert!(
            error
                .to_string()
                .contains("moves across filesystems are not supported")
        );
    }
}
