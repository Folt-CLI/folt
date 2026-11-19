use super::*;
use crate::test_support::TempHome;

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
                RealFileSystem.rename_no_replace(from, to)
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
        RealFileSystem
            .create_symlink(Path::new("foreign target"), to)
            .unwrap();
        RealFileSystem.rename_no_replace(from, to)
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

#[test]
fn temporary_path_keeps_its_shape() {
    let parent = Path::new("parent");
    let path = temporary_path(parent, "folt-name-test");
    assert_eq!(path.parent(), Some(parent));
    let prefix = format!(".folt-name-test-{}-", std::process::id());
    let counter = path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix(&prefix)
        .unwrap();
    assert!(counter.parse::<u64>().is_ok());
}
