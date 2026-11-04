use super::*;
use crate::object::hash_blob;
use crate::placement::StagedBlob;
use crate::test_support::facts;

fn missing() -> PruneFacts {
    PruneFacts {
        row: facts().placement,
        observation: Ok(Observation::Missing),
        parent_exists: Ok(true),
        hash: None,
    }
}

#[test]
fn every_observation_by_kind() {
    for kind in [
        PlacementKind::Copy,
        PlacementKind::Link,
        PlacementKind::Internal,
    ] {
        for observation in [
            Observation::Missing,
            Observation::IntactLink,
            Observation::OtherLink {
                target: "other".into(),
            },
            facts().observation,
            Observation::Directory,
            Observation::NotAFile,
        ] {
            let mut input = missing();
            input.row.kind = kind;
            input.hash = input.row.base.as_ref().map(|base| base.hash);
            input.observation = Ok(observation.clone());
            let expected = match (kind, &observation) {
                (PlacementKind::Internal, Observation::Missing) => {
                    Some("internal placement missing; damaged home (#51)")
                }
                (PlacementKind::Internal, _) => Some("internal placement"),
                (_, Observation::Missing) => None,
                (PlacementKind::Link, Observation::IntactLink) => Some("in sync"),
                (PlacementKind::Link, Observation::File { .. } | Observation::OtherLink { .. }) => {
                    Some("a different file than expected")
                }
                (PlacementKind::Copy, Observation::File { .. }) => Some("in sync"),
                _ => Some("unusable entry"),
            };
            assert_eq!(
                judge(&input).as_deref(),
                expected,
                "{kind:?} {observation:?}"
            );
        }
    }
}

#[test]
fn parent_and_read_errors_keep_rows_and_copy_edits_are_notes() {
    let mut input = missing();
    input.parent_exists = Ok(false);
    assert_eq!(judge(&input).as_deref(), Some("parent directory missing"));
    input.observation = Err("permission denied".into());
    assert_eq!(
        judge(&input).as_deref(),
        Some("read error: permission denied")
    );
    input.parent_exists = Err("unreachable share".into());
    input.observation = Ok(Observation::Missing);
    assert!(judge(&input).unwrap().contains("unreachable share"));
    input.parent_exists = Ok(true);
    input.observation = Ok(facts().observation);
    input.hash = Some(hash_blob(b"other"));
    assert_eq!(judge(&input).as_deref(), Some("edited or different"));
    input.row.staged = Some(StagedBlob {
        hash: input.hash.unwrap(),
        size: 4,
    });
    assert_eq!(judge(&input).as_deref(), Some("matches stage"));
}

#[test]
fn stage_reachability_accounts_for_twins_and_other_folios() {
    let mut input = missing();
    input.row.staged = Some(StagedBlob {
        hash: hash_blob(b"stage"),
        size: 5,
    });
    let mut twin = input.row.clone();
    // Distinct path keys can be resolved without touching the filesystem.
    twin.path = PlacementPath::resolve(std::env::temp_dir().join("folt-prune-twin")).unwrap();
    for (keep_twin, other_folio, expected) in [(false, false, 1), (true, false, 0), (true, true, 1)]
    {
        if other_folio {
            twin.id = crate::folio_dir::FolioId::from_bytes([9; 16]);
        }
        let mut all = vec![input.row.clone()];
        if keep_twin {
            all.push(twin.clone());
        }
        let result = report(&[input.clone()], &all, true);
        let expected_blobs = if expected == 1 {
            vec![(input.row.id, input.row.staged.unwrap().hash)]
        } else {
            Vec::new()
        };
        assert_eq!(result.unreachable_staged_blobs, expected_blobs);
        assert_eq!(result.dropped[0].stage, Some(hash_blob(b"stage")));
        assert_eq!(result.dropped[0].stage_unreachable, expected == 1);
    }
    let mut gone_twin = input.clone();
    gone_twin.row.path = twin.path;
    let stage = (input.row.id, input.row.staged.unwrap().hash);
    let both = [input, gone_twin];
    let all: Vec<_> = both.iter().map(|facts| facts.row.clone()).collect();
    let result = report(&both, &all, true);
    assert_eq!(result.unreachable_staged_blobs, [stage]);
    assert_eq!(result.dropped.len(), 2);
    for dropped in result.dropped {
        assert_eq!(dropped.stage, Some(stage.1));
        assert!(dropped.stage_unreachable);
    }
}

