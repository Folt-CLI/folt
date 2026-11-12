use super::*;

#[test]
fn blocked_link_registration_does_not_probe_or_create_markers() {
    let fixture = TempHome::new().unwrap();
    let directory = fixture.dir("directory");
    let missing = fixture.root.join("missing");
    let plan = plan_register(&fixture.home, [&directory, &missing]).unwrap();
    let result = apply_links_with_hook(&fixture.home, &plan, false, |_| {
        panic!("blocked apply must not probe")
    })
    .unwrap();
    assert_eq!(result.registration.summary.refused, 1);
    assert!(result.paths.is_empty());
    assert!(!directory.join(MARKER_FILE).exists());
    assert!(!fixture.home.path().join("registry.db").exists());
}

#[test]
fn link_registration_failure_rolls_back_before_commit() {
    let fixture = TempHome::new().unwrap();
    let directory = fixture.dir("directory");
    let plan = plan_register(&fixture.home, [&directory]).unwrap();
    let folios = fixture.home.path().join("folios");
    let result = crate::fs::with_sync_failure(&folios, || {
        apply_links_with_hook(&fixture.home, &plan, false, |_| Ok(()))
    });
    assert!(result.is_err());
    assert!(!directory.join(MARKER_FILE).exists());
    assert_eq!(fixture.folio_count(), 0);
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
}

#[cfg_attr(windows, ignore = "real symlinks require privileges")]
#[test]
fn crash_after_registration_keeps_copies_and_retry_converts_without_new_folios() {
    let fixture = TempHome::new().unwrap();
    let path = fixture.root.join("file");
    fs::write(&path, b"content").unwrap();
    let plan = plan_register(&fixture.home, [&path]).unwrap();
    let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = apply_links_with_hook(&fixture.home, &plan, false, |step| {
            if matches!(step, LinkStep::Registered) {
                panic!("simulated crash");
            }
            Ok(())
        });
    }));
    assert!(crash.is_err());
    assert_eq!(fixture.folio_count(), 1);
    assert!(!fs::symlink_metadata(&path).unwrap().is_symlink());
    assert_eq!(fs::read(&path).unwrap(), b"content");
    let result = apply_register_links(&fixture.home, &plan, false).unwrap();
    assert_eq!(result.registration.summary.already_registered, 1);
    assert_eq!(result.paths[0].1, RegisterLinkResult::Linked);
    assert_eq!(fixture.folio_count(), 1);
}

#[cfg_attr(windows, ignore = "real symlinks require privileges")]
#[test]
fn edited_path_is_reported_and_other_conversions_continue() {
    let fixture = TempHome::new().unwrap();
    let paths = ["first", "edited", "last"].map(|name| fixture.root.join(name));
    for path in &paths {
        fs::write(path, b"original").unwrap();
    }
    let edited = resolved_path(&paths[1]);
    let plan = plan_register(&fixture.home, &paths).unwrap();
    let result = apply_links_with_hook(&fixture.home, &plan, false, |step| {
        match step {
            LinkStep::Probe(path) => PlacementFs::new(&fixture.home).probe_link(path)?,
            LinkStep::Convert(path) if resolved_path(path) == edited => fs::write(path, b"edit")?,
            _ => {}
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(result.paths[0].1, RegisterLinkResult::Linked);
    assert!(
        matches!(&result.paths[1].1, RegisterLinkResult::LeftAsCopy { reason }
        if reason.contains("edited"))
    );
    assert_eq!(result.paths[2].1, RegisterLinkResult::Linked);
    assert_eq!(fs::read(&paths[1]).unwrap(), b"edit");
    let registry = Registry::open_existing(&fixture.home).unwrap();
    assert_eq!(registry.folios().unwrap().len(), 3);
    assert_eq!(
        registry
            .placement_at(&PlacementPath::resolve(&paths[1]).unwrap())
            .unwrap()
            .unwrap()
            .kind,
        PlacementKind::Copy
    );
}

#[cfg_attr(windows, ignore = "real symlinks require privileges")]
#[test]
fn conversion_syncs_link_creation_and_rename_and_retry_finishes_copy_row() {
    for successful_syncs in [0, 1] {
        let fixture = TempHome::new().unwrap();
        let directory = fixture.dir("source").canonicalize().unwrap();
        let path = directory.join("file");
        fs::write(&path, b"content").unwrap();
        let plan = plan_register(&fixture.home, [&path]).unwrap();
        apply_register(&fixture.home, &plan, false).unwrap();
        let (result, retry) =
            crate::fs::with_sync_failure_after(&directory, successful_syncs, || {
                let result = apply_register_links(&fixture.home, &plan, false).unwrap();
                let retry = apply_register_links(&fixture.home, &plan, false).unwrap();
                (result, retry)
            });
        assert!(
            matches!(&result.paths[0].1, RegisterLinkResult::LeftAsCopy { reason }
            if reason.contains("injected directory sync failure"))
        );
        assert_eq!(
            fs::symlink_metadata(&path).unwrap().is_symlink(),
            successful_syncs == 1
        );
        assert_eq!(fs::read(&path).unwrap(), b"content");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        let registry = Registry::open_existing(&fixture.home).unwrap();
        let row = registry
            .placement_at(&PlacementPath::resolve(&path).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(result.paths[0].0, path);
        assert_eq!(retry.paths[0].0, path);
        assert!(
            matches!(&retry.paths[0].1, RegisterLinkResult::LeftAsCopy { reason }
            if reason.contains("injected directory sync failure"))
        );
        assert_eq!(row.kind, PlacementKind::Copy);
        if successful_syncs == 1 {
            let (_, placement) = row.to_placement();
            assert_eq!(
                crate::placement::classify(&placement, &Observation::IntactLink, None),
                crate::placement::Drift::Unusable
            );
        }
        let result = apply_register_links(&fixture.home, &plan, false).unwrap();
        assert_eq!(result.paths[0].1, RegisterLinkResult::Linked);
        assert_eq!(fixture.folio_count(), 1);
        assert_eq!(
            registry.placement_at(&row.path).unwrap().unwrap().kind,
            PlacementKind::Link
        );
    }
}
