use super::*;

#[test]
fn case_variant_appearing_marker_replans_and_cleans_up_on_failure() {
    for fail_transaction in [false, true] {
        let fixture = TempHome::new().unwrap();
        let owned = fixture.dir("owned");
        let appeared = fixture.dir("appeared");
        let marker = appeared.join("__FOLIO__.MD");
        let lowercase = appeared.join(MARKER_FILE);
        let plan = plan_register(&fixture.home, [&owned, &appeared]).unwrap();
        let mut reached_transaction = false;
        let result = apply_with_hook(&fixture.home, &plan, false, |step| {
            match step {
                Step::Markers => {
                    fs::write(&marker, b"---\nfolio-name: appeared\n---\neditor content").unwrap();
                    // A hard link supplies the case-insensitive alias on filesystems
                    // where the two spellings name distinct paths. create_new must
                    // see an existing file and must not claim ownership of it.
                    if !lowercase.exists() {
                        fs::hard_link(&marker, &lowercase).unwrap();
                    }
                }
                Step::Refreshed(refreshed) => {
                    let index = refreshed
                        .requests
                        .iter()
                        .position(|p| p.path == appeared)
                        .unwrap();
                    refreshed.entries[index].path = marker.clone();
                }
                Step::Transaction => {
                    reached_transaction = true;
                    if fail_transaction {
                        return Err(injected());
                    }
                }
                _ => {}
            }
            Ok(())
        });
        assert!(reached_transaction);
        assert_eq!(
            fs::read(&marker).unwrap(),
            b"---\nfolio-name: appeared\n---\neditor content"
        );
        assert_eq!(
            fs::read(&lowercase).unwrap(),
            b"---\nfolio-name: appeared\n---\neditor content"
        );
        if fail_transaction {
            assert!(result.is_err());
            assert!(!owned.join(MARKER_FILE).exists());
            assert_eq!(fixture.folio_count(), 0);
            assert!(
                Registry::open_existing(&fixture.home)
                    .unwrap()
                    .folios()
                    .unwrap()
                    .is_empty()
            );
        } else {
            let applied = result.unwrap();
            assert_eq!(applied.entries[1].path, resolved_path(&marker));
            assert_eq!(
                applied.entries[1].size,
                b"---\nfolio-name: appeared\n---\neditor content".len() as u64
            );
            assert!(!applied.entries[1].creates_marker);
            let registry = Registry::open_existing(&fixture.home).unwrap();
            let row = registry
                .placement_at(&PlacementPath::resolve(&marker).unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(
                fs::read(
                    fixture
                        .home
                        .path()
                        .join("folios")
                        .join(row.id.to_string())
                        .join(CONTENT_FILE)
                )
                .unwrap(),
                b"---\nfolio-name: appeared\n---\neditor content"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn collapsed_directory_request_is_blocked_before_creating_markers() {
    use std::os::unix::fs::symlink;

    let fixture = TempHome::new().unwrap();
    let a = fixture.dir("a");
    let b = fixture.dir("b");
    fs::create_dir(a.join("notes")).unwrap();
    fs::create_dir(b.join("notes")).unwrap();
    let current = fixture.root.join("current");
    symlink(&a, &current).unwrap();
    let plan = plan_register(
        &fixture.home,
        [
            RegisterRequest::new(current.join("notes"), FolioName::parse("Chosen").unwrap()),
            RegisterRequest::from(b.join("notes")),
        ],
    )
    .unwrap();
    assert_eq!(plan.summary.new, 2);
    fs::remove_file(&current).unwrap();
    symlink(&b, &current).unwrap();

    let mut marker_created = false;
    let blocked = apply_with_hook(&fixture.home, &plan, false, |step| {
        if matches!(step, Step::MarkerSynced) {
            marker_created = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(
        !marker_created,
        "moved request must be blocked before marker creation"
    );
    assert_eq!(blocked.entries.len(), 1);
    assert_eq!(blocked.summary.refused, 1);
    assert_eq!(blocked.entries[0].path, plan.entries[1].path);
    assert_eq!(
        blocked.entries[0].status,
        RegisterStatus::Refused(RefusalReason::DestinationChanged {
            path: plan.entries[1].path.clone(),
        })
    );
    for entry in &plan.entries {
        assert!(!entry.path.exists());
    }
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn registration_syncs_markers_and_uses_durable_folios_before_transaction() {
    let fixture = TempHome::new().unwrap();
    let directory = fixture.dir("directory");
    let file = fixture.root.join("file");
    fs::write(&file, b"content").unwrap();
    let plan = plan_register(&fixture.home, [&directory, &file]).unwrap();
    let mut marker_synced = false;
    let mut durable_folios = 0;
    let mut committed = false;
    apply_with_hook(&fixture.home, &plan, false, |step| {
        match step {
            Step::MarkerSynced => marker_synced = true,
            Step::Folio(durability) => {
                assert!(marker_synced);
                assert_eq!(durability, Durability::DataAndDirectories);
                durable_folios += 1;
            }
            Step::Transaction => {
                assert!(marker_synced);
                assert_eq!(durable_folios, 2);
                assert!(
                    Registry::open_existing(&fixture.home)
                        .unwrap()
                        .folios()
                        .unwrap()
                        .is_empty()
                );
            }
            Step::Commit => committed = true,
            _ => {}
        }
        Ok(())
    })
    .unwrap();
    assert!(committed);
}

#[test]
fn probes_the_entire_batch_before_creating_anything() {
    let fixture = TempHome::new().unwrap();
    let first = fixture.dir("first");
    let second = fixture.dir("second");
    let plan = plan_register(&fixture.home, [&first, &second]).unwrap();
    let mut probed = Vec::new();
    let result = apply_links_with_hook(&fixture.home, &plan, false, |step| {
        if let LinkStep::Probe(path) = step {
            assert!(!first.join(MARKER_FILE).exists());
            assert!(!second.join(MARKER_FILE).exists());
            assert!(!fixture.home.path().join("folios").exists());
            assert!(!fixture.home.path().join("registry.db").exists());
            assert!(matches!(
                fixture.home.try_lock(),
                Err(HomeError::Locked { .. })
            ));
            probed.push(path.to_path_buf());
            if probed.len() == 2 {
                return Err(io::Error::other("probe unavailable").into());
            }
        } else {
            panic!("registration must not start after a failed probe");
        }
        Ok(())
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("probe unavailable")
    );
    assert_eq!(probed.len(), 2);
    assert!(!first.join(MARKER_FILE).exists());
    assert!(!fixture.home.path().join("registry.db").exists());
}
