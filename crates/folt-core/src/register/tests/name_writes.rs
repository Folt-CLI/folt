use super::*;

#[test]
fn name_writes_restore_the_whole_batch_at_transaction_and_commit_failures() {
    for fail_commit in [false, true] {
        let fixture = TempHome::new().unwrap();
        let paths = [fixture.root.join("one.md"), fixture.root.join("two.md")];
        let originals: [&[u8]; 2] = [b"first body", b"---\ntitle: keep\n---\nsecond body"];
        for (path, bytes) in paths.iter().zip(originals) {
            fs::write(path, bytes).unwrap();
        }
        let directory = fixture.dir("directory");
        let marker = directory.join(MARKER_FILE);
        let plan = plan_register(&fixture.home, [&paths[0], &paths[1], &directory]).unwrap();
        let mut reached = false;
        let error = apply_with_hook(&fixture.home, &plan, false, |step| {
            if matches!(step, Step::Transaction) && !fail_commit
                || matches!(step, Step::Commit) && fail_commit
            {
                reached = true;
                for (path, expected) in paths.iter().zip(["one", "two"]) {
                    assert_eq!(
                        front_matter::read_name(&fs::read(path).unwrap())
                            .unwrap()
                            .as_deref(),
                        Some(expected)
                    );
                }
                assert_eq!(
                    fs::read(&marker).unwrap(),
                    b"---\nfolio-name: \"directory\"\n---\n"
                );
                return Err(injected());
            }
            Ok(())
        })
        .unwrap_err();
        assert!(reached);
        assert!(error.to_string().contains("injected failure"));
        for (path, bytes) in paths.iter().zip(originals) {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(!marker.exists());
        assert_eq!(fixture.folio_count(), 0);
        assert!(
            Registry::open_existing(&fixture.home)
                .unwrap()
                .folios()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn restore_preserves_external_edits_and_reports_the_retained_name() {
    let fixture = TempHome::new().unwrap();
    let path = fixture.root.join("file.md");
    fs::write(&path, b"body").unwrap();
    let plan = plan_register(&fixture.home, [&path]).unwrap();
    let error = apply_with_hook(&fixture.home, &plan, false, |step| {
        if matches!(step, Step::Transaction) {
            assert_eq!(
                front_matter::read_name(&fs::read(&path).unwrap())
                    .unwrap()
                    .as_deref(),
                Some("file")
            );
            fs::write(&path, b"---\nfolio-name: file\n---\nuser edit").unwrap();
            return Err(injected());
        }
        Ok(())
    })
    .unwrap_err();
    assert!(error.to_string().contains("keeps the name"));
    assert!(
        error
            .to_string()
            .contains(&plan.entries[0].path.display().to_string())
    );
    assert_eq!(
        fs::read(path).unwrap(),
        b"---\nfolio-name: file\n---\nuser edit"
    );
    assert_eq!(fixture.folio_count(), 0);
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn changed_after_refresh_blocks_without_writing_and_restores_earlier_files() {
    let fixture = TempHome::new().unwrap();
    let first = fixture.root.join("first.md");
    let second = fixture.root.join("second.md");
    fs::write(&first, b"first body").unwrap();
    fs::write(&second, b"second body").unwrap();
    let plan = plan_register(&fixture.home, [&first, &second]).unwrap();
    let result = apply_with_hook(&fixture.home, &plan, false, |step| {
        if matches!(step, Step::Refreshed(_)) {
            fs::write(&second, b"user edit").unwrap();
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(
        result.entries[1].status,
        RegisterStatus::Refused(RefusalReason::FileChanged)
    );
    assert_eq!(fs::read(first).unwrap(), b"first body");
    assert_eq!(fs::read(second).unwrap(), b"user edit");
    assert_eq!(fixture.folio_count(), 0);
}

#[test]
fn missing_refreshed_marker_signature_blocks_and_rolls_back_earlier_files() {
    let fixture = TempHome::new().unwrap();
    let first = fixture.root.join("first.md");
    let notes = fixture.dir("notes");
    let original = b"body";
    fs::write(&first, original).unwrap();
    let plan = plan_register(&fixture.home, [&first, &notes]).unwrap();
    let first = &plan.entries[0].path;
    let marker = &plan.entries[1].path;
    let mut folios_started = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_with_hook(&fixture.home, &plan, false, |step| {
            match step {
                Step::MarkerSynced => fs::remove_file(marker).unwrap(),
                Step::Refreshed(refreshed) => {
                    assert!(refreshed.entries[1].signature.is_none());
                    fs::write(marker, b"").unwrap();
                }
                Step::Folio(_) => {
                    folios_started += 1;
                    assert_ne!(fs::read(first).unwrap(), original);
                }
                _ => {}
            }
            Ok(())
        })
    }));
    let result = result.expect("a missing refreshed signature must not panic");
    if let Ok(blocked) = result {
        assert_eq!(
            blocked.entries[1].status,
            RegisterStatus::Refused(RefusalReason::FileChanged)
        );
    }
    assert_eq!(folios_started, 1);
    assert_eq!(fs::read(first).unwrap(), original);
    assert_eq!(fixture.folio_count(), 0);
    assert_eq!(
        fs::read_dir(fixture.home.path().join("folios/.new"))
            .unwrap()
            .count(),
        0
    );
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn captured_size_confirmation_restores_names_and_removes_new_markers() {
    let fixture = TempHome::new().unwrap();
    fs::create_dir(fixture.home.path().join("folios")).unwrap();
    fs::write(
        fixture.home.path().join("config.toml"),
        "large_file_bytes = 20\n",
    )
    .unwrap();
    let directory = fixture.dir("directory");
    let path = fixture.root.join("file.md");
    fs::write(&path, b"body").unwrap();
    let plan = plan_register(&fixture.home, [&directory, &path]).unwrap();
    assert_eq!(plan.summary.needs_confirmation, 0);
    let blocked = apply_register(&fixture.home, &plan, false).unwrap();
    assert_eq!(blocked.summary.needs_confirmation, 1);
    assert!(!directory.join(MARKER_FILE).exists());
    assert_eq!(fs::read(&path).unwrap(), b"body");
    assert_eq!(fixture.folio_count(), 0);
    // Replan because restoring the original bytes changes the signature.
    let retry = plan_register(&fixture.home, [&directory, &path]).unwrap();
    let confirmed = apply_register(&fixture.home, &retry, true).unwrap();
    assert_eq!(confirmed.summary.refused, 0);
    assert!(directory.join(MARKER_FILE).exists());
    assert_eq!(fixture.folio_count(), 2);
}

#[test]
fn metadata_failure_after_name_publish_restores_or_reports_retained_name() {
    let fixture = TempHome::new().unwrap();
    fs::create_dir(fixture.home.path().join("folios")).unwrap();
    let path = fixture.root.join("file.md");
    let original = b"body";
    fs::write(&path, original).unwrap();
    let plan = plan_register(&fixture.home, [&path]).unwrap();
    let path = &plan.entries[0].path;
    let error = crate::placement_fs::with_rewrite_metadata_failure(path, || {
        apply_register(&fixture.home, &plan, false)
    })
    .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("injected rewrite metadata failure"));
    if fs::read(path).unwrap() != original {
        assert!(
            message.contains(&format!("{} keeps the name:", path.display())),
            "a retained name must be reported with its path: {message}"
        );
    }
    assert_eq!(fixture.folio_count(), 0);
    assert!(
        Registry::open_existing(&fixture.home)
            .unwrap()
            .folios()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn restore_failure_before_publication_reports_the_retained_name_with_path() {
    let fixture = TempHome::new().unwrap();
    let path = fixture.root.join("file.md");
    fs::write(&path, b"body").unwrap();
    let plan = plan_register(&fixture.home, [&path]).unwrap();
    let path = &plan.entries[0].path;
    let edited = b"---\nfolio-name: \"file\"\n---\nbody with user edits";
    let error = apply_with_hook(&fixture.home, &plan, false, |step| {
        if matches!(step, Step::Transaction) {
            fs::write(path, edited).unwrap();
            return Err(injected());
        }
        Ok(())
    })
    .unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains(&format!(
            "{} keeps the name: file changed after our write",
            path.display()
        )),
        "a failed restore must report the retained name with its path: {message}"
    );
    assert_eq!(fs::read(path).unwrap(), edited);
    assert_eq!(fixture.folio_count(), 0);
}

#[test]
fn directory_sync_failure_after_name_publish_still_attempts_restore() {
    let fixture = TempHome::new().unwrap();
    fs::create_dir(fixture.home.path().join("folios")).unwrap();
    let path = fixture.root.join("file.md");
    fs::write(&path, b"body").unwrap();
    let plan = plan_register(&fixture.home, [&path]).unwrap();
    let error = crate::fs::with_sync_failure(&fixture.root, || {
        apply_register(&fixture.home, &plan, false)
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected directory sync failure")
    );
    let message = error.to_string();
    assert!(
        message.contains(&format!(
            "{}: restoration completed but the directory sync failed; original bytes are back, durability of the restore is not confirmed",
            plan.entries[0].path.display()
        )),
        "a completed restore must report unconfirmed durability: {message}"
    );
    assert!(!message.contains("keeps the name"));
    // Both renames happened even though their directory syncs failed.
    assert_eq!(fs::read(path).unwrap(), b"body");
    assert_eq!(fixture.folio_count(), 0);
}
