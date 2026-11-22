use folt_core::merge::{MergeResult, merge3};
use folt_core::object::ObjectKind;
use folt_core::ops::history::{
    HistoryError, HistoryRefusal, HistoryStores, MergeOutcome, MergeRequest,
};
use folt_core::placement::ConflictOperation;
use folt_core::placement_fs::PlacementFs;
use folt_core::ref_name::{RefKind, RefName};
use folt_core::registry::Registry;

use crate::branch_merge::copy;
use crate::common::TempHome;
use crate::support::{TestFolio, metadata};

fn conflict_case(base: &[u8], ours: &[u8], theirs: &[u8]) {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base_id = folio.commit_internal(&home, &mut lock, base);
    let other = folio.second_branch(&home, &mut lock, "topic", base_id, theirs);
    let tip = folio.commit_internal(&home, &mut lock, ours);
    let row = copy(&home, &mut folio, &lock, "main", "first.md");
    let second = copy(&home, &mut folio, &lock, "main", "second.md");
    let internal = folio.internal();
    let before: Vec<_> = std::fs::read_dir(folio.folio.path().join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let topic = RefName::parse("topic").unwrap();
    let outcome = home
        .home
        .merge_branch(
            &row,
            MergeRequest {
                other: &topic,
                metadata: metadata(),
            },
            &PlacementFs::new(&home.home),
            &HistoryStores {
                objects: &folio.objects,
                refs: &folio.refs,
                object_limit: 16384,
            },
            &mut folio.registry,
            &mut lock,
        )
        .unwrap();
    let MergeOutcome::Conflicted(state) = outcome else {
        panic!("expected conflict state")
    };
    let MergeResult::Conflicted {
        merged: Some(expected),
        marker_lines,
    } = merge3(base, ours, theirs)
    else {
        panic!("expected text conflict")
    };
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), expected);
    assert_eq!(state.operation, ConflictOperation::Merge);
    assert_eq!(state.other, other);
    assert_eq!(state.marker_lines, marker_lines);
    assert_eq!(
        folio
            .objects
            .get(state.pre_merge, ObjectKind::Blob, u64::MAX)
            .unwrap(),
        ours
    );
    assert_eq!(
        home.home
            .conflict_state(&row.path, &folio.registry)
            .unwrap(),
        Some(state.clone())
    );
    let mut expected_row = row.clone();
    expected_row.conflict = Some(state.clone());
    assert_eq!(
        folio.registry.placement_at(&row.path).unwrap(),
        Some(expected_row)
    );
    assert_eq!(
        folio
            .refs
            .read(RefKind::Branch, &RefName::parse("main").unwrap())
            .unwrap(),
        Some(tip)
    );
    for untouched in [&second, &internal] {
        assert_eq!(
            folio
                .registry
                .placement_at(&untouched.path)
                .unwrap()
                .as_ref(),
            Some(untouched)
        );
        assert_eq!(std::fs::read(untouched.path.to_path_buf()).unwrap(), ours);
        assert_eq!(
            home.home
                .conflict_state(&untouched.path, &folio.registry)
                .unwrap(),
            None
        );
    }
    for entry in std::fs::read_dir(folio.folio.path().join("blobs")).unwrap() {
        let path = entry.unwrap().path();
        if !before.contains(&path) {
            let id = path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .parse::<folt_core::id::ObjectId>()
                .unwrap();
            assert_eq!(folio.objects.kind_of(id).unwrap(), ObjectKind::Blob);
        }
    }
    drop(folio.registry);
    let reopened = Registry::open_existing(&home.home).unwrap();
    assert_eq!(
        home.home.conflict_state(&row.path, &reopened).unwrap(),
        Some(state)
    );
    if base.starts_with(b"---\n") {
        assert!(expected.starts_with(b"---\n<<<<<<<\n"));
        assert!(
            expected
                .windows(b">>>>>>>\n---\n".len())
                .any(|bytes| bytes == b">>>>>>>\n---\n")
        );
    }
}

#[test]
fn scenarios_1_to_3_copy_conflict_persists_without_advancing_history_or_other_placements() {
    conflict_case(b"base\n", b"ours\n", b"theirs\n");
}

#[test]
fn scenario_5_conflict_markers_remain_inside_front_matter() {
    conflict_case(
        b"---\ntitle: base\n---\nbody\n",
        b"---\ntitle: ours\n---\nbody\n",
        b"---\ntitle: theirs\n---\nbody\n",
    );
}

#[test]
fn scenario_6_internal_text_and_copy_binary_conflicts_write_nothing() {
    for binary in [false, true] {
        let home = TempHome::new();
        let mut lock = home.lock();
        let mut folio = TestFolio::new(&home, &lock);
        let base = folio.commit_internal(&home, &mut lock, b"base\n");
        folio.second_branch(&home, &mut lock, "topic", base, b"theirs\n");
        let ours: &[u8] = if binary { b"ours\0\n" } else { b"ours\n" };
        let tip = folio.commit_internal(&home, &mut lock, ours);
        let row = if binary {
            copy(&home, &mut folio, &lock, "main", "copy.bin")
        } else {
            folio.internal()
        };
        let before = super::branch_merge::snapshot(home.root(), &home.path().join("lock"));
        let error = home
            .home
            .merge_branch(
                &row,
                MergeRequest {
                    other: &RefName::parse("topic").unwrap(),
                    metadata: metadata(),
                },
                &PlacementFs::new(&home.home),
                &HistoryStores {
                    objects: &folio.objects,
                    refs: &folio.refs,
                    object_limit: 16384,
                },
                &mut folio.registry,
                &mut lock,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            HistoryError::Refused(HistoryRefusal::MergeConflict)
        ));
        assert_eq!(
            super::branch_merge::snapshot(home.root(), &home.path().join("lock")),
            before
        );
        assert_eq!(
            folio.registry.placement_at(&row.path).unwrap(),
            Some(row.clone())
        );
        assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), ours);
        assert_eq!(
            folio
                .refs
                .read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            Some(tip)
        );
    }
}
