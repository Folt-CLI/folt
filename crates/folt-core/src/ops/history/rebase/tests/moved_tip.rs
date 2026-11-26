use super::*;

fn advance_from_other_copy(
    home: &TempHome,
    copy: &Placement,
    stores: &HistoryStores<'_>,
    registry: &mut Registry,
    lock: &mut HomeLock,
) -> Placement {
    use crate::placement::{PlacementKind, PlacementOps};
    use crate::placement_path::PlacementPath;
    let fs = PlacementFs::new(&home.home);
    let mut internal = copy.clone();
    internal.kind = PlacementKind::Internal;
    internal.path =
        PlacementPath::resolve(crate::folio::dir::path::content_path(&home.home, copy.id)).unwrap();
    internal.base = fs
        .refresh(
            copy.id,
            &internal.to_placement().1,
            stores.objects,
            hash_blob(b"ours\n"),
            None,
        )
        .unwrap()
        .base;
    let mut other = copy.clone();
    other.path = PlacementPath::resolve(home.home.path().join("other.txt")).unwrap();
    other.base = fs
        .create(
            copy.id,
            &other.to_placement().1,
            stores.objects,
            hash_blob(b"ours\n"),
        )
        .unwrap()
        .base;
    registry
        .write(lock, |tx| {
            tx.insert_placement(&internal)?;
            tx.insert_placement(&other)
        })
        .unwrap();
    std::fs::write(other.path.to_path_buf(), b"advanced\n").unwrap();
    crate::ops::stage::stage(&other, &fs, stores.objects, stores.refs, registry, lock).unwrap();
    let other = registry.placement_at(&other.path).unwrap().unwrap();
    let facts = crate::ops::history::gather_commit(&other, &fs, stores, registry).unwrap();
    let plan = crate::ops::history::judge_commit(&facts).unwrap();
    crate::ops::history::apply_commit(
        &plan,
        crate::ops::history::CommitMetadata {
            author: crate::object::Author::anonymous(),
            time: crate::object::CommitTime::from_unix_seconds(1).unwrap(),
            message: "advance from another copy".into(),
            body: None,
            group: None,
        },
        &fs,
        stores,
        registry,
        lock,
    )
    .unwrap();
    registry.placement_at(&other.path).unwrap().unwrap()
}

fn file_snapshot(
    path: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(file_snapshot(&path));
        } else {
            files.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    files
}

#[test]
fn moved_tip_abort_restores_copy_and_protects_edited_resolution() {
    use crate::ops::refusal::RefusalReason;
    for (edited, discard) in [(false, false), (true, false), (true, true)] {
        let home = TempHome::new().unwrap();
        let mut lock = home.home.try_lock().unwrap();
        let (mut registry, objects, refs, copy, plan) = fixture(&home, &mut lock);
        let fs = PlacementFs::new(&home.home);
        let stores = HistoryStores {
            objects: &objects,
            refs: &refs,
            object_limit: 16384,
        };
        stopped::apply_stop(&plan, &fs, &stores, &mut registry, &mut lock).unwrap();
        let other = advance_from_other_copy(&home, &copy, &stores, &mut registry, &mut lock);
        let mut row = registry.placement_at(&copy.path).unwrap().unwrap();
        if edited {
            std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
            crate::ops::stage::stage(&row, &fs, &objects, &refs, &mut registry, &lock).unwrap();
            row = registry.placement_at(&copy.path).unwrap().unwrap();
            assert!(row.staged.is_some());
        }
        let before = registry.stopped_rebase(copy.id).unwrap();
        if edited && !discard {
            assert!(matches!(
                home.home.abort_rebase(&row, false, &fs, &stores, &mut registry, &mut lock),
                Err(HistoryError::Refused(HistoryRefusal::Move(error)))
                    if error.reason == RefusalReason::Edited
            ));
            assert_eq!(
                std::fs::read(row.path.to_path_buf()).unwrap(),
                b"resolved\n"
            );
            assert_eq!(registry.placement_at(&row.path).unwrap(), Some(row.clone()));
            assert_eq!(registry.stopped_rebase(copy.id).unwrap(), before);
        }
        home.home
            .abort_rebase(
                &row,
                discard || edited,
                &fs,
                &stores,
                &mut registry,
                &mut lock,
            )
            .unwrap();
        let restored = registry.placement_at(&copy.path).unwrap().unwrap();
        assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
        assert_eq!(restored.base.as_ref().unwrap().hash, hash_blob(b"ours\n"));
        assert_eq!(restored.commit_id, copy.commit_id);
        assert_eq!(
            crate::ops::status::judge_working(
                &crate::ops::history::gather_file(&restored, &fs, &refs).unwrap()
            )
            .edit,
            crate::ops::status::Edit::Clean
        );
        assert!(restored.staged.is_none());
        assert!(restored.conflict.is_none());
        assert!(registry.stopped_rebase(copy.id).unwrap().is_none());
        assert_eq!(
            refs.read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            other.commit_id
        );
        assert_eq!(
            registry.placement_at(&other.path).unwrap(),
            Some(other.clone())
        );
        assert_eq!(
            std::fs::read(other.path.to_path_buf()).unwrap(),
            b"advanced\n"
        );
        assert_eq!(
            objects
                .get(hash_blob(b"ours\n"), ObjectKind::Blob, 16384)
                .unwrap(),
            b"ours\n"
        );
    }
}

