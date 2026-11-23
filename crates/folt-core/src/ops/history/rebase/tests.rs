use super::stopped::RebaseStep;
use super::*;
use crate::history::{Expect, RefStore};
use crate::home::HomeLock;
use crate::ops::history::{HistoryError, HistoryStores};
use crate::placement_fs::PlacementFs;
use crate::registry::{Placement, Registry};
use crate::store::ObjectStore;
use crate::test_support::TempHome;

fn fixture(
    home: &TempHome,
    lock: &mut HomeLock,
) -> (Registry, ObjectStore, RefStore, Placement, RebasePlan) {
    let (registry, objects, refs, row) = super::super::tests::conflict_fixture(&home.home, lock);
    let onto = refs
        .read(RefKind::Branch, &RefName::parse("topic").unwrap())
        .unwrap()
        .unwrap();
    let facts = gather_rebase(
        row.id,
        &RefName::parse("main").unwrap(),
        onto,
        Some(&row),
        &PlacementFs::new(&home.home),
        &HistoryStores {
            objects: &objects,
            refs: &refs,
            object_limit: 16384,
        },
        &registry,
    )
    .unwrap();
    let plan = judge_rebase(&facts, false).unwrap();
    (registry, objects, refs, row, plan)
}

fn fail_at(target: RebaseStep) -> impl FnMut(RebaseStep) -> Result<(), HistoryError> {
    move |step| {
        if step == target {
            Err(HistoryRefusal::InvalidFacts.into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn replay_stop_keeps_prior_tip_current_commit_and_markers() {
    let home = TempHome::new().unwrap();
    let mut lock = home.home.try_lock().unwrap();
    let (_, _, _, row, plan) = fixture(&home, &mut lock);
    let facts = &plan.facts;
    let stop = plan.stop.as_ref().unwrap();
    assert_eq!(stop.current, row.commit_id.unwrap());
    assert!(stop.remaining.is_empty());
    assert_eq!(plan.tip, facts.reset.target.commit);
    assert!(plan.replayed.is_empty());
    assert!(stop.bytes.starts_with(b"<<<<<<<\n"));
    assert_eq!(stop.marker_lines.len(), 3);
    assert_eq!(judge_rebase(facts, false).unwrap(), plan);
    let mut no_copy = facts.clone();
    no_copy.copy = None;
    assert_eq!(
        judge_rebase(&no_copy, false).unwrap_err(),
        HistoryRefusal::RebaseConflict(stop.current)
    );
}

#[test]
fn row_before_file_is_recovered_by_continue_or_abort() {
    for abort in [false, true] {
        let home = TempHome::new().unwrap();
        let mut lock = home.home.try_lock().unwrap();
        let (mut registry, objects, refs, copy, plan) = fixture(&home, &mut lock);
        let fs = PlacementFs::new(&home.home);
        let stores = HistoryStores {
            objects: &objects,
            refs: &refs,
            object_limit: 16384,
        };
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
        assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
        drop(registry);
        let mut registry = Registry::open_or_create(&home.home, &lock).unwrap();
        let row = registry.placement_at(&copy.path).unwrap().unwrap();
        if abort {
            home.home
                .abort_rebase(&row, false, &fs, &stores, &mut registry, &mut lock)
                .unwrap();
            assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
            assert!(registry.stopped_rebase(row.id).unwrap().is_none());
            assert!(
                registry
                    .placement_at(&row.path)
                    .unwrap()
                    .unwrap()
                    .conflict
                    .is_none()
            );
        } else {
            assert!(matches!(
                home.home
                    .continue_rebase(&row, &fs, &stores, &mut registry, &mut lock)
                    .unwrap(),
                RebaseOutcome::Stopped(_)
            ));
            assert_eq!(
                std::fs::read(copy.path.to_path_buf()).unwrap(),
                plan.stop.as_ref().unwrap().bytes
            );
        }
        assert_eq!(
            refs.read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            copy.commit_id
        );
    }
}

#[test]
fn continue_recovers_object_publication_row_update_branch_movement_and_file_write() {
    for step in [
        RebaseStep::ObjectsPublished,
        RebaseStep::RowWritten,
        RebaseStep::BranchMoved,
        RebaseStep::FileWritten,
    ] {
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
        std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
        let expected = replay::resolved(
            &plan.facts.commits[&copy.commit_id.unwrap()],
            plan.tip,
            b"resolved\n".to_vec(),
        )
        .unwrap()
        .0;
        assert!(
            super::resume::continue_steps(
                &row,
                &fs,
                &stores,
                &mut registry,
                &mut lock,
                fail_at(step)
            )
            .is_err()
        );
        assert!(objects.get_commit(expected, 16384).is_ok());
        drop(registry);
        let mut registry = Registry::open_or_create(&home.home, &lock).unwrap();
        let row = registry.placement_at(&copy.path).unwrap().unwrap();
        assert_eq!(
            home.home
                .continue_rebase(&row, &fs, &stores, &mut registry, &mut lock)
                .unwrap(),
            RebaseOutcome::Finished(expected)
        );
        assert!(registry.stopped_rebase(row.id).unwrap().is_none());
        let finished = registry.placement_at(&copy.path).unwrap().unwrap();
        assert!(finished.conflict.is_none());
        assert_eq!(finished.commit_id, Some(expected));
        assert_eq!(
            std::fs::read(copy.path.to_path_buf()).unwrap(),
            b"resolved\n"
        );
        assert_eq!(
            refs.read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            Some(expected)
        );
    }
}

#[test]
fn start_file_written_failure_retains_abortable_progress() {
    let home = TempHome::new().unwrap();
    let mut lock = home.home.try_lock().unwrap();
    let (mut registry, objects, refs, copy, plan) = fixture(&home, &mut lock);
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &objects,
        refs: &refs,
        object_limit: 16384,
    };
    assert!(
        stopped::apply_stop_steps(
            &plan,
            &fs,
            &stores,
            &mut registry,
            &mut lock,
            fail_at(RebaseStep::FileWritten)
        )
        .is_err()
    );
    let row = registry.placement_at(&copy.path).unwrap().unwrap();
    home.home
        .abort_rebase(&row, false, &fs, &stores, &mut registry, &mut lock)
        .unwrap();
    assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
}

#[test]
fn branch_moved_refuses_even_after_final_row_and_preserves_progress() {
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
    std::fs::write(row.path.to_path_buf(), b"resolved\n").unwrap();
    assert!(
        super::resume::continue_steps(
            &row,
            &fs,
            &stores,
            &mut registry,
            &mut lock,
            fail_at(RebaseStep::RowWritten)
        )
        .is_err()
    );
    let before = registry.stopped_rebase(copy.id).unwrap();
    refs.update(
        &mut lock,
        RefKind::Branch,
        &RefName::parse("main").unwrap(),
        plan.facts.reset.target.commit,
        Expect::Is(copy.commit_id.unwrap()),
    )
    .unwrap();
    assert!(matches!(
        home.home
            .continue_rebase(&row, &fs, &stores, &mut registry, &mut lock),
        Err(HistoryError::Refused(HistoryRefusal::BranchMoved { .. }))
    ));
    assert_eq!(registry.stopped_rebase(copy.id).unwrap(), before);
}

mod replay_cases;
