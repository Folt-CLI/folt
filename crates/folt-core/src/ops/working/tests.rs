use super::*;
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
