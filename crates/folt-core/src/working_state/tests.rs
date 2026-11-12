use super::*;
use crate::object::hash_blob;
use crate::test_support::facts;

#[test]
fn working_target_decision_table() {
    let mut row = facts().placement;
    let mut internal = row.clone();
    internal.kind = PlacementKind::Internal;
    internal.head = "main".into();
    for kind in [PlacementKind::Copy, PlacementKind::Internal] {
        row.kind = kind;
        assert_eq!(
            resolve_observed(&row, &Observation::Missing, None).unwrap(),
            row
        );
    }
    row.kind = PlacementKind::Link;
    assert_eq!(
        resolve_observed(&row, &Observation::IntactLink, Some(internal.clone())).unwrap(),
        internal
    );
    for (observation, internal, reason) in [
        (
            facts().observation,
            Some(internal.clone()),
            RefusalReason::LinkReplaced,
        ),
        (
            Observation::OtherLink {
                target: "other".into(),
            },
            Some(internal.clone()),
            RefusalReason::LinkReplaced,
        ),
        (
            Observation::Missing,
            Some(internal.clone()),
            RefusalReason::Gone,
        ),
        (
            Observation::NotAFile,
            Some(internal.clone()),
            RefusalReason::NotAFile,
        ),
        (
            Observation::IntactLink,
            None,
            RefusalReason::MissingInternal,
        ),
    ] {
        assert!(
            matches!(resolve_observed(&row, &observation, internal), Err(RulesError::Refused(ref refusal)) if refusal.reason == reason)
        );
    }
    internal.id = crate::id::FolioId::from_bytes([2; 16]);
    assert!(
        matches!(resolve_observed(&row, &Observation::IntactLink, Some(internal)), Err(RulesError::Refused(ref refusal)) if refusal.reason == RefusalReason::MissingInternal)
    );
}

#[test]
fn edit_requires_matching_size_and_hash() {
    let mut facts = facts();
    assert_eq!(judge_working(&facts).edit, Edit::Clean);
    facts.placement_hash = Some(hash_blob(b"edit"));
    assert_eq!(judge_working(&facts).edit, Edit::Edited);
    facts.placement_hash = facts.placement.base.as_ref().map(|base| base.hash);
    if let Observation::File { signature } = &mut facts.observation {
        signature.size += 1;
    }
    assert_eq!(judge_working(&facts).edit, Edit::Edited);
    facts.placement.base = None;
    assert_eq!(judge_working(&facts).edit, Edit::Edited);
}

#[test]
fn stage_distinguishes_later_edits() {
    let mut facts = facts();
    assert_eq!(judge_working(&facts).staged, Staged::None);
    facts.placement.staged = Some(StagedBlob {
        hash: facts.placement_hash.unwrap(),
        size: 4,
    });
    assert_eq!(judge_working(&facts).staged, Staged::Staged);
    facts.placement_hash = Some(hash_blob(b"edit"));
    assert_eq!(judge_working(&facts).staged, Staged::StagedThenModified);
    facts.placement_hash = Some(facts.placement.staged.unwrap().hash);
    facts.observation = Observation::Missing;
    assert_eq!(judge_working(&facts).staged, Staged::StagedThenModified);
}
