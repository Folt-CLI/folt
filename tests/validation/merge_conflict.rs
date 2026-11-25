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

struct ConflictedCopies {
    home: TempHome,
    folio: TestFolio,
    lock: folt_core::home::HomeLock,
    flagged: folt_core::registry::Placement,
    second: folt_core::registry::Placement,
    base: folt_core::id::ObjectId,
}

impl ConflictedCopies {
    fn new() -> Self {
        Self::with_content(b"base\n", b"ours\n", b"theirs\n")
    }

    fn with_content(base_bytes: &[u8], ours: &[u8], theirs: &[u8]) -> Self {
        let home = TempHome::new();
        let mut lock = home.lock();
        let mut folio = TestFolio::new(&home, &lock);
        let base = folio.commit_internal(&home, &mut lock, base_bytes);
        folio.second_branch(&home, &mut lock, "topic", base, theirs);
        folio.commit_internal(&home, &mut lock, ours);
        let flagged = copy(&home, &mut folio, &lock, "main", "flagged.md");
        let second = copy(&home, &mut folio, &lock, "main", "ordinary.md");
        let outcome = home
            .home
            .merge_branch(
                &flagged,
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
            .unwrap();
        assert!(matches!(outcome, MergeOutcome::Conflicted(_)));
        let flagged = folio.registry.placement_at(&flagged.path).unwrap().unwrap();
        Self {
            home,
            folio,
            lock,
            flagged,
            second,
            base,
        }
    }

    fn facts(&self, row: &folt_core::registry::Placement) -> folt_core::ops::facts::PlacementFacts {
        self.home
            .home
            .gather_working(
                row,
                &PlacementFs::new(&self.home.home),
                &self.folio.refs,
                &self.folio.registry,
            )
            .unwrap()
    }

    fn assert_flagged_unchanged(&self, bytes: &[u8]) {
        assert_eq!(
            self.folio
                .registry
                .placement_at(&self.flagged.path)
                .unwrap()
                .as_ref(),
            Some(&self.flagged)
        );
        assert_eq!(
            std::fs::read(self.flagged.path.to_path_buf()).unwrap(),
            bytes
        );
    }
}

fn assert_copy_refusal(error: impl std::fmt::Display, row: &folt_core::registry::Placement) {
    let message = error.to_string();
    assert!(
        message.contains(&row.path.to_path_buf().display().to_string()),
        "{message}"
    );
    assert!(message.contains("conflict"), "{message}");
}

#[test]
fn conflicted_copy_refuses_stage_and_commit_while_second_copy_commits() {
    let mut case = ConflictedCopies::new();
    let fs = PlacementFs::new(&case.home.home);
    let stores = HistoryStores {
        objects: &case.folio.objects,
        refs: &case.folio.refs,
        object_limit: 16384,
    };
    let bytes = std::fs::read(case.flagged.path.to_path_buf()).unwrap();
    let error = case
        .home
        .home
        .stage(
            &case.flagged,
            &fs,
            stores.objects,
            stores.refs,
            &mut case.folio.registry,
            &case.lock,
        )
        .unwrap_err();
    assert_copy_refusal(error, &case.flagged);
    let facts = case
        .home
        .home
        .gather_commit(&case.flagged, &fs, &stores, &case.folio.registry)
        .unwrap();
    assert_copy_refusal(
        case.home.home.judge_commit(&facts).unwrap_err(),
        &case.flagged,
    );
    std::fs::write(case.second.path.to_path_buf(), b"second copy edit\n").unwrap();
    case.home
        .home
        .stage(
            &case.second,
            &fs,
            stores.objects,
            stores.refs,
            &mut case.folio.registry,
            &case.lock,
        )
        .unwrap();
    let second = case
        .folio
        .registry
        .placement_at(&case.second.path)
        .unwrap()
        .unwrap();
    let facts = case
        .home
        .home
        .gather_commit(&second, &fs, &stores, &case.folio.registry)
        .unwrap();
    let plan = case.home.home.judge_commit(&facts).unwrap();
    case.home
        .home
        .apply_commit(
            &plan,
            metadata(),
            &fs,
            &stores,
            &mut case.folio.registry,
            &mut case.lock,
        )
        .unwrap();
    case.assert_flagged_unchanged(&bytes);
    assert_eq!(
        std::fs::read(case.folio.internal().path.to_path_buf()).unwrap(),
        b"second copy edit\n"
    );
}

#[test]
fn stage_accepts_resolution_and_ordinary_marker_like_content() {
    for (resolved, accepted) in [
        (b"resolved\n".as_slice(), true),
        (b"Heading\n========\n", true),
        (b"<<<<<<< ours\n========\n>>>>>>> theirs\n", true),
        (b"Heading\n=======\n", false),
        (b"<<<<<<< ours\n=======\n>>>>>>>\n", false),
    ] {
        let mut case = ConflictedCopies::new();
        let fs = PlacementFs::new(&case.home.home);
        std::fs::write(case.flagged.path.to_path_buf(), resolved).unwrap();
        let result = case.home.home.stage(
            &case.flagged,
            &fs,
            &case.folio.objects,
            &case.folio.refs,
            &mut case.folio.registry,
            &case.lock,
        );
        if accepted {
            result.unwrap();
            assert!(
                case.folio
                    .registry
                    .placement_at(&case.flagged.path)
                    .unwrap()
                    .unwrap()
                    .staged
                    .is_some()
            );
        } else {
            assert!(matches!(
                result,
                Err(folt_core::ops::rules_error::RulesError::Refused(
                    folt_core::ops::refusal::PlacementRefusal {
                        reason: folt_core::ops::refusal::RefusalReason::ConflictMarkers,
                        ..
                    }
                ))
            ));
            case.assert_flagged_unchanged(resolved);
        }
        std::fs::write(
            case.second.path.to_path_buf(),
            b"<<<<<<<\n=======\n>>>>>>>\n",
        )
        .unwrap();
        case.home
            .home
            .stage(
                &case.second,
                &fs,
                &case.folio.objects,
                &case.folio.refs,
                &mut case.folio.registry,
                &case.lock,
            )
            .unwrap();
    }
}

#[test]
fn conflict_blocks_checkout_convert_and_relocate_only_for_selected_copy() {
    use folt_core::ops::checkout::CheckoutStores;
    use folt_core::ops::refusal::RefusalReason;
    use folt_core::placement::PlacementKind;
    use folt_core::placement_path::PlacementPath;
    let mut case = ConflictedCopies::new();
    let fs = PlacementFs::new(&case.home.home);
    let bytes = std::fs::read(case.flagged.path.to_path_buf()).unwrap();
    let main = RefName::parse("main").unwrap();
    let stores = CheckoutStores {
        objects: &case.folio.objects,
        refs: &case.folio.refs,
        object_limit: 16384,
    };
    let error = case
        .home
        .home
        .checkout_placement(
            &case.flagged,
            &main,
            true,
            &fs,
            &stores,
            &mut case.folio.registry,
            &case.lock,
        )
        .unwrap_err();
    assert_copy_refusal(error, &case.flagged);
    case.home
        .home
        .checkout_placement(
            &case.second,
            &main,
            false,
            &fs,
            &stores,
            &mut case.folio.registry,
            &case.lock,
        )
        .unwrap();
    case.second = case
        .folio
        .registry
        .placement_at(&case.second.path)
        .unwrap()
        .unwrap();
    for row in [&case.flagged, &case.second] {
        let facts = case.facts(row);
        let target = case
            .home
            .home
            .gather_relocate_target(
                row,
                &PlacementPath::resolve(case.home.root().join("moved.md")).unwrap(),
                &fs,
                &case.folio.registry,
            )
            .unwrap();
        let relocation = case.home.home.judge_relocate(&facts, &target);
        let conversion = case
            .home
            .home
            .judge_convert(&facts, PlacementKind::Link, None, None);
        if row.conflict.is_some() {
            for error in [relocation.unwrap_err(), conversion.unwrap_err()] {
                assert_eq!(error.reason, RefusalReason::Conflicted);
                assert_copy_refusal(error, row);
            }
        } else {
            case.home
                .home
                .apply_relocate(
                    &relocation.unwrap(),
                    &fs,
                    &mut case.folio.registry,
                    &case.lock,
                )
                .unwrap();
        }
    }
    let moved_path = PlacementPath::resolve(case.home.root().join("moved.md")).unwrap();
    let moved = case
        .folio
        .registry
        .placement_at(&moved_path)
        .unwrap()
        .unwrap();
    let facts = case.facts(&moved);
    let plan = case
        .home
        .home
        .judge_convert(&facts, PlacementKind::Link, None, None)
        .unwrap();
    case.home
        .home
        .apply_convert(
            &plan,
            &fs,
            &case.folio.objects,
            &mut case.folio.registry,
            &case.lock,
        )
        .unwrap();
    assert_eq!(
        case.folio
            .registry
            .placement_at(&moved_path)
            .unwrap()
            .unwrap()
            .kind,
        PlacementKind::Link
    );
    case.assert_flagged_unchanged(&bytes);
}

#[test]
fn reset_preserves_conflicted_copy_and_moves_other_copy_and_internal_row() {
    let mut case = ConflictedCopies::new();
    let fs = PlacementFs::new(&case.home.home);
    let bytes = std::fs::read(case.flagged.path.to_path_buf()).unwrap();
    let stores = HistoryStores {
        objects: &case.folio.objects,
        refs: &case.folio.refs,
        object_limit: 16384,
    };
    let facts = case
        .home
        .home
        .gather_reset(
            case.flagged.id,
            &RefName::parse("main").unwrap(),
            case.base,
            &fs,
            &stores,
            &case.folio.registry,
        )
        .unwrap();
    let plan = case.home.home.judge_reset(&facts, false).unwrap();
    assert_eq!(plan.skipped(), &[case.flagged.path.to_path_buf()]);
    case.home
        .home
        .apply_reset(
            &plan,
            &fs,
            &stores,
            &mut case.folio.registry,
            &mut case.lock,
        )
        .unwrap();
    case.assert_flagged_unchanged(&bytes);
    for row in [&case.second, &case.folio.internal()] {
        assert_eq!(
            case.folio
                .registry
                .placement_at(&row.path)
                .unwrap()
                .unwrap()
                .commit_id,
            Some(case.base)
        );
        assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"base\n");
    }
}

