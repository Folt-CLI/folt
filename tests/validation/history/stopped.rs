use super::*;
use folt_core::id::ObjectId;
use folt_core::ops::history::{HistoryError, HistoryRefusal, RebaseOutcome};
use folt_core::placement::{ConflictOperation, ConflictState};
use folt_core::registry::Placement;

fn fixture(
    home: &TempHome,
    lock: &mut folt_core::home::HomeLock,
    twice: bool,
) -> (TestFolio, Placement, ObjectId, ObjectId) {
    let mut folio = TestFolio::new(home, lock);
    let root = folio.commit_internal(home, lock, b"base\n");
    let first = folio.commit_internal(home, lock, b"ours\n");
    if twice {
        folio.commit_internal(home, lock, b"later\n");
    }
    let onto = folio.commit_with_parents(lock, b"theirs\n", &[root]);
    let copy = folio.copy(home, lock);
    let tip = folio.internal().commit_id.unwrap();
    let fs = PlacementFs::new(&home.home);
    let written = fs
        .refresh(
            copy.id,
            &copy.to_placement().1,
            &folio.objects,
            folio.objects.get_commit(tip, 16384).unwrap().content,
            None,
        )
        .unwrap();
    let mut copy = copy;
    copy.commit_id = Some(tip);
    copy.base = written.base;
    folio
        .registry
        .write(lock, |tx| tx.update_placement(&copy))
        .unwrap();
    (folio, copy, onto, first)
}

fn start(
    home: &TempHome,
    lock: &mut folt_core::home::HomeLock,
    folio: &mut TestFolio,
    copy: &Placement,
    onto: ObjectId,
) -> ConflictState {
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_rebase(
            copy.id,
            &RefName::parse("main").unwrap(),
            onto,
            Some(copy),
            &fs,
            &stores,
            &folio.registry,
        )
        .unwrap();
    let plan = home.home.judge_rebase(&facts, false).unwrap();
    let RebaseOutcome::Stopped(state) = home
        .home
        .apply_rebase(&plan, &fs, &stores, &mut folio.registry, lock)
        .unwrap()
    else {
        panic!("expected stop")
    };
    state
}

fn current(folio: &TestFolio, copy: &Placement) -> Placement {
    folio.registry.placement_at(&copy.path).unwrap().unwrap()
}

fn continue_now(
    home: &TempHome,
    lock: &mut folt_core::home::HomeLock,
    folio: &mut TestFolio,
    copy: &Placement,
) -> Result<RebaseOutcome, HistoryError> {
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    home.home.continue_rebase(
        &current(folio, copy),
        &PlacementFs::new(&home.home),
        &stores,
        &mut folio.registry,
        lock,
    )
}

#[test]
fn scenario_1_stop_records_replay_and_preserves_branch_and_other_placements() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, first) = fixture(&home, &mut lock, true);
    let internal = folio.internal();
    let state = start(&home, &mut lock, &mut folio, &copy, onto);
    assert_eq!(state.operation, ConflictOperation::Rebase);
    assert_eq!(state.other, first);
    assert_eq!(state.pre_merge, folt_core::object::hash_blob(b"later\n"));
    let progress = folio.registry.stopped_rebase(copy.id).unwrap().unwrap();
    assert_eq!(progress.current_replay, Some(first));
    assert_eq!(progress.new_tip, onto);
    assert_eq!(progress.remaining, vec![copy.commit_id.unwrap()]);
    assert_eq!(folio.internal(), internal);
    assert_eq!(
        std::fs::read(internal.path.to_path_buf()).unwrap(),
        b"later\n"
    );
    assert_eq!(
        folio
            .refs
            .read(RefKind::Branch, &RefName::parse("main").unwrap())
            .unwrap(),
        copy.commit_id
    );
    assert!(
        std::fs::read(copy.path.to_path_buf())
            .unwrap()
            .windows(7)
            .any(|s| s == b"<<<<<<<")
    );
}