#[test]
fn dropped_copy_reports_reachable_stage_distinct_from_unstaged_copy() {
    let mut staged = missing();
    let stage = hash_blob(b"stage");
    staged.row.staged = Some(StagedBlob {
        hash: stage,
        size: 5,
    });
    let mut live_twin = staged.clone();
    live_twin.row.path =
        PlacementPath::resolve(std::env::temp_dir().join("folt-prune-live-twin")).unwrap();
    live_twin.observation = Ok(facts().observation);
    live_twin.hash = live_twin.row.base.as_ref().map(|base| base.hash);
    let mut unstaged = missing();
    unstaged.row.staged = None;
    unstaged.row.path =
        PlacementPath::resolve(std::env::temp_dir().join("folt-prune-unstaged")).unwrap();
    let inputs = [staged.clone(), live_twin.clone(), unstaged.clone()];
    let all: Vec<_> = inputs.iter().map(|input| input.row.clone()).collect();

    let result = report(&inputs, &all, true);

    assert_eq!(
        result.dropped,
        [
            DroppedPlacement {
                path: staged.row.path,
                kind: PlacementKind::Copy,
                stage: Some(stage),
                stage_unreachable: false,
            },
            DroppedPlacement {
                path: unstaged.row.path,
                kind: PlacementKind::Copy,
                stage: None,
                stage_unreachable: false,
            },
        ]
    );
    assert_eq!(result.kept.len(), 1);
    assert_eq!(result.kept[0].path, live_twin.row.path);
    assert!(result.unreachable_staged_blobs.is_empty());
}

#[test]
fn dropped_staged_copy_without_twin_reports_unreachable_folio_blob_pair() {
    let mut input = missing();
    let stage = hash_blob(b"stage");
    input.row.staged = Some(StagedBlob {
        hash: stage,
        size: 5,
    });

    let result = report(
        std::slice::from_ref(&input),
        std::slice::from_ref(&input.row),
        true,
    );

    assert_eq!(
        result.dropped,
        [DroppedPlacement {
            path: input.row.path,
            kind: PlacementKind::Copy,
            stage: Some(stage),
            stage_unreachable: true,
        }]
    );
    assert_eq!(result.unreachable_staged_blobs, [(input.row.id, stage)]);
}

#[test]
fn two_dropped_copies_report_shared_unreachable_stage_pair_once() {
    let mut first = missing();
    let stage = hash_blob(b"stage");
    first.row.staged = Some(StagedBlob {
        hash: stage,
        size: 5,
    });
    let mut second = first.clone();
    second.row.path =
        PlacementPath::resolve(std::env::temp_dir().join("folt-prune-dropped-twin")).unwrap();
    let inputs = [first, second];
    let all: Vec<_> = inputs.iter().map(|input| input.row.clone()).collect();

    let result = report(&inputs, &all, true);

    assert_eq!(result.unreachable_staged_blobs, [(all[0].id, stage)]);
    assert_eq!(result.dropped.len(), 2);
    for (dropped, row) in result.dropped.iter().zip(&all) {
        assert_eq!(dropped.path, row.path);
        assert_eq!(dropped.kind, PlacementKind::Copy);
        assert_eq!(dropped.stage, Some(stage));
        assert!(dropped.stage_unreachable);
    }
}

#[test]
fn mass_deletion_boundaries() {
    for (dropped, total, expected) in [
        (0, 0, false),
        (4, 4, false),
        (5, 10, false),
        (5, 9, true),
        (6, 11, true),
        (6, 12, false),
        (usize::MAX, usize::MAX, true),
    ] {
        assert_eq!(needs_confirmation(dropped, total), expected);
    }
}

#[test]
fn scope_matches_whole_path_segments() {
    let mut row = missing().row;
    let root = std::env::temp_dir();
    let scope = [PlacementPath::resolve_directory(root.join("old")).unwrap()];
    for (path, expected) in [("old", true), ("old-notes", false)] {
        row.path = PlacementPath::resolve_directory(root.join(path)).unwrap();
        assert_eq!(in_scope(&row, &scope), expected);
    }
}
