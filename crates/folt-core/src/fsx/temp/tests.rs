use super::*;

#[test]
fn names_have_the_requested_shape_and_are_unique() {
    let first = temp_name(".ref.", '.');
    let second = temp_name(".ref.", '.');
    let prefix = format!(".ref.{}.", std::process::id());
    assert!(first.strip_prefix(&prefix).unwrap().parse::<u64>().is_ok());
    assert!(second.strip_prefix(&prefix).unwrap().parse::<u64>().is_ok());
    assert_ne!(first, second);
}

use crate::test_support::TempDir;
use std::io::Write;

#[test]
fn create_uses_requested_name_and_drop_removes_file() {
    let directory = TempDir::new().unwrap();
    for separator in ['.', '-'] {
        let temporary =
            TempFile::create(directory.path(), ".test.", separator, TempMode::Default).unwrap();
        let path = temporary.path().to_path_buf();
        let prefix = format!(".test.{}{separator}", std::process::id());
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .strip_prefix(&prefix)
                .unwrap()
                .parse::<u64>()
                .is_ok()
        );
        assert!(path.is_file());
        drop(temporary);
        assert!(!path.exists());
    }
}

#[test]
fn publish_keeps_written_bytes_at_target() {
    let directory = TempDir::new().unwrap();
    let target = directory.path().join("published");
    let mut temporary = TempFile::create(directory.path(), "", '.', TempMode::Private).unwrap();
    let source = temporary.path().to_path_buf();
    temporary.file_mut().write_all(b"written bytes").unwrap();
    temporary.sync().unwrap();
    temporary.publish(&target).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(target).unwrap(), b"written bytes");
}

#[test]
fn failed_publish_removes_temporary_file() {
    let directory = TempDir::new().unwrap();
    let target = directory.path().join("directory");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("child"), b"existing").unwrap();
    let temporary = TempFile::create(directory.path(), "", '.', TempMode::Default).unwrap();
    let source = temporary.path().to_path_buf();
    assert!(temporary.publish(&target).is_err());
    assert!(!source.exists());
    assert_eq!(fs::read(target.join("child")).unwrap(), b"existing");
}

#[test]
fn keep_preserves_exact_path_and_create_at_does_not_replace_it() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("exact");
    let mut temporary = TempFile::create_at(&path, TempMode::Private).unwrap();
    temporary.file_mut().write_all(b"kept").unwrap();
    temporary.keep();
    assert_eq!(fs::read(&path).unwrap(), b"kept");
    assert!(matches!(
        TempFile::create_at(&path, TempMode::Default),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&path).unwrap(), b"kept");
}

#[test]
fn create_retries_existing_names_without_removing_them() {
    let directory = TempDir::new().unwrap();
    let first = NEXT_TEMP.load(Ordering::Relaxed);
    // Cover counters consumed by the other concurrently running temporary-file tests too.
    let end = first + 256;
    for counter in first..end {
        let path = directory
            .path()
            .join(format!("retry.{}.{counter}", std::process::id()));
        fs::write(path, b"existing").unwrap();
    }
    let temporary = TempFile::create(directory.path(), "retry.", '.', TempMode::Default).unwrap();
    let counter = temporary
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .rsplit('.')
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert!(counter >= end);
    drop(temporary);
    for counter in first..end {
        let path = directory
            .path()
            .join(format!("retry.{}.{counter}", std::process::id()));
        assert_eq!(fs::read(path).unwrap(), b"existing");
    }
}

#[test]
fn create_failure_reports_attempted_path() {
    let directory = TempDir::new().unwrap();
    let missing = directory.path().join("missing");
    match TempFile::create(&missing, "test.", '.', TempMode::Default) {
        Err((path, error)) => {
            assert_eq!(path.parent(), Some(missing.as_path()));
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
        }
        Ok(_) => panic!("creation in a missing directory succeeded"),
    }
}

#[cfg(unix)]
#[test]
fn modes_match_private_and_default_creation() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempDir::new().unwrap();
    let normal = directory.path().join("normal");
    File::create(&normal).unwrap();
    for mode in [TempMode::Private, TempMode::Default] {
        let temporary = TempFile::create(directory.path(), "mode.", '.', mode).unwrap();
        let expected = match mode {
            TempMode::Private => 0o600,
            TempMode::Default => fs::metadata(&normal).unwrap().permissions().mode() & 0o777,
        };
        assert_eq!(
            fs::metadata(temporary.path()).unwrap().permissions().mode() & 0o777,
            expected
        );
    }
}

#[test]
fn take_file_transfers_handle_without_disarming_cleanup() {
    let directory = TempDir::new().unwrap();
    for action in ["drop", "publish", "keep"] {
        let path = directory.path().join(action);
        let mut temporary = TempFile::create_at(&path, TempMode::Default).unwrap();
        let mut file = temporary.take_file().unwrap();
        assert!(temporary.take_file().is_none());
        file.write_all(b"transferred").unwrap();
        drop(file);
        assert_eq!(fs::read(&path).unwrap(), b"transferred");
        match action {
            "publish" => {
                let target = directory.path().join("published");
                temporary.publish(&target).unwrap();
                assert!(!path.exists());
                assert_eq!(fs::read(target).unwrap(), b"transferred");
            }
            "keep" => {
                temporary.keep();
                assert_eq!(fs::read(path).unwrap(), b"transferred");
            }
            _ => {
                drop(temporary);
                assert!(!path.exists());
            }
        }
    }
}