#[test]
fn scenario_2_continue_stops_again_then_finishes_all_placements() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, true);
    start(&home, &mut lock, &mut folio, &copy, onto);
    std::fs::write(copy.path.to_path_buf(), b"resolved\n").unwrap();
    let RebaseOutcome::Stopped(state) = continue_now(&home, &mut lock, &mut folio, &copy).unwrap()
    else {
        panic!("expected second stop")
    };
    assert_eq!(state.other, copy.commit_id.unwrap());
    assert_eq!(state.pre_merge, folt_core::object::hash_blob(b"later\n"));
    std::fs::write(copy.path.to_path_buf(), b"final\n").unwrap();
    let RebaseOutcome::Finished(tip) = continue_now(&home, &mut lock, &mut folio, &copy).unwrap()
    else {
        panic!("expected finish")
    };
    assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_none());
    assert!(current(&folio, &copy).conflict.is_none());
    assert_eq!(current(&folio, &copy).commit_id, Some(tip));
    assert_eq!(folio.internal().commit_id, Some(tip));
    assert_eq!(
        std::fs::read(folio.internal().path.to_path_buf()).unwrap(),
        b"final\n"
    );
    let replayed = folio.objects.get_commit(tip, 16384).unwrap();
    let original = folio
        .objects
        .get_commit(copy.commit_id.unwrap(), 16384)
        .unwrap();
    assert_eq!(
        (
            &replayed.author,
            replayed.time,
            &replayed.message,
            &replayed.body,
            &replayed.group
        ),
        (
            &original.author,
            original.time,
            &original.message,
            &original.body,
            &original.group
        )
    );
}

#[test]
fn scenario_3_abort_restores_original_and_requires_discard_for_edits() {
    for edited in [false, true] {
        let home = TempHome::new();
        let mut lock = home.lock();
        let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
        start(&home, &mut lock, &mut folio, &copy, onto);
        if edited {
            std::fs::write(copy.path.to_path_buf(), b"edited\n").unwrap();
        }
        let stores = HistoryStores {
            objects: &folio.objects,
            refs: &folio.refs,
            object_limit: 16384,
        };
        let row = current(&folio, &copy);
        let fs = PlacementFs::new(&home.home);
        if edited {
            assert!(matches!(
                home.home
                    .abort_rebase(&row, false, &fs, &stores, &mut folio.registry, &mut lock),
                Err(HistoryError::Refused(HistoryRefusal::Move(_)))
            ));
            assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_some());
        }
        home.home
            .abort_rebase(&row, edited, &fs, &stores, &mut folio.registry, &mut lock)
            .unwrap();
        assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
        assert_eq!(
            folio
                .refs
                .read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            copy.commit_id
        );
        assert!(current(&folio, &copy).conflict.is_none());
        assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_none());
        assert!(matches!(
            home.home.abort_rebase(
                &current(&folio, &copy),
                false,
                &fs,
                &stores,
                &mut folio.registry,
                &mut lock
            ),
            Err(HistoryError::Refused(HistoryRefusal::NoStoppedRebase))
        ));
    }
}

#[test]
fn scenario_4_continue_recovers_durable_row_before_marker_file() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
    let state = start(&home, &mut lock, &mut folio, &copy, onto);
    let markers = std::fs::read(copy.path.to_path_buf()).unwrap();
    // Reconstruct the durable state at the row-before-file crash boundary.
    let mut progress = folio.registry.stopped_rebase(copy.id).unwrap().unwrap();
    progress.file_before = state.pre_merge;
    folio
        .registry
        .write(&lock, |tx| tx.update_stopped_rebase(&progress))
        .unwrap();
    std::fs::write(copy.path.to_path_buf(), b"ours\n").unwrap();
    drop(folio.registry);
    folio.registry = folt_core::registry::Registry::open_or_create(&home.home, &lock).unwrap();
    assert_eq!(
        continue_now(&home, &mut lock, &mut folio, &copy).unwrap(),
        RebaseOutcome::Stopped(state)
    );
    assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), markers);
}

#[test]
fn scenario_5_continue_refuses_markers_and_unrelated_branch_tip() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
    start(&home, &mut lock, &mut folio, &copy, onto);
    let before = folio.registry.stopped_rebase(copy.id).unwrap();
    assert!(matches!(
        continue_now(&home, &mut lock, &mut folio, &copy),
        Err(HistoryError::Refused(HistoryRefusal::Move(
            folt_core::ops::refusal::PlacementRefusal {
                reason: folt_core::ops::refusal::RefusalReason::ConflictMarkers,
                ..
            }
        )))
    ));
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &RefName::parse("main").unwrap(),
            onto,
            Expect::Is(copy.commit_id.unwrap()),
        )
        .unwrap();
    assert!(matches!(
        continue_now(&home, &mut lock, &mut folio, &copy),
        Err(HistoryError::Refused(HistoryRefusal::BranchMoved { .. }))
    ));
    assert_eq!(folio.registry.stopped_rebase(copy.id).unwrap(), before);
    assert!(current(&folio, &copy).conflict.is_some());
}

