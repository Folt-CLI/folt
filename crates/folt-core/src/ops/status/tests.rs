use super::*;
use crate::object::hash_blob;
use crate::placement::StagedBlob;
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
fn link_status_uses_shared_internal_working_status() {
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
    assert_eq!(status[0].edit, crate::ops::status::Edit::Edited);
    assert_eq!(status[0].staged, crate::ops::status::Staged::Staged);
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
