use super::*;
use crate::object::hash_blob;
use crate::test_support::facts;

#[test]
fn user_kind_excludes_internal() {
    for (text, expected) in [
        ("copy", Some(PlacementKind::Copy)),
        ("link", Some(PlacementKind::Link)),
        ("internal", None),
        ("Copy", None),
        ("", None),
    ] {
        assert_eq!(PlacementKind::parse_user(text), expected);
    }
}

#[test]
fn copy_drift_table() {
    let facts = facts();
    let p = facts.placement.to_placement().1;
    for (observation, hash, expected) in [
        (
            facts.observation.clone(),
            facts.placement_hash,
            Drift::InSync,
        ),
        (
            facts.observation.clone(),
            Some(hash_blob(b"edit")),
            Drift::PlacementEdited,
        ),
        (facts.observation.clone(), None, Drift::Unusable),
        (Observation::Missing, None, Drift::Gone),
        (Observation::NotAFile, None, Drift::Unusable),
        (Observation::IntactLink, None, Drift::Unusable),
    ] {
        assert_eq!(classify(&p, &observation, hash), expected);
    }
}

#[test]
fn link_drift_table() {
    let facts = facts();
    let mut p = facts.placement.to_placement().1;
    p.kind = PlacementKind::Link;
    for (observation, expected) in [
        (Observation::IntactLink, Drift::InSync),
        (
            Observation::OtherLink {
                target: "other".into(),
            },
            Drift::Detached,
        ),
        (facts.observation, Drift::Detached),
        (Observation::Missing, Drift::Gone),
        (Observation::NotAFile, Drift::Unusable),
    ] {
        assert_eq!(classify(&p, &observation, None), expected);
        assert!(!needs_hash(&p, &observation));
    }
}

#[test]
fn hash_only_when_size_can_match_or_base_is_absent() {
    let facts = facts();
    let mut p = facts.placement.to_placement().1;
    assert!(needs_hash(&p, &facts.observation));
    let mut signature = p.base.as_ref().unwrap().signature.clone();
    signature.size += 1;
    let changed = Observation::File { signature };
    assert!(!needs_hash(&p, &changed));
    assert_eq!(classify(&p, &changed, None), Drift::PlacementEdited);
    p.base = None;
    assert!(needs_hash(&p, &changed));
    assert_eq!(
        classify(&p, &changed, Some(hash_blob(b"x"))),
        Drift::PlacementEdited
    );
    assert_eq!(classify(&p, &changed, None), Drift::Unusable);
    assert!(!needs_hash(&p, &Observation::Missing));
}
