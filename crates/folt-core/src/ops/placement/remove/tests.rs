use super::*;
use crate::placement::PlacementError;
use crate::test_support::RelocateFixture;

#[test]
fn remove_crash_leaves_only_an_unregistered_file() {
    for delete_file in [false, true] {
        let (fixture, lock, mut registry) = RelocateFixture::new(false);
        let tip = crate::object::hash_blob(b"tip");
        let progress = crate::registry::StoppedRebase {
            id: fixture.row.id,
            branch: fixture.row.head.clone(),
            copy: fixture.row.path.clone(),
            original_tip: tip,
            destination: tip,
            remaining: vec![tip],
            current_replay: Some(tip),
            new_tip: tip,
            file_before: tip,
            marker_blob: Some(tip),
            discard: false,
        };
        registry
            .write(&lock, |tx| tx.insert_stopped_rebase(&progress))
            .unwrap();
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        let plan = judge_remove(&fixture.facts(), delete_file, true).unwrap();
        assert!(
            apply_remove_after_row(&plan, &fs, &mut registry, &lock, || {
                Err(PlacementError::PlacementChanged.into())
            })
            .is_err()
        );
        assert!(registry.placement_at(&fixture.row.path).unwrap().is_none());
        assert!(registry.stopped_rebase(fixture.row.id).unwrap().is_none());
        assert_eq!(
            std::fs::read(fixture.row.path.to_path_buf()).unwrap(),
            b"edited content"
        );
    }
}

#[test]
fn remove_deletes_row_with_its_conflict_state() {
    for delete_file in [false, true] {
        let (mut fixture, lock, mut registry) = RelocateFixture::new(false);
        fixture.row.conflict = crate::test_support::conflicted_facts().placement.conflict;
        registry
            .write(&lock, |tx| tx.update_placement(&fixture.row))
            .unwrap();
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        let plan = judge_remove(&fixture.facts(), delete_file, true).unwrap();
        apply_remove(&plan, &fs, &mut registry, &lock).unwrap();
        assert_eq!(registry.placement_at(&fixture.row.path).unwrap(), None);
        assert_eq!(fixture.row.path.to_path_buf().exists(), !delete_file);
    }
}
