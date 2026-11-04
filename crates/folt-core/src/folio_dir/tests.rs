use super::*;
use crate::home::Home;
use crate::test_support::TempDir;

#[test]
fn publishes_staged_folios_with_both_durability_modes() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("home");
    let home = Home::open_or_create(path.clone()).unwrap();
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
