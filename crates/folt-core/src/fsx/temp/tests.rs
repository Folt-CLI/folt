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
