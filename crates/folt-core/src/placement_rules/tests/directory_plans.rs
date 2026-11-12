use super::*;

#[test]
fn pending_copy_requires_every_field_to_match() {
    for field in 0..5 {
        let (fixture, lock, mut registry) = RelocateFixture::new(false);
        let plan = fixture.plan(&registry);
        let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
        let mut other = plan.target.rows[0].1.clone();
        match field {
            0 => other.head = "other".into(),
            1 => other.commit_id = None,
            2 => other.base = None,
            3 => other.staged = None,
            _ => {
                other.kind = PlacementKind::Link;
                other.head = "main".into();
                other.commit_id = None;
                other.base = None;
                other.staged = None;
            }
        }
        registry
            .write(&lock, |tx| tx.insert_placement(&other))
            .unwrap();
        let target =
            gather_relocate_target(&fixture.row, &plan.target.path, &fs, &registry).unwrap();
        assert!(!target.pending);
        assert_eq!(
            judge_relocate(&fixture.facts(), &target)
                .unwrap_err()
                .reason,
            RefusalReason::DestinationRegistered
        );
        assert!(apply_relocate(&plan, &fs, &mut registry, &lock).is_err());
        assert_eq!(registry.placement_at(&other.path).unwrap(), Some(other));
        assert_eq!(
            registry.placement_at(&fixture.row.path).unwrap(),
            Some(fixture.row.clone())
        );
    }
}

#[test]
fn directory_plan_refuses_inside_source_and_unrelated_destination_rows() {
    let (fixture, lock, mut registry) = RelocateFixture::new(true);
    let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
    let path =
        registry::PlacementPath::resolve(fixture.row.path.to_path_buf().join("child")).unwrap();
    let target = gather_relocate_target(&fixture.row, &path, &fs, &registry).unwrap();
    assert_eq!(
        judge_relocate(&fixture.facts(), &target)
            .unwrap_err()
            .reason,
        RefusalReason::InsideSource
    );
    let plan = fixture.plan(&registry);
    // A registered path under an absent destination is still a collision.
    let mut foreign = plan.target.rows[0].1.clone();
    foreign.head = "foreign".into();
    registry
        .write(&lock, |tx| tx.insert_placement(&foreign))
        .unwrap();
    let target = gather_relocate_target(&fixture.row, &plan.target.path, &fs, &registry).unwrap();
    assert_eq!(
        judge_relocate(&fixture.facts(), &target)
            .unwrap_err()
            .reason,
        RefusalReason::DestinationRegistered
    );
    assert!(apply_relocate(&plan, &fs, &mut registry, &lock).is_err());
    assert_eq!(registry.placement_at(&foreign.path).unwrap(), Some(foreign));
}

#[test]
fn a_registered_copy_replaced_by_a_directory_is_refused() {
    let (fixture, _lock, registry) = RelocateFixture::new(false);
    std::fs::remove_file(fixture.row.path.to_path_buf()).unwrap();
    std::fs::create_dir(fixture.row.path.to_path_buf()).unwrap();
    let fs = crate::placement_fs::PlacementFs::new(&fixture.home);
    let to = registry::PlacementPath::resolve(fixture.root.join("destination")).unwrap();
    let target = gather_relocate_target(&fixture.row, &to, &fs, &registry).unwrap();
    assert_eq!(
        judge_relocate(&fixture.facts(), &target)
            .unwrap_err()
            .reason,
        RefusalReason::NotAFile
    );
}
