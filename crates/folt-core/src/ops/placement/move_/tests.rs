use super::*;
use crate::object::hash_blob;
use crate::test_support::{facts, target};

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
