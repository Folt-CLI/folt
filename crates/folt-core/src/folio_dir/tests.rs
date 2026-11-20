use super::*;
use crate::config::env::Environment;
use crate::home::Home;
use crate::test_support::TempDir;

#[test]
fn publishes_staged_folios_with_both_durability_modes() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("home");
    let home = Home::open_or_create(path.clone(), &Environment::default()).unwrap();
    let lock = home.try_lock().unwrap();

    for durability in [Durability::None, Durability::DataAndDirectories] {
        // On Windows this rename fails if the staged content file is still open.
        let folio = create_folio(
            home.path(),
            &lock,
            FolioSpec {
                content: b"staged content".to_vec(),
            },
            durability,
        )
        .unwrap();
        assert_eq!(
            folio.path(),
            path.join("folios").join(folio.id().to_string())
        );
        assert_eq!(
            fs::read(folio.path().join(CONTENT_FILE)).unwrap(),
            b"staged content"
        );
        assert!(matches!(
            inspect(home.path(), OsStr::new(&folio.id().to_string()), None),
            Inspection::Unregistered(_, fixes) if fixes.is_empty()
        ));
        assert_eq!(fs::read_dir(path.join("folios/.new")).unwrap().count(), 0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            for directory in [
                path.clone(),
                path.join("folios"),
                path.join("folios/.new"),
                folio.path().to_path_buf(),
                folio.path().join("blobs"),
                folio.path().join("refs"),
            ] {
                assert_eq!(
                    fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
        }
    }

    drop(lock);
}

#[test]
fn failure_after_content_creation_leaves_no_staging_entry() {
    let directory = TempDir::new().unwrap();
    let home =
        Home::open_or_create(directory.path().join("home"), &Environment::default()).unwrap();
    let lock = home.try_lock().unwrap();
    let folios = home.path().join(FOLIOS_DIR);
    // Fail the containing-directory sync after the directory has been renamed.
    let fs = crate::fsx::fault::FaultFileSystem::new().sync_failure(&folios, 0);
    let error = create_folio_with(
        &fs,
        home.path(),
        &lock,
        FolioSpec {
            content: b"content".to_vec(),
        },
        Durability::DataAndDirectories,
    )
    .unwrap_err();
    assert!(matches!(error, FolioDirError::Io { path, source }
        if path == folios && source.to_string() == "injected directory sync failure"));
    assert_eq!(fs::read_dir(folios.join(NEW_DIR)).unwrap().count(), 0);
    assert_eq!(fs::read_dir(folios).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn published_content_keeps_private_mode() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempDir::new().unwrap();
    let home =
        Home::open_or_create(directory.path().join("home"), &Environment::default()).unwrap();
    let lock = home.try_lock().unwrap();
    for durability in [Durability::None, Durability::DataAndDirectories] {
        let folio = create_folio(
            home.path(),
            &lock,
            FolioSpec {
                content: b"content".to_vec(),
            },
            durability,
        )
        .unwrap();
        assert_eq!(
            fs::metadata(folio.path().join(CONTENT_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
