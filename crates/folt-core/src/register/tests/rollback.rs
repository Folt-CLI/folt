use super::*;

#[test]
fn published_folio_sync_failure_rolls_back_directory_markers_and_rows() {
    let fixture = TempHome::new().unwrap();
    let directory = fixture.dir("directory");
    let plan = plan_register(&fixture.home, [&directory]).unwrap();
    let folios = fixture.home.path().join("folios");
    let result =
        crate::fs::with_sync_failure(&folios, || apply_register(&fixture.home, &plan, false));
    match result {
        Err(RegisterError::Folio(FolioDirError::Io { path, source })) => {
            assert_eq!(path, resolved_path(&folios));
            assert_eq!(source.kind(), io::ErrorKind::Other);
            assert_eq!(source.to_string(), "injected directory sync failure");
        }
        other => panic!("expected the published Folio sync error, got {other:?}"),
    }
    assert!(!directory.join(MARKER_FILE).exists());
    assert_eq!(fixture.folio_count(), 0);
    assert_eq!(fs::read_dir(folios.join(".new")).unwrap().count(), 0);
    let registry = Registry::open_existing(&fixture.home).unwrap();
    assert!(registry.folios().unwrap().is_empty());
    assert!(
        registry
            .placement_at(&PlacementPath::resolve(directory.join(MARKER_FILE)).unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn ordinary_failures_roll_back_all_rows_and_remove_only_owned_artifacts() {
    for fail_before_transaction in [false, true] {
        let fixture = TempHome::new().unwrap();
        let new = fixture.dir("new");
        let existing = fixture.dir("existing");
        let marker = existing.join(MARKER_FILE);
        fs::write(&marker, b"keep").unwrap();
        let plan = plan_register(&fixture.home, [&new, &existing]).unwrap();
        let result = apply_with_hook(&fixture.home, &plan, false, |step| {
            if matches!(step, Step::Transaction) == fail_before_transaction
                && matches!(step, Step::Transaction | Step::Commit)
            {
                Err(injected())
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(!new.join(MARKER_FILE).exists());
        assert_eq!(fs::read(&marker).unwrap(), b"keep");
        assert_eq!(fixture.folio_count(), 0);
        let registry = Registry::open_existing(&fixture.home).unwrap();
        assert!(registry.folios().unwrap().is_empty());
        for entry in &plan.entries {
            assert!(
                registry
                    .placement_at(&PlacementPath::resolve(&entry.path).unwrap())
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn marker_that_appears_during_apply_is_used_and_preserved_on_failure() {
    let fixture = TempHome::new().unwrap();
    let dir = fixture.dir("directory");
    let marker = dir.join(MARKER_FILE);
    let plan = plan_register(&fixture.home, [&dir]).unwrap();
    assert!(
        apply_with_hook(&fixture.home, &plan, false, |step| {
            match step {
                Step::Markers => {
                    fs::write(&marker, b"appeared").unwrap();
                    Ok(())
                }
                Step::Transaction => Err(injected()),
                _ => Ok(()),
            }
        })
        .is_err()
    );
    assert_eq!(fs::read(marker).unwrap(), b"appeared");
    assert_eq!(fixture.folio_count(), 0);
}

#[test]
fn replaced_owned_marker_is_never_removed() {
    let fixture = TempHome::new().unwrap();
    let dir = fixture.dir("directory");
    let marker = dir.join(MARKER_FILE);
    let plan = plan_register(&fixture.home, [&dir]).unwrap();
    assert!(
        apply_with_hook(&fixture.home, &plan, false, |step| {
            if matches!(step, Step::Transaction) {
                let replacement = fixture.root.join("replacement");
                fs::write(&replacement, b"replacement").unwrap();
                fs::remove_file(&marker).unwrap();
                fs::rename(replacement, &marker).unwrap();
                Err(injected())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(fs::read(marker).unwrap(), b"replacement");
}

#[test]
fn crash_before_transaction_leaves_orphan_and_retry_creates_a_new_folio() {
    let fixture = TempHome::new().unwrap();
    let path = fixture.root.join("file");
    fs::write(&path, b"content").unwrap();
    let plan = plan_register(&fixture.home, [&path]).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = apply_with_hook(&fixture.home, &plan, false, |step| {
            if matches!(step, Step::Transaction) {
                panic!("simulated crash");
            }
            Ok(())
        });
    }));
    assert!(result.is_err());
    assert_eq!(fixture.folio_count(), 1);
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
    apply_register(&fixture.home, &plan, false).unwrap();
    assert_eq!(fixture.folio_count(), 2);
    assert_eq!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn oversized_appearing_marker_blocks_and_cleans_only_the_new_marker() {
    let fixture = TempHome::new().unwrap();
    fs::write(
        fixture.home.path().join("config.toml"),
        "large_file_bytes = 4\n",
    )
    .unwrap();
    let first = fixture.dir("first");
    let second = fixture.dir("second");
    let marker = second.join(MARKER_FILE);
    let plan = plan_register(&fixture.home, [&first, &second]).unwrap();
    let result = apply_with_hook(&fixture.home, &plan, false, |step| {
        if matches!(step, Step::Markers) {
            fs::write(&marker, b"large").unwrap();
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(result.summary.needs_confirmation, 1);
    assert!(!first.join(MARKER_FILE).exists());
    assert_eq!(fs::read(marker).unwrap(), b"large");
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unique_name_index_failure_rolls_back_the_entire_registration() {
    let fixture = TempHome::new().unwrap();
    let paths = ["first.txt", "second.txt"].map(|p| fixture.root.join(p));
    for path in &paths {
        fs::write(path, b"body").unwrap();
    }
    let plan = plan_register(&fixture.home, &paths).unwrap();
    let lock = fixture.home.lock().unwrap();
    let holder = FolioId::generate();
    let result = apply_locked(&fixture.home, &plan, false, &lock, |step| {
        if matches!(step, Step::Transaction) {
            let mut registry = Registry::open_or_create(&fixture.home, &lock)?;
            registry.write(&lock, |tx| {
                tx.add_folio(&NewFolio {
                    id: holder,
                    format: FolioFormat {
                        layout: FOLIO_LAYOUT_VERSION,
                        algorithm: OBJECT_ALGORITHM.into(),
                    },
                })?;
                tx.set_name(holder, &FolioName::parse("second").unwrap())
            })?;
        }
        Ok(())
    });
    assert!(
        matches!(result, Err(RegisterError::Registry(RegistryError::NameTaken(ref name))) if name.as_str() == "second")
    );
    let registry = Registry::open_existing(&fixture.home).unwrap();
    assert_eq!(registry.folios().unwrap(), vec![holder]);
    assert_eq!(
        registry
            .find_by_name(&FolioName::parse("first").unwrap())
            .unwrap(),
        None
    );
    assert_eq!(fixture.folio_count(), 0);
    for path in &paths {
        assert!(
            registry
                .placement_at(&PlacementPath::resolve(path).unwrap())
                .unwrap()
                .is_none()
        );
    }
}
