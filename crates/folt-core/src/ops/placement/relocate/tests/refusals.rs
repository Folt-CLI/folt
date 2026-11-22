use super::*;
use crate::ops::placement::remove::judge_remove;

#[test]
fn relocation_and_removal_refusal_table() {
    let original = facts();
    let target = RelocateTarget {
        path: crate::placement_path::PlacementPath::resolve(
            std::env::temp_dir().join("folt-unit-destination"),
        )
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

#[test]
fn conflict_refusal_precedes_destination_checks() {
    let (fixture, _lock, registry) = crate::test_support::RelocateFixture::new(false);
    let mut facts = fixture.facts();
    facts.placement.conflict = crate::test_support::conflicted_facts().placement.conflict;
    let target = fixture.plan(&registry).target;
    let mut occupied = target;
    occupied.registered = true;
    let refusal = judge_relocate(&facts, &occupied).unwrap_err();
    assert_eq!(refusal.reason, RefusalReason::Conflicted);
    assert!(
        refusal
            .to_string()
            .contains(&facts.placement.path.to_path_buf().display().to_string())
    );
}
