use super::*;
use crate::config::formats::OBJECT_ALGORITHM;
use crate::folio::dir::{Durability, FolioSpec, create_folio};
use crate::folio::format::FolioFormat;
use crate::home::HomeLock;
use crate::layout::FOLIO_LAYOUT_VERSION;
use crate::object::{Author, Commit, CommitTime};
use crate::test_support::TempHome;

fn store(home: &TempHome, lock: &HomeLock) -> ObjectStore {
    let folio = create_folio(
        home.home.path(),
        lock,
        FolioSpec { content: vec![] },
        Durability::None,
    )
    .unwrap();
    ObjectStore::open(
        &folio,
        &FolioFormat {
            layout: FOLIO_LAYOUT_VERSION,
            algorithm: OBJECT_ALGORITHM.into(),
        },
    )
    .unwrap()
}

fn commit(objects: &ObjectStore, lock: &HomeLock, bytes: &[u8], parents: &[ObjectId]) -> ObjectId {
    let content = objects.put(lock, ObjectKind::Blob, bytes).unwrap();
    objects
        .put_commit(
            lock,
            &Commit {
                content,
                parents: parents.to_vec(),
                author: Author::anonymous(),
                time: CommitTime::from_unix_seconds(0).unwrap(),
                message: "fixture".into(),
                body: None,
                group: None,
            },
        )
        .unwrap()
}

#[test]
fn folds_three_ancestors_in_supplied_order_and_keeps_text_markers() {
    let home = TempHome::new().unwrap();
    let lock = home.home.try_lock().unwrap();
    let objects = store(&home, &lock);
    let original = commit(&objects, &lock, b"original\n", &[]);
    let mut ids = [
        commit(&objects, &lock, b"first\n", &[original]),
        commit(&objects, &lock, b"second\n", &[original]),
        commit(&objects, &lock, b"third\n", &[original]),
    ];
    ids.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let bytes: Vec<_> = ids
        .iter()
        .map(|id| content(&objects, *id, 16384).unwrap())
        .collect();
    let MergeResult::Conflicted {
        merged: Some(pair), ..
    } = merge3(b"original\n", &bytes[0], &bytes[1])
    else {
        panic!("expected ancestor text conflict");
    };
    let MergeResult::Conflicted {
        merged: Some(expected),
        ..
    } = merge3(b"original\n", &pair, &bytes[2])
    else {
        panic!("expected accumulated text conflict");
    };
    assert_eq!(fold(&objects, &ids, 16384).unwrap(), Some(expected));
}

#[test]
fn recursively_folds_several_bases_and_propagates_binary_conflicts() {
    for binary in [false, true] {
        let home = TempHome::new().unwrap();
        let lock = home.home.try_lock().unwrap();
        let objects = store(&home, &lock);
        let original = commit(&objects, &lock, b"a\nb\nc\nd\ne\n", &[]);
        let left = if binary {
            b"A\0b\nc\nd\ne\n"
        } else {
            b"A\nb\nc\nd\ne\n"
        };
        let a = commit(&objects, &lock, left, &[original]);
        let b = commit(&objects, &lock, b"a\nb\nC\nd\ne\n", &[original]);
        let c = commit(&objects, &lock, b"A\nB\nC\nd\ne\n", &[a, b]);
        let d = commit(&objects, &lock, b"A\nb\nC\nd\nE\n", &[b, a]);
        let mut ids = [c, d];
        ids.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        assert!(matches!(
            merge_base(&objects, c, d, 16384).unwrap(),
            MergeBase::Several(_)
        ));
        assert_eq!(
            fold(&objects, &ids, 16384).unwrap(),
            if binary {
                None
            } else {
                Some(b"A\nB\nC\nd\nE\n".to_vec())
            }
        );
    }
}

#[test]
fn unrelated_ancestors_use_empty_bytes_and_object_errors_propagate() {
    let home = TempHome::new().unwrap();
    let lock = home.home.try_lock().unwrap();
    let objects = store(&home, &lock);
    let a = commit(&objects, &lock, b"", &[]);
    let b = commit(&objects, &lock, b"new\n", &[]);
    assert_eq!(
        fold(&objects, &[a, b], 16384).unwrap(),
        Some(b"new\n".to_vec())
    );
    assert!(fold(&objects, &[a, b], 1).is_err());
    assert!(fold(&objects, &[a, ObjectId::from_bytes([0; 32])], 16384).is_err());
}
