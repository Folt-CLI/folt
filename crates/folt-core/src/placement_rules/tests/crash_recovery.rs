use super::*;

#[test]
fn prune_interrupted_moves_and_rollback_with_existing_hooks() {
    for crash in [RelocateStep::Inserted, RelocateStep::Renamed] {
        let (fixture, lock, mut registry) = RelocateFixture::new(false);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
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
        let result = crate::prune::prune_placements(
            &mut registry,
            &fs,
            &lock,
            &[],
            crate::prune::PruneOptions {
                apply: true,
                confirm_mass_deletion: false,
            },
        )
        .unwrap();
        assert_eq!(result.dropped.len(), plan.target.rows.len());
        assert_move_rows(
            &plan,
            &registry,
            crash == RelocateStep::Inserted,
            crash == RelocateStep::Renamed,
        );
        for row in registry.all_placements().unwrap() {
            assert_ne!(
                fs.read(row.id, &row.to_placement().1).unwrap(),
                Observation::Missing
            );
            assert_eq!(row.staged, plan.target.rows[0].0.staged);
        }
    }
    let (fixture, lock, mut registry) = RelocateFixture::new(true);
    let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
    let rows = registry.all_placements().unwrap();
    for row in &rows {
        std::fs::remove_file(row.path.to_path_buf()).unwrap();
    }
    registry.fail_writes_at(&rows[1].path, false);
    assert!(
        crate::prune::prune_placements(
            &mut registry,
            &fs,
            &lock,
            &[],
            crate::prune::PruneOptions {
                apply: true,
                confirm_mass_deletion: true
            },
        )
        .is_err()
    );
    assert_eq!(registry.all_placements().unwrap(), rows);
}

#[test]
fn prune_preserves_missing_parents_after_directory_move_interruptions() {
    for crash in [RelocateStep::Inserted, RelocateStep::Renamed] {
        let (fixture, lock, mut registry) = RelocateFixture::new(true);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
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
        let before = registry.all_placements().unwrap();
        let report = crate::prune::prune_placements(
            &mut registry,
            &fs,
            &lock,
            &[],
            crate::prune::PruneOptions {
                apply: true,
                confirm_mass_deletion: false,
            },
        )
        .unwrap();
        assert!(report.dropped.is_empty());
        assert_eq!(
            report
                .kept
                .iter()
                .filter(|row| row.reason == "parent directory missing")
                .count(),
            3
        );
        assert_eq!(registry.all_placements().unwrap(), before);
    }
}

#[test]
fn move_crash_windows_reopen_and_reuse_pending_rows() {
    for directory in [false, true] {
        for crash in [
            RelocateStep::Inserted,
            RelocateStep::Renamed,
            RelocateStep::Deleted,
        ] {
            let (fixture, lock, mut registry) = RelocateFixture::new(directory);
            let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
            let plan = fixture.plan(&registry);
            assert_eq!(plan.target.rows.len(), if directory { 3 } else { 1 });
            let result =
                apply_relocate_steps(&plan, &fs, &mut registry, &lock, |step, registry| {
                    assert_move_rows(&plan, registry, step != RelocateStep::Deleted, true);
                    if step == crash {
                        Err(PlacementError::PlacementChanged.into())
                    } else {
                        Ok(())
                    }
                });
            assert!(result.is_err());
            assert_eq!(
                fixture.row.path.to_path_buf().exists(),
                crash == RelocateStep::Inserted
            );
            assert_eq!(
                plan.target.path.to_path_buf().exists(),
                crash != RelocateStep::Inserted
            );
            drop(registry);
            let mut registry = Registry::open_or_create(&fixture.home, &lock).unwrap();
            // Re-gather after a restart while the old rows remain. Reusing a
            // pending set must succeed even though another INSERT would fail.
            let retry = if crash == RelocateStep::Deleted {
                plan.clone()
            } else {
                fixture.plan(&registry)
            };
            apply_relocate(&retry, &fs, &mut registry, &lock).unwrap();
            assert_move_rows(&plan, &registry, false, true);
            apply_relocate(&plan, &fs, &mut registry, &lock).unwrap();
            if directory {
                for index in 0..3 {
                    assert_eq!(
                        std::fs::read(plan.target.path.to_path_buf().join(format!("file-{index}")))
                            .unwrap(),
                        b"edited content"
                    );
                }
            } else {
                assert_eq!(
                    std::fs::read(plan.target.path.to_path_buf()).unwrap(),
                    b"edited content"
                );
            }
        }
    }
}

#[test]
fn remove_crash_leaves_only_an_unregistered_file() {
    for delete_file in [false, true] {
        let (fixture, lock, mut registry) = RelocateFixture::new(false);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        let plan = judge_remove(&fixture.facts(), delete_file, true).unwrap();
        assert!(
            apply_remove_after_row(&plan, &fs, &mut registry, &lock, || {
                Err(PlacementError::PlacementChanged.into())
            })
            .is_err()
        );
        assert!(registry.placement_at(&fixture.row.path).unwrap().is_none());
        assert_eq!(
            std::fs::read(fixture.row.path.to_path_buf()).unwrap(),
            b"edited content"
        );
    }
}