#[test]
fn moved_tip_continue_refuses_before_recovery_or_publication() {
    for row_before_file in [false, true] {
        let home = TempHome::new().unwrap();
        let mut lock = home.home.try_lock().unwrap();
        let (mut registry, objects, refs, copy, plan) = fixture(&home, &mut lock);
        let fs = PlacementFs::new(&home.home);
        let stores = HistoryStores {
            objects: &objects,
            refs: &refs,
            object_limit: 16384,
        };
        if row_before_file {
            assert!(
                stopped::apply_stop_steps(
                    &plan,
                    &fs,
                    &stores,
                    &mut registry,
                    &mut lock,
                    fail_at(RebaseStep::RowWritten)
                )
                .is_err()
            );
        } else {
            stopped::apply_stop(&plan, &fs, &stores, &mut registry, &mut lock).unwrap();
            std::fs::write(copy.path.to_path_buf(), b"resolved\n").unwrap();
        }
        let other = advance_from_other_copy(&home, &copy, &stores, &mut registry, &mut lock);
        let row = registry.placement_at(&copy.path).unwrap().unwrap();
        let progress = registry.stopped_rebase(copy.id).unwrap();
        let folio_path = crate::folio::dir::path::content_path(&home.home, copy.id);
        let files = file_snapshot(folio_path.parent().unwrap());
        let bytes = std::fs::read(copy.path.to_path_buf()).unwrap();
        let placements = registry.placements_of_head(copy.id, "main").unwrap();
        let error = home
            .home
            .continue_rebase(&row, &fs, &stores, &mut registry, &mut lock)
            .unwrap_err();
        assert!(
            matches!(&error, HistoryError::Refused(HistoryRefusal::BranchMoved { tip, ahead }) if *tip == other.commit_id && ahead.is_empty())
        );
        let message = error.to_string();
        assert!(message.contains("rebase abort"));
        assert!(message.contains("discard"));
        assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), bytes);
        assert_eq!(registry.stopped_rebase(copy.id).unwrap(), progress);
        assert_eq!(
            registry.placements_of_head(copy.id, "main").unwrap(),
            placements
        );
        assert_eq!(file_snapshot(folio_path.parent().unwrap()), files);
        assert_eq!(
            std::fs::read(other.path.to_path_buf()).unwrap(),
            b"advanced\n"
        );
    }
}

#[test]
fn recorded_new_tip_is_refused_during_replay_and_abort_refuses_completed_replay() {
    let home = TempHome::new().unwrap();
    let mut lock = home.home.try_lock().unwrap();
    let (mut registry, objects, refs, copy, plan) = fixture(&home, &mut lock);
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &objects,
        refs: &refs,
        object_limit: 16384,
    };
    stopped::apply_stop(&plan, &fs, &stores, &mut registry, &mut lock).unwrap();
    let row = registry.placement_at(&copy.path).unwrap().unwrap();
    let mut progress = registry.stopped_rebase(copy.id).unwrap().unwrap();
    refs.update(
        &mut lock,
        RefKind::Branch,
        &RefName::parse("main").unwrap(),
        progress.new_tip,
        Expect::Is(progress.original_tip),
    )
    .unwrap();
    assert!(matches!(
        home.home
            .continue_rebase(&row, &fs, &stores, &mut registry, &mut lock),
        Err(HistoryError::Refused(HistoryRefusal::BranchMoved { .. }))
    ));
    progress.current_replay = None;
    registry
        .write(&lock, |tx| tx.update_stopped_rebase(&progress))
        .unwrap();
    assert!(matches!(
        home.home
            .abort_rebase(&row, true, &fs, &stores, &mut registry, &mut lock),
        Err(HistoryError::Refused(HistoryRefusal::InvalidFacts))
    ));
    assert_eq!(registry.stopped_rebase(copy.id).unwrap(), Some(progress));
}

#[test]
fn branch_moved_guidance_preserves_tip_and_ahead_details() {
    let tip = hash_blob(b"tip");
    let refusal = HistoryRefusal::BranchMoved {
        tip: Some(tip),
        ahead: vec!["other.txt".into()],
    };
    let message = refusal.to_string();
    assert!(message.starts_with(&format!(
        "branch moved to {:?}; ahead at other.txt",
        Some(tip)
    )));
    assert!(message.contains("if a rebase is stopped"));
    assert!(message.contains("rebase abort"));
    assert!(message.contains("discard"));
}
