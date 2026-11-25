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
fn conflict_checks_refuse_any_recorded_whole_line() {
    let row = crate::test_support::conflicted_facts().placement;
    assert!(is_conflicted(&row));
    assert!(!is_conflicted(&facts().placement));
    let state = row.conflict.unwrap();
    for bytes in [
        b"<<<<<<<\n".as_slice(),
        b"=======\n",
        b">>>>>>>\n",
        b">>>>>>>\n=======\n<<<<<<<\n",
        b"Heading\n=======\nbody\n",
        b"<<<<<<< ours\n=======\n>>>>>>>\n",
        b"\xff\n<<<<<<<\n",
        b"<<<<<<<\n=======\n>>>>>>>",
    ] {
        assert!(markers_remain(&state, bytes), "{bytes:?}");
    }
    let mut repeated = state.clone();
    repeated.marker_lines.extend(state.marker_lines.clone());
    assert!(markers_remain(&repeated, b"=======\n"));
    repeated.marker_lines.clear();
    assert!(!markers_remain(&repeated, b"<<<<<<<\n=======\n>>>>>>>\n"));
}

#[test]
fn conflict_checks_preserve_byte_exact_boundaries() {
    let state = crate::test_support::conflicted_facts()
        .placement
        .conflict
        .unwrap();
    for bytes in [
        b"resolved\n".as_slice(),
        b"",
        b"prefix<<<<<<<\n",
        b"<<<<<<< suffix\n",
        b" <<<<<<<\n",
        b"<<<<<<<\r\n",
        b"<<<<<<<",
        b"Heading\n========\n",
    ] {
        assert!(!markers_remain(&state, bytes), "{bytes:?}");
        let mut with_exact_line = bytes.to_vec();
        with_exact_line.extend_from_slice(b"\n=======\n");
        assert!(markers_remain(&state, &with_exact_line));
    }
}
