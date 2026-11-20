use super::*;
use crate::placement::PlacementError;
use crate::test_support::RelocateFixture;

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
