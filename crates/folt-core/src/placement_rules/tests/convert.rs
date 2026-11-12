use super::*;

#[test]
// split in the folt-core refactor (#203)
#[allow(clippy::too_many_lines)]
fn convert_decision_table() {
    let mut copy = facts();
    copy.placement.head = "main".into();
    let mut target = target();
    target.branch = RefName::parse("main").unwrap();
    target.content = copy.placement_hash.unwrap();
    let mut internal = copy.clone();
    internal.placement.kind = PlacementKind::Internal;
    let mut link = copy.clone();
    link.placement.kind = PlacementKind::Link;
    link.placement.base = None;
    link.observation = Observation::IntactLink;
    link.placement_hash = None;

    let mut cases = Vec::new();
    cases.push((copy.clone(), PlacementKind::Link, None));
    let mut behind = copy.clone();
    behind.tip = Some(hash_blob(b"new tip"));
    cases.push((behind, PlacementKind::Link, None));
    cases.push((link.clone(), PlacementKind::Copy, None));
    let mut resumed_copy = link.clone();
    resumed_copy.observation = copy.observation.clone();
    resumed_copy.placement_hash = Some(target.content);
    cases.push((resumed_copy, PlacementKind::Copy, None));
    let mut resumed_link = copy.clone();
    resumed_link.observation = Observation::IntactLink;
    cases.push((resumed_link, PlacementKind::Link, None));
    cases.push((
        internal.clone(),
        PlacementKind::Copy,
        Some(RefusalReason::Internal),
    ));
    cases.push((
        copy.clone(),
        PlacementKind::Internal,
        Some(RefusalReason::Internal),
    ));
    cases.push((
        copy.clone(),
        PlacementKind::Copy,
        Some(RefusalReason::SameKind),
    ));
    let mut topic = copy.clone();
    topic.placement.head = "topic".into();
    cases.push((topic, PlacementKind::Link, Some(RefusalReason::CopyBranch)));
    let mut staged = copy.clone();
    staged.placement.staged = Some(placement::StagedBlob {
        hash: target.content,
        size: 4,
    });
    cases.push((staged, PlacementKind::Link, Some(RefusalReason::Staged)));
    let mut edited = copy.clone();
    edited.placement_hash = Some(hash_blob(b"edit"));
    cases.push((edited, PlacementKind::Link, Some(RefusalReason::Edited)));
    let mut unhashable = copy.clone();
    unhashable.placement_hash = None;
    cases.push((
        unhashable,
        PlacementKind::Link,
        Some(RefusalReason::Unusable),
    ));
    let mut detached = link.clone();
    detached.observation = copy.observation.clone();
    detached.placement_hash = Some(hash_blob(b"edit"));
    cases.push((detached, PlacementKind::Copy, Some(RefusalReason::Edited)));
    for (observation, reason) in [
        (Observation::Missing, RefusalReason::ConvertGone),
        (Observation::NotAFile, RefusalReason::NotAFile),
        (
            Observation::OtherLink {
                target: "other".into(),
            },
            RefusalReason::OtherLink,
        ),
    ] {
        for (mut facts, to_kind) in [
            (copy.clone(), PlacementKind::Link),
            (link.clone(), PlacementKind::Copy),
        ] {
            facts.observation = observation.clone();
            cases.push((facts, to_kind, Some(reason)));
        }
    }
    for (facts, to_kind, reason) in cases {
        assert_eq!(
            judge_convert(&facts, to_kind, Some(&target), Some(&internal))
                .err()
                .map(|r| r.reason),
            reason
        );
    }
    for altered in 0..6 {
        let mut dirty = internal.clone();
        match altered {
            0 => dirty.placement_hash = Some(hash_blob(b"edit")),
            1 => dirty.observation = Observation::Missing,
            2 => {
                dirty.placement.staged = Some(placement::StagedBlob {
                    hash: target.content,
                    size: 4,
                })
            }
            3 => dirty.placement.id = FolioId::from_bytes([2; 16]),
            4 => dirty.placement.base = None,
            _ => dirty.placement.kind = PlacementKind::Copy,
        }
        assert_eq!(
            judge_convert(&link, PlacementKind::Copy, Some(&target), Some(&dirty))
                .unwrap_err()
                .reason,
            RefusalReason::InternalEdited
        );
    }
    assert_eq!(
        judge_convert(&link, PlacementKind::Copy, Some(&target), None)
            .unwrap_err()
            .reason,
        RefusalReason::InternalEdited
    );
    target.branch = RefName::parse("topic").unwrap();
    assert_eq!(
        judge_convert(&copy, PlacementKind::Link, Some(&target), None)
            .unwrap_err()
            .reason,
        RefusalReason::CopyBranch
    );
}

#[test]
fn conversion_without_history_keeps_all_copy_refusals() {
    let mut copy = facts();
    copy.placement.head = "main".into();
    copy.placement.commit_id = None;
    copy.tip = None;
    assert!(judge_convert(&copy, PlacementKind::Link, None, None).is_ok());
    for (change, reason) in [
        (0, RefusalReason::CopyBranch),
        (1, RefusalReason::Edited),
        (2, RefusalReason::Staged),
        (3, RefusalReason::Internal),
        (4, RefusalReason::ConvertGone),
    ] {
        let mut altered = copy.clone();
        match change {
            0 => altered.placement.head = "topic".into(),
            1 => altered.placement_hash = Some(hash_blob(b"edited")),
            2 => {
                altered.placement.staged = Some(placement::StagedBlob {
                    hash: hash_blob(b"stage"),
                    size: 5,
                })
            }
            3 => altered.placement.kind = PlacementKind::Internal,
            _ => altered.observation = Observation::Missing,
        }
        assert_eq!(
            judge_convert(&altered, PlacementKind::Link, None, None)
                .unwrap_err()
                .reason,
            reason
        );
    }
    let mut link = copy;
    link.placement.kind = PlacementKind::Link;
    link.observation = Observation::IntactLink;
    assert!(judge_convert(&link, PlacementKind::Copy, None, None).is_err());
}
