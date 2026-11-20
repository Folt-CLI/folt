use super::*;
use crate::object::hash_blob;
use crate::test_support::facts;

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
    assert_eq!(status[0].edit, crate::ops::working::Edit::Edited);
    assert_eq!(status[0].staged, crate::ops::working::Staged::Staged);
}