#[test]
fn status_prune_and_remove_handle_each_copys_own_conflict_state() {
    let mut case = ConflictedCopies::new();
    let fs = PlacementFs::new(&case.home.home);
    let status = case
        .home
        .home
        .gather_status(
            &[
                case.flagged.clone(),
                case.second.clone(),
                case.folio.internal(),
            ],
            &fs,
            |_| Ok(&case.folio.refs),
            &case.folio.registry,
        )
        .unwrap();
    assert_eq!(status[0].conflict, Some(ConflictOperation::Merge));
    assert_eq!(status[1].conflict, None);
    assert_eq!(status[2].conflict, None);
    let bytes = std::fs::read(case.flagged.path.to_path_buf()).unwrap();
    std::fs::remove_file(case.flagged.path.to_path_buf()).unwrap();
    std::fs::remove_file(case.second.path.to_path_buf()).unwrap();
    let report = case
        .home
        .home
        .prune_placements(
            &mut case.folio.registry,
            &fs,
            &case.lock,
            &[],
            folt_core::ops::prune::PruneOptions {
                apply: true,
                confirm_mass_deletion: false,
            },
        )
        .unwrap();
    assert!(
        report
            .kept
            .iter()
            .any(|kept| kept.path == case.flagged.path && kept.reason.contains("conflict"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("conflict")
                && warning.contains(&case.flagged.path.to_path_buf().display().to_string()))
    );
    assert!(
        report
            .dropped
            .iter()
            .any(|dropped| dropped.path == case.second.path)
    );
    assert_eq!(
        case.folio
            .registry
            .placement_at(&case.flagged.path)
            .unwrap()
            .as_ref(),
        Some(&case.flagged)
    );
    std::fs::write(case.flagged.path.to_path_buf(), &bytes).unwrap();
    let facts = case.facts(&case.flagged);
    let plan = case.home.home.judge_remove(&facts, true, true).unwrap();
    case.home
        .home
        .apply_remove(&plan, &fs, &mut case.folio.registry, &case.lock)
        .unwrap();
    assert_eq!(
        case.folio
            .registry
            .placement_at(&case.flagged.path)
            .unwrap(),
        None
    );
    assert!(!case.flagged.path.to_path_buf().exists());
    let second = copy(
        &case.home,
        &mut case.folio,
        &case.lock,
        "main",
        "remove-second.md",
    );
    let plan = case
        .home
        .home
        .judge_remove(&case.facts(&second), true, false)
        .unwrap();
    case.home
        .home
        .apply_remove(&plan, &fs, &mut case.folio.registry, &case.lock)
        .unwrap();
    assert_eq!(
        case.folio.registry.placement_at(&second.path).unwrap(),
        None
    );
}

#[test]
fn two_region_merge_refuses_partial_resolution_then_continues() {
    use folt_core::ops::refusal::{PlacementRefusal, RefusalReason};
    use folt_core::ops::rules_error::RulesError;
    let mut case = ConflictedCopies::with_content(
        b"base first\nseparator\nbase second\n",
        b"ours first\nseparator\nours second\n",
        b"theirs first\nseparator\ntheirs second\n",
    );
    let bytes = std::fs::read(case.flagged.path.to_path_buf()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(
        case.flagged.conflict.as_ref().unwrap().marker_lines.len(),
        6
    );
    let regions: Vec<_> = text.split("separator\n").collect();
    for partial in [
        format!("resolved first\nseparator\n{}", regions[1]),
        format!("{}separator\nresolved second\n", regions[0]),
        "Heading\n=======\n".to_owned(),
    ] {
        std::fs::write(case.flagged.path.to_path_buf(), partial.as_bytes()).unwrap();
        let before = case
            .folio
            .registry
            .references(case.flagged.id, &case.flagged.head)
            .unwrap();
        assert!(matches!(
            case.home.home.stage(
                &case.flagged,
                &PlacementFs::new(&case.home.home),
                &case.folio.objects,
                &case.folio.refs,
                &mut case.folio.registry,
                &case.lock,
            ),
            Err(RulesError::Refused(PlacementRefusal {
                reason: RefusalReason::ConflictMarkers,
                ..
            }))
        ));
        case.assert_flagged_unchanged(partial.as_bytes());
        assert_eq!(
            case.folio
                .registry
                .references(case.flagged.id, &case.flagged.head)
                .unwrap(),
            before
        );
        assert_eq!(
            case.folio
                .refs
                .read(RefKind::Branch, &RefName::parse("main").unwrap())
                .unwrap(),
            case.flagged.commit_id
        );
    }
    let resolved = b"resolved first\nseparator\nresolved second\n";
    std::fs::write(case.flagged.path.to_path_buf(), resolved).unwrap();
    let fs = PlacementFs::new(&case.home.home);
    case.home
        .home
        .stage(
            &case.flagged,
            &fs,
            &case.folio.objects,
            &case.folio.refs,
            &mut case.folio.registry,
            &case.lock,
        )
        .unwrap();
    let staged = case
        .folio
        .registry
        .placement_at(&case.flagged.path)
        .unwrap()
        .unwrap();
    let id = case
        .home
        .home
        .continue_merge(
            &staged,
            metadata(),
            &fs,
            &HistoryStores {
                objects: &case.folio.objects,
                refs: &case.folio.refs,
                object_limit: 16384,
            },
            &mut case.folio.registry,
            &mut case.lock,
        )
        .unwrap();
    let commit = case.folio.objects.get_commit(id, 16384).unwrap();
    assert_eq!(
        commit.parents,
        vec![
            case.flagged.commit_id.unwrap(),
            case.flagged.conflict.as_ref().unwrap().other
        ]
    );
    assert_eq!(
        case.folio
            .objects
            .get(commit.content, ObjectKind::Blob, u64::MAX)
            .unwrap(),
        resolved
    );
    let row = case
        .folio
        .registry
        .placement_at(&staged.path)
        .unwrap()
        .unwrap();
    assert!(row.staged.is_none());
    assert!(row.conflict.is_none());
}
