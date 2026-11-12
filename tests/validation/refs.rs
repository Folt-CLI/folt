use folt_core::object::ObjectKind;
use folt_core::ref_name::{RefKind, RefName};
use folt_core::refs::{Expect, RefError};

use crate::common::TempHome;
use crate::support::TestFolio;

#[test]
fn updates_and_deletes_compare_expected_value() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let first = folio.commit_object(&lock, b"first");
    let second = folio.commit_object(&lock, b"second");
    let name = RefName::parse("topic").unwrap();
    folio
        .refs
        .update(&mut lock, RefKind::Branch, &name, first, Expect::Absent)
        .unwrap();
    assert!(
        matches!(folio.refs.update(&mut lock, RefKind::Branch, &name, second, Expect::Absent), Err(RefError::Stale { actual: Some(id) }) if id == first)
    );
    folio
        .refs
        .update(&mut lock, RefKind::Branch, &name, second, Expect::Is(first))
        .unwrap();
    assert_eq!(
        folio.refs.read(RefKind::Branch, &name).unwrap(),
        Some(second)
    );
    assert!(matches!(
        folio
            .refs
            .delete(&mut lock, RefKind::Branch, &name, Expect::Is(first)),
        Err(RefError::Stale { .. })
    ));
    assert_eq!(
        folio.refs.read(RefKind::Branch, &name).unwrap(),
        Some(second)
    );
    folio
        .refs
        .delete(&mut lock, RefKind::Branch, &name, Expect::Is(second))
        .unwrap();
    assert_eq!(folio.refs.read(RefKind::Branch, &name).unwrap(), None);
}

#[test]
fn blob_cannot_be_branch_target() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let blob = folio.objects.put(&lock, ObjectKind::Blob, b"blob").unwrap();
    let name = RefName::parse("topic").unwrap();
    assert!(matches!(
        folio
            .refs
            .update(&mut lock, RefKind::Branch, &name, blob, Expect::Absent),
        Err(RefError::WrongKind {
            actual: ObjectKind::Blob,
            ..
        })
    ));
    assert_eq!(folio.refs.read(RefKind::Branch, &name).unwrap(), None);
}

#[test]
fn lists_nested_refs_in_name_order() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let commit = folio.commit_object(&lock, b"content");
    let names: Vec<_> = ["a", "topic/one", "z"]
        .map(|s| RefName::parse(s).unwrap())
        .into();
    for name in names.iter().rev() {
        folio
            .refs
            .update(&mut lock, RefKind::Branch, name, commit, Expect::Absent)
            .unwrap();
    }
    assert_eq!(
        folio.refs.list(RefKind::Branch).unwrap(),
        names.into_iter().map(|n| (n, commit)).collect::<Vec<_>>()
    );
    assert!(folio.refs.list(RefKind::Tag).unwrap().is_empty());
}
