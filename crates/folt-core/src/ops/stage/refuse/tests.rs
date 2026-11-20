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
