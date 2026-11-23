use super::*;
use crate::object::hash_blob;
use crate::placement::StagedBlob;
use crate::test_support::facts;

#[test]
fn row_without_stage_is_not_staged() {
    let row = facts().placement;
    assert!(!is_staged(&row));
}

#[test]
fn row_with_stage_is_staged() {
    let mut row = facts().placement;
    row.staged = Some(StagedBlob {
        hash: hash_blob(b"stage"),
        size: 5,
    });
    assert!(is_staged(&row));
}

#[test]
fn conflict_checks_use_recorded_whole_lines_in_order() {
    let row = crate::test_support::conflicted_facts().placement;
    assert!(is_conflicted(&row));
    assert!(!is_conflicted(&facts().placement));
    let state = row.conflict.unwrap();
    for (bytes, expected) in [
        (
            b"before\n<<<<<<<\nours\n=======\ntheirs\n>>>>>>>\nafter\n".as_slice(),
            true,
        ),
        (b"<<<<<<<\nours\ntheirs\n>>>>>>>\n", false),
        (b"<<<<<<< ours\n=======\n>>>>>>>\n", false),
        (b"<<<<<<<\n========\n>>>>>>>\n", false),
        (b"Heading\n=======\nbody\n", false),
        (b">>>>>>>\n=======\n<<<<<<<\n", false),
        (b"<<<<<<<\n=======\n>>>>>>>", false),
        (b"\xff\n<<<<<<<\n=======\n>>>>>>>\n", true),
    ] {
        assert_eq!(markers_remain(&state, bytes), expected, "{bytes:?}");
    }
    let mut repeated = state.clone();
    repeated.marker_lines.extend(state.marker_lines.clone());
    assert!(!markers_remain(&repeated, b"<<<<<<<\n=======\n>>>>>>>\n"));
    assert!(markers_remain(
        &repeated,
        b"<<<<<<<\n=======\n>>>>>>>\n<<<<<<<\n=======\n>>>>>>>\n"
    ));
}
