use super::*;

#[test]
fn status_compares_all_history_states() {
    let old = hash_blob(b"old");
    let tip = hash_blob(b"tip");
    for (recorded, current, expected) in [
        (None, None, Behind::NoHistory),
        (None, Some(tip), Behind::Unrecorded),
        (Some(old), None, Behind::MissingTip),
        (Some(tip), Some(tip), Behind::UpToDate),
        (Some(old), Some(tip), Behind::Behind { tip }),
    ] {
        let mut facts = facts();
        facts.placement.commit_id = recorded;
        facts.tip = current;
        assert_eq!(judge_status(&[facts])[0].behind, Some(expected));
    }
}

#[test]
fn links_have_no_behind_status() {
    let mut facts = facts();
    facts.placement.kind = PlacementKind::Link;
    facts.observation = Observation::IntactLink;
    let status = judge_status(&[facts]);
    assert_eq!(status[0].behind, None);
    assert_eq!(status[0].drift, Drift::InSync);
}

#[test]
fn link_status_uses_shared_internal_working_state() {
    let mut link = facts();
    link.placement.kind = PlacementKind::Link;
    link.placement.base = None;
    link.observation = Observation::IntactLink;
    let mut internal = facts();
    internal.placement.kind = PlacementKind::Internal;
    internal.placement.head = "main".into();
    internal.placement_hash = Some(hash_blob(b"edit"));
    internal.placement.staged = Some(placement::StagedBlob {
        hash: hash_blob(b"edit"),
        size: 4,
    });
    let status = judge_status(&[link, internal]);
    assert_eq!(status[0].drift, Drift::InSync);
    assert_eq!(status[0].behind, None);
    assert_eq!(status[0].edit, crate::working_state::Edit::Edited);
    assert_eq!(status[0].staged, crate::working_state::Staged::Staged);
}

#[test]
fn move_refuses_links_and_internal_branch_changes() {
    for (kind, reason) in [
        (PlacementKind::Link, RefusalReason::Link),
        (PlacementKind::Internal, RefusalReason::InternalBranch),
    ] {
        let mut facts = facts();
        facts.placement.kind = kind;
        assert_eq!(
            judge_move(&facts, &target(), true).unwrap_err().reason,
            reason
        );
    }
}

#[test]
fn move_refuses_unusable_observations_even_with_discard() {
    for (observation, reason) in [
        (Observation::Missing, RefusalReason::Gone),
        (Observation::NotAFile, RefusalReason::NotAFile),
        (Observation::IntactLink, RefusalReason::Unusable),
        (
            Observation::OtherLink {
                target: "elsewhere".into(),
            },
            RefusalReason::Unusable,
        ),
    ] {
        let mut facts = facts();
        facts.observation = observation;
        assert_eq!(
            judge_move(&facts, &target(), true).unwrap_err().reason,
            reason
        );
    }
}

#[test]
fn edited_move_requires_discard_or_matching_incoming_content() {
    let mut facts = facts();
    facts.placement_hash = Some(hash_blob(b"edits"));
    assert_eq!(
        judge_move(&facts, &target(), false).unwrap_err().reason,
        RefusalReason::Edited
    );
    assert!(judge_move(&facts, &target(), true).is_ok());
    let mut target = target();
    target.content = facts.placement_hash.unwrap();
    assert!(judge_move(&facts, &target, false).is_ok());
}

#[test]
fn clean_move_preserves_target_and_row() {
    let facts = facts();
    let target = target();
    let plan = judge_move(&facts, &target, false).unwrap();
    assert_eq!(plan.placement(), &facts.placement);
    assert_eq!(plan.target(), &target);
}

#[test]
fn relocation_and_removal_refusal_table() {
    let original = facts();
    let target = RelocateTarget {
        path: registry::PlacementPath::resolve(std::env::temp_dir().join("folt-unit-destination"))
            .unwrap(),
        observation: Observation::Missing,
        registered: false,
        pending: false,
        rows: Vec::new(),
    };
    for (kind, observation, reason) in [
        (
            PlacementKind::Internal,
            original.observation.clone(),
            RefusalReason::Internal,
        ),
        (
            PlacementKind::Copy,
            Observation::Missing,
            RefusalReason::Gone,
        ),
        (
            PlacementKind::Copy,
            Observation::NotAFile,
            RefusalReason::NotAFile,
        ),
        (
            PlacementKind::Link,
            original.observation.clone(),
            RefusalReason::LinkReplaced,
        ),
        (
            PlacementKind::Link,
            Observation::OtherLink {
                target: "other".into(),
            },
            RefusalReason::LinkReplaced,
        ),
    ] {
        let mut source = original.clone();
        source.placement.kind = kind;
        source.observation = observation;
        assert_eq!(judge_relocate(&source, &target).unwrap_err().reason, reason);
        if kind == PlacementKind::Internal {
            for delete_file in [false, true] {
                assert_eq!(
                    judge_remove(&source, delete_file, true).unwrap_err().reason,
                    reason
                );
            }
        } else if source.observation == Observation::Missing {
            assert!(judge_remove(&source, true, false).is_ok());
        } else {
            assert_eq!(
                judge_remove(&source, true, true).unwrap_err().reason,
                reason
            );
            assert!(judge_remove(&source, false, false).is_ok());
        }
    }
    let mut occupied = target.clone();
    occupied.registered = true;
    assert_eq!(
        judge_relocate(&original, &occupied).unwrap_err().reason,
        RefusalReason::DestinationRegistered
    );
    occupied.registered = false;
    occupied.observation = Observation::NotAFile;
    assert_eq!(
        judge_relocate(&original, &occupied).unwrap_err().reason,
        RefusalReason::DestinationExists
    );
    occupied = target.clone();
    occupied.path = original.placement.path.clone();
    assert_eq!(
        judge_relocate(&original, &occupied).unwrap_err().reason,
        RefusalReason::InsideSource
    );
}