#[test]
fn scenario_6_start_refuses_existing_stop_and_no_copy_conflict() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, first) = fixture(&home, &mut lock, false);
    let fs = PlacementFs::new(&home.home);
    let main = RefName::parse("main").unwrap();
    let facts = home
        .home
        .gather_rebase(
            copy.id,
            &main,
            onto,
            None,
            &fs,
            &folio.stores(),
            &folio.registry,
        )
        .unwrap();
    assert_eq!(
        home.home.judge_rebase(&facts, false).unwrap_err(),
        HistoryRefusal::RebaseConflict(first)
    );
    start(&home, &mut lock, &mut folio, &copy, onto);
    assert!(matches!(
        home.home.gather_rebase(
            copy.id,
            &main,
            onto,
            Some(&current(&folio, &copy)),
            &fs,
            &folio.stores(),
            &folio.registry
        ),
        Err(HistoryError::Refused(HistoryRefusal::RebaseStopped))
    ));
}

#[test]
fn scenario_7_new_registry_has_stopped_table_and_refuses_old_schemas() {
    use folt_core::registry::Registry;
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    assert!(
        folio
            .registry
            .stopped_rebase(folio.folio.id())
            .unwrap()
            .is_none()
    );
    let path = home.path().join("registry.db");
    drop(folio.registry);
    let mut bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[60..64], &5_u32.to_be_bytes());
    for version in 1_i64..5 {
        bytes[60..64].copy_from_slice(&(version as u32).to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            matches!(Registry::open_or_create(&home.home, &lock), Err(folt_core::placement_path::RegistryError::OutdatedSchema { found, required: 5, .. }) if found == version)
        );
        assert!(matches!(
            Registry::open_existing(&home.home),
            Err(folt_core::placement_path::RegistryError::OutdatedSchema { .. })
        ));
    }
}

#[test]
fn scenario_4_continue_recovers_published_objects_and_moved_branch() {
    for moved in [false, true] {
        let home = TempHome::new();
        let mut lock = home.lock();
        let (mut folio, copy, onto, first) = fixture(&home, &mut lock, false);
        start(&home, &mut lock, &mut folio, &copy, onto);
        std::fs::write(copy.path.to_path_buf(), b"resolved\n").unwrap();
        let content = folio
            .objects
            .put(&lock, folt_core::object::ObjectKind::Blob, b"resolved\n")
            .unwrap();
        let mut replay = folio.objects.get_commit(first, 16384).unwrap();
        replay.parents = vec![onto];
        replay.content = content;
        let expected = folio.objects.put_commit(&lock, &replay).unwrap();
        folio.objects.sync_blobs().unwrap();
        if moved {
            let mut progress = folio.registry.stopped_rebase(copy.id).unwrap().unwrap();
            progress.current_replay = None;
            progress.remaining.clear();
            progress.marker_blob = None;
            progress.file_before = content;
            progress.new_tip = expected;
            folio
                .registry
                .write(&lock, |tx| tx.update_stopped_rebase(&progress))
                .unwrap();
            folio
                .refs
                .update(
                    &mut lock,
                    RefKind::Branch,
                    &RefName::parse("main").unwrap(),
                    expected,
                    Expect::Is(copy.commit_id.unwrap()),
                )
                .unwrap();
            folio.refs.sync_refs().unwrap();
        }
        drop(folio.registry);
        folio.registry = folt_core::registry::Registry::open_or_create(&home.home, &lock).unwrap();
        assert_eq!(
            continue_now(&home, &mut lock, &mut folio, &copy).unwrap(),
            RebaseOutcome::Finished(expected)
        );
        assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_none());
        assert_eq!(current(&folio, &copy).commit_id, Some(expected));
        assert!(current(&folio, &copy).conflict.is_none());
        assert_eq!(folio.internal().commit_id, Some(expected));
        assert_eq!(
            std::fs::read(folio.internal().path.to_path_buf()).unwrap(),
            b"resolved\n"
        );
    }
}

#[test]
fn continue_accepts_resolution_equal_to_pre_rebase_bytes() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
    start(&home, &mut lock, &mut folio, &copy, onto);
    std::fs::write(copy.path.to_path_buf(), b"ours\n").unwrap();
    assert!(matches!(
        continue_now(&home, &mut lock, &mut folio, &copy).unwrap(),
        RebaseOutcome::Finished(_)
    ));
    assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
    assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_none());
}

mod abort;

mod remove;