#[test]
fn close_releases_handle_without_disarming_cleanup() {
    let directory = TempDir::new().unwrap();
    for publish in [false, true] {
        let mut temporary =
            TempFile::create(directory.path(), "close.", '.', TempMode::Default).unwrap();
        let path = temporary.path().to_path_buf();
        temporary.file_mut().write_all(b"closed").unwrap();
        temporary.close();
        temporary.close();
        assert!(temporary.take_file().is_none());
        assert_eq!(fs::read(&path).unwrap(), b"closed");
        if publish {
            let target = directory.path().join("published");
            temporary.publish(&target).unwrap();
            assert_eq!(fs::read(target).unwrap(), b"closed");
        } else {
            drop(temporary);
        }
        assert!(!path.exists());
    }
}

#[test]
fn remove_deletes_file_and_returns_raw_removal_errors() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("remove");
    let temporary = TempFile::create_at(&path, TempMode::Default).unwrap();
    temporary.remove().unwrap();
    assert!(!path.exists());

    let mut temporary = TempFile::create_at(&path, TempMode::Default).unwrap();
    temporary.close();
    fs::remove_file(&path).unwrap();
    let expected = fs::remove_file(&path).unwrap_err();
    let error = temporary.remove().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(error.raw_os_error(), expected.raw_os_error());

    let mut temporary = TempFile::create_at(&path, TempMode::Default).unwrap();
    temporary.close();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let expected = fs::remove_file(&path).unwrap_err();
    let error = temporary.remove().unwrap_err();
    assert_eq!(error.kind(), expected.kind());
    assert_eq!(error.raw_os_error(), expected.raw_os_error());
    assert!(path.is_dir());
}

#[test]
fn sibling_uses_requested_names_retries_and_reports_attempted_path() {
    let directory = TempDir::new().unwrap();
    let first = NEXT_TEMP.load(Ordering::Relaxed);
    let end = first + 256;
    for counter in first..end {
        let path = directory
            .path()
            .join(format!("sibling.{}-{counter}", std::process::id()));
        fs::write(path, b"existing").unwrap();
    }
    let mut temporary = TempFile::sibling(directory.path(), "sibling.", '-').unwrap();
    let path = temporary.path().to_path_buf();
    let prefix = format!("sibling.{}-", std::process::id());
    let counter = path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix(&prefix)
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert!(counter >= end);
    assert!(!path.exists());
    assert!(temporary.take_file().is_none());
    fs::write(&path, b"created later").unwrap();
    drop(temporary);
    assert!(!path.exists());
    for counter in first..end {
        let path = directory
            .path()
            .join(format!("sibling.{}-{counter}", std::process::id()));
        assert_eq!(fs::read(path).unwrap(), b"existing");
    }

    let parent_file = directory.path().join("parent-file");
    fs::write(&parent_file, b"parent").unwrap();
    // Windows reports NotFound beneath a regular file; NUL is invalid on every platform.
    for parent in [parent_file, directory.path().join("invalid\0")] {
        let expected = fs::symlink_metadata(parent.join("child")).unwrap_err();
        match TempFile::sibling(&parent, "test.", '.') {
            Err((path, error)) => {
                assert_ne!(expected.kind(), io::ErrorKind::NotFound);
                assert_eq!(path.parent(), Some(parent.as_path()));
                assert_eq!(error.kind(), expected.kind());
                assert_eq!(error.raw_os_error(), expected.raw_os_error());
            }
            Ok(mut temporary) => {
                assert_eq!(expected.kind(), io::ErrorKind::NotFound);
                assert_eq!(temporary.path().parent(), Some(parent.as_path()));
                assert!(!temporary.path().exists());
                assert!(temporary.take_file().is_none());
            }
        }
    }
}

#[test]
fn sibling_at_guards_absent_path_and_refuses_existing_entries() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("exact");
    let mut temporary = TempFile::sibling_at(&path).unwrap();
    assert_eq!(temporary.path(), path);
    assert!(!path.exists());
    assert!(temporary.take_file().is_none());
    fs::write(&path, b"created later").unwrap();
    drop(temporary);
    assert!(!path.exists());

    fs::write(&path, b"existing").unwrap();
    assert!(matches!(
        TempFile::sibling_at(&path),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&path).unwrap(), b"existing");
    // Accept NotFound as reported by the platform and preserve every other error.
    for candidate in [path.join("child"), directory.path().join("invalid\0")] {
        let expected = fs::symlink_metadata(&candidate).unwrap_err();
        match TempFile::sibling_at(&candidate) {
            Err(error) => {
                assert_ne!(expected.kind(), io::ErrorKind::NotFound);
                assert_eq!(error.kind(), expected.kind());
                assert_eq!(error.raw_os_error(), expected.raw_os_error());
            }
            Ok(mut temporary) => {
                assert_eq!(expected.kind(), io::ErrorKind::NotFound);
                assert_eq!(temporary.path(), candidate);
                assert!(!candidate.exists());
                assert!(temporary.take_file().is_none());
            }
        }
    }
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(matches!(
        TempFile::sibling_at(&path),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists
    ));
    assert!(path.is_dir());

    #[cfg(unix)]
    {
        let symbolic_link = directory.path().join("symbolic-link");
        let target = directory.path().join("target");
        for exists in [false, true] {
            if exists {
                fs::write(&target, b"target bytes").unwrap();
            }
            std::os::unix::fs::symlink(&target, &symbolic_link).unwrap();
            assert!(matches!(
                TempFile::sibling_at(&symbolic_link),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists
            ));
            assert_eq!(fs::read_link(&symbolic_link).unwrap(), target);
            assert_eq!(target.exists(), exists);
            if exists {
                assert_eq!(fs::read(&target).unwrap(), b"target bytes");
            }
            fs::remove_file(&symbolic_link).unwrap();
        }
    }
}
