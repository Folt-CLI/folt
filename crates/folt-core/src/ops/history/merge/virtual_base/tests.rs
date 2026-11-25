use super::*;
use crate::config::formats::OBJECT_ALGORITHM;
use crate::folio::dir::{Durability, FolioSpec, create_folio};
use crate::folio::format::FolioFormat;
use crate::history::{merge_base, merge_base_from};
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

// Use disjoint first-byte ranges to keep the supplied order deterministic.
fn ordered_commit(
    objects: &ObjectStore,
    lock: &HomeLock,
    bytes: &[u8],
    parent: ObjectId,
    range: std::ops::Range<u8>,
) -> ObjectId {
    let seed = commit(objects, lock, bytes, &[parent]);
    let mut candidate = objects.get_commit(seed, 16384).unwrap();
    for nonce in 0..256 {
        candidate.message = format!("ordered ancestor {nonce}");
        let id = crate::object::hash_object(ObjectKind::Commit, &candidate.encode_body().unwrap());
        if range.contains(&id.as_bytes()[0]) {
            assert_eq!(objects.put_commit(lock, &candidate).unwrap(), id);
            return id;
        }
    }
    panic!("bounded metadata search must find an ordered ancestor");
}

#[test]
fn fold_searches_against_all_accumulated_ancestors() {
    let home = TempHome::new().unwrap();
    let lock = home.home.try_lock().unwrap();
    let objects = store(&home, &lock);
    let r = commit(&objects, &lock, b"a\nb\nc\nd\ne\n", &[]);
    let d = commit(&objects, &lock, b"A\nb\nc\nd\ne\n", &[r]);
    let a = ordered_commit(&objects, &lock, b"A\nb\nc\nd\ne\n", d, 0..64);
    let b = ordered_commit(&objects, &lock, b"a\nb\nc\nd\ne\n", r, 64..128);
    let c = ordered_commit(&objects, &lock, b"a\nb\nc\nd\ne\n", d, 128..255);
    assert!(a.as_bytes() < b.as_bytes() && b.as_bytes() < c.as_bytes());
    assert_eq!(
        merge_base(&objects, a, b, 16384).unwrap(),
        MergeBase::One(r)
    );
    assert_eq!(
        merge_base(&objects, b, c, 16384).unwrap(),
        MergeBase::One(r)
    );
    assert_eq!(
        merge_base_from(&objects, &[a, b], c, 16384).unwrap(),
        MergeBase::One(d)
    );
    let MergeResult::Clean(pair) = merge3(
        &content(&objects, r, 16384).unwrap(),
        &content(&objects, a, 16384).unwrap(),
        &content(&objects, b, 16384).unwrap(),
    ) else {
        panic!("first fold must be clean");
    };
    assert_eq!(
        merge3(
            &content(&objects, r, 16384).unwrap(),
            &pair,
            &content(&objects, c, 16384).unwrap()
        ),
        MergeResult::Clean(b"A\nb\nc\nd\ne\n".to_vec())
    );
    assert_eq!(
        fold(&objects, &[a, b, c], 16384).unwrap(),
        Some(b"a\nb\nc\nd\ne\n".to_vec())
    );
}

#[test]
fn missing_ancestor_preserves_store_io_error() {
    let home = TempHome::new().unwrap();
    let lock = home.home.try_lock().unwrap();
    let objects = store(&home, &lock);
    let a = commit(&objects, &lock, b"content\n", &[]);
    let missing = ObjectId::from_bytes([0; 32]);
    let StoreError::Io { path, source } = fold(&objects, &[a, missing], 16384).unwrap_err() else {
        panic!("expected missing-object I/O error");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(
        path.file_stem().unwrap().to_str().unwrap(),
        missing.to_string()
    );
}

#[test]
fn two_ancestors_keep_clean_text_conflict_and_binary_results() {
    let home = TempHome::new().unwrap();
    let lock = home.home.try_lock().unwrap();
    let objects = store(&home, &lock);
    let original = commit(&objects, &lock, b"a\nb\nc\n", &[]);
    let a = commit(&objects, &lock, b"A\nb\nc\n", &[original]);
    for (other, expected) in [
        (b"a\nb\nC\n".as_slice(), Some(b"A\nb\nC\n".as_slice())),
        (
            b"other\nb\nc\n",
            Some(b"<<<<<<<\nA\n=======\nother\n>>>>>>>\nb\nc\n"),
        ),
        (b"other\0\n", None),
    ] {
        let b = commit(&objects, &lock, other, &[original]);
        assert_eq!(fold(&objects, &[a, b], 16384).unwrap().as_deref(), expected);
    }
}
