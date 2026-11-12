use super::*;

#[test]
fn rename_failure_rolls_back_new_rows_and_preserves_racing_destinations() {
    for directory in [false, true] {
        for target_directory in [false, true] {
            let (fixture, lock, mut registry) = RelocateFixture::new(directory);
            let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
            let plan = fixture.plan(&registry);
            let destination = plan.target.path.to_path_buf();
            let result =
                apply_relocate_steps(&plan, &fs, &mut registry, &lock, |step, registry| {
                    assert_eq!(step, RelocateStep::Inserted);
                    assert_move_rows(&plan, registry, true, true);
                    if target_directory {
                        std::fs::create_dir(&destination).unwrap();
                    } else {
                        std::fs::write(&destination, b"other process").unwrap();
                    }
                    Ok(())
                });
            assert!(result.is_err());
            assert_move_rows(&plan, &registry, true, false);
            assert!(fixture.row.path.to_path_buf().exists());
            assert_eq!(destination.is_dir(), target_directory);
            if !target_directory {
                assert_eq!(std::fs::read(destination).unwrap(), b"other process");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn rename_race_preserves_a_new_link() {
    let (fixture, lock, mut registry) = RelocateFixture::new(false);
    let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
    let plan = fixture.plan(&registry);
    let result = apply_relocate_steps(&plan, &fs, &mut registry, &lock, |step, _| {
        assert_eq!(step, RelocateStep::Inserted);
        std::os::unix::fs::symlink("foreign target", plan.target.path.to_path_buf()).unwrap();
        Ok(())
    });
    assert!(result.is_err());
    assert_move_rows(&plan, &registry, true, false);
    assert_eq!(
        std::fs::read_link(plan.target.path.to_path_buf()).unwrap(),
        PathBuf::from("foreign target")
    );
}

#[test]
fn pending_rows_are_cleaned_when_retry_destination_is_occupied() {
    let (fixture, lock, mut registry) = RelocateFixture::new(true);
    let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
    let plan = fixture.plan(&registry);
    assert!(
        apply_relocate_steps(&plan, &fs, &mut registry, &lock, |_, _| {
            Err(PlacementError::PlacementChanged.into())
        })
        .is_err()
    );
    std::fs::create_dir(plan.target.path.to_path_buf()).unwrap();
    let retry = fixture.plan(&registry);
    assert!(apply_relocate(&retry, &fs, &mut registry, &lock).is_err());
    assert_move_rows(&plan, &registry, true, false);
    assert!(fixture.row.path.to_path_buf().exists());
}

#[test]
fn directory_registry_transactions_are_all_or_nothing() {
    for insert in [false, true] {
        let (fixture, lock, mut registry) = RelocateFixture::new(true);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        let plan = fixture.plan(&registry);
        let (old, new) = &plan.target.rows[1];
        registry.fail_writes_at(if insert { &new.path } else { &old.path }, insert);
        let result = apply_relocate(&plan, &fs, &mut registry, &lock);
        assert!(result.is_err());
        assert_move_rows(&plan, &registry, true, !insert);
        assert_eq!(fixture.row.path.to_path_buf().exists(), insert);
        assert_eq!(plan.target.path.to_path_buf().exists(), !insert);
    }
}

#[cfg(unix)]
#[test]
fn link_move_retries_each_crash_window_using_only_pending_rows() {
    for crash in [
        RelocateStep::Inserted,
        RelocateStep::Renamed,
        RelocateStep::Deleted,
    ] {
        let (mut fixture, lock, mut registry) = RelocateFixture::new(false);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        let target = fs.link_target(fixture.row.id);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"shared content").unwrap();
        std::fs::remove_file(fixture.row.path.to_path_buf()).unwrap();
        std::os::unix::fs::symlink(&target, fixture.row.path.to_path_buf()).unwrap();
        fixture.row.kind = PlacementKind::Link;
        fixture.row.head = "main".into();
        fixture.row.commit_id = None;
        fixture.row.base = None;
        fixture.row.staged = None;
        registry
            .write(&lock, |tx| tx.update_placement(&fixture.row))
            .unwrap();
        let plan = fixture.plan(&registry);
        assert!(
            apply_relocate_steps(&plan, &fs, &mut registry, &lock, |step, _| {
                if step == crash {
                    Err(PlacementError::PlacementChanged.into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        drop(registry);
        let mut registry = Registry::open_or_create(&fixture.home, &lock).unwrap();
        let retry = if crash == RelocateStep::Deleted {
            plan.clone()
        } else {
            fixture.plan(&registry)
        };
        apply_relocate(&retry, &fs, &mut registry, &lock).unwrap();
        assert_move_rows(&plan, &registry, false, true);
        assert_eq!(
            std::fs::read_link(plan.target.path.to_path_buf()).unwrap(),
            target
        );
        assert_eq!(
            std::fs::read(plan.target.path.to_path_buf()).unwrap(),
            b"shared content"
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn relative_link_normalization_preserves_registry_failure_windows() {
    for insert in [true, false] {
        let (mut fixture, lock, mut registry) = RelocateFixture::new(false);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        if let Err(error) = fs.probe_link(&fixture.row.path.to_path_buf()) {
            eprintln!("skipping relative link failure windows: {error}");
            return;
        }
        let target = fs.link_target(fixture.row.id);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"shared content").unwrap();
        let relative = target
            .strip_prefix(fixture.home.path().parent().unwrap())
            .unwrap();
        std::fs::remove_file(fixture.row.path.to_path_buf()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(relative, fixture.row.path.to_path_buf()).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(relative, fixture.row.path.to_path_buf()).unwrap();
        fixture.row.kind = PlacementKind::Link;
        fixture.row.head = "main".into();
        fixture.row.commit_id = None;
        fixture.row.base = None;
        fixture.row.staged = None;
        registry
            .write(&lock, |tx| tx.update_placement(&fixture.row))
            .unwrap();
        let plan = fixture.plan(&registry);
        let (old, new) = &plan.target.rows[0];
        registry.fail_writes_at(if insert { &new.path } else { &old.path }, insert);
        assert!(apply_relocate(&plan, &fs, &mut registry, &lock).is_err());
        assert_move_rows(&plan, &registry, true, !insert);
        let mut present = if insert { old.clone() } else { new.clone() };
        assert_eq!(
            fs.read(present.id, &present.to_placement().1).unwrap(),
            Observation::IntactLink
        );
        assert!(
            std::fs::read_link(present.path.to_path_buf())
                .unwrap()
                .is_absolute()
        );
        assert_eq!(
            std::fs::read(present.path.to_path_buf()).unwrap(),
            b"shared content"
        );
        assert_eq!(old.path.to_path_buf().exists(), insert);
        assert_eq!(new.path.to_path_buf().exists(), !insert);
        registry.clear_write_failure();
        drop(registry);
        let mut registry = Registry::open_or_create(&fixture.home, &lock).unwrap();
        let retry = fixture.plan(&registry);
        apply_relocate(&retry, &fs, &mut registry, &lock).unwrap();
        assert_move_rows(&plan, &registry, false, true);
        present.path = new.path.clone();
        assert_eq!(
            fs.read(present.id, &present.to_placement().1).unwrap(),
            Observation::IntactLink
        );
        assert!(
            std::fs::read_link(present.path.to_path_buf())
                .unwrap()
                .is_absolute()
        );
    }
}
