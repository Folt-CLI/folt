use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use folt_core::history::{Expect, MergeBase, is_ancestor, merge_base};
use folt_core::home::HomeLock;
use folt_core::id::ObjectId;
use folt_core::object::{Author, Commit, CommitTime, ObjectError, hash_blob};
use folt_core::ref_name::{RefKind, RefName};
use folt_core::store::StoreError;

use crate::common::TempHome;
use crate::support::TestFolio;

fn commit(folio: &TestFolio, lock: &HomeLock, message: &str, parents: &[ObjectId]) -> ObjectId {
    folio
        .objects
        .put_commit(
            lock,
            &Commit {
                content: hash_blob(b"initial"),
                parents: parents.to_vec(),
                author: Author::anonymous(),
                time: CommitTime::from_unix_seconds(0).unwrap(),
                message: message.into(),
                body: None,
                group: None,
            },
        )
        .unwrap()
}

#[test]
fn linear_ancestry_includes_self_but_not_descendants() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let c1 = commit(&folio, &lock, "c1", &[]);
    let c2 = commit(&folio, &lock, "c2", &[c1]);
    let c3 = commit(&folio, &lock, "c3", &[c2]);
    let limit = folio.stores().object_limit;
    assert!(is_ancestor(&folio.objects, c1, c3, limit).unwrap());
    assert!(is_ancestor(&folio.objects, c1, c2, limit).unwrap());
    assert!(!is_ancestor(&folio.objects, c3, c1, limit).unwrap());
    assert!(is_ancestor(&folio.objects, c2, c2, limit).unwrap());
}

#[test]
fn diamond_follows_every_parent() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let left = commit(&folio, &lock, "left", &[root]);
    let right = commit(&folio, &lock, "right", &[root]);
    let merged = commit(&folio, &lock, "merged", &[left, right]);
    let limit = folio.stores().object_limit;
    for ancestor in [root, left, right] {
        assert!(is_ancestor(&folio.objects, ancestor, merged, limit).unwrap());
    }
    assert!(!is_ancestor(&folio.objects, left, right, limit).unwrap());
}

#[test]
fn merge_base_handles_divergence_descendants_and_self() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let left = commit(&folio, &lock, "left", &[root]);
    let right = commit(&folio, &lock, "right", &[root]);
    let limit = folio.stores().object_limit;
    for (a, b, expected) in [
        (left, right, root),
        (root, left, root),
        (left, root, root),
        (left, left, left),
    ] {
        assert_eq!(
            merge_base(&folio.objects, a, b, limit).unwrap(),
            MergeBase::One(expected)
        );
    }
}

#[test]
fn disjoint_roots_have_no_merge_base() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let a = commit(&folio, &lock, "a", &[]);
    let b = commit(&folio, &lock, "b", &[]);
    assert_eq!(
        merge_base(&folio.objects, a, b, folio.stores().object_limit).unwrap(),
        MergeBase::None
    );
}

#[test]
fn criss_cross_reports_sorted_best_ancestors_in_either_argument_order() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let a = commit(&folio, &lock, "a", &[root]);
    let b = commit(&folio, &lock, "b", &[root]);
    let m1 = commit(&folio, &lock, "m1", &[a, b]);
    let m2 = commit(&folio, &lock, "m2", &[b, a]);
    let mut expected = vec![a, b];
    expected.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    for (a, b) in [(m1, m2), (m2, m1)] {
        assert_eq!(
            merge_base(&folio.objects, a, b, folio.stores().object_limit).unwrap(),
            MergeBase::Several(expected.clone())
        );
    }
}

#[test]
fn common_ancestor_chain_keeps_only_the_nearest() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let older = commit(&folio, &lock, "older", &[root]);
    let nearest = commit(&folio, &lock, "nearest", &[older]);
    // Direct older parents ensure pruning does not depend on traversal order.
    let left = commit(&folio, &lock, "left", &[nearest, root]);
    let right = commit(&folio, &lock, "right", &[older, nearest]);
    assert_eq!(
        merge_base(&folio.objects, left, right, folio.stores().object_limit).unwrap(),
        MergeBase::One(nearest)
    );
}

#[test]
fn three_thousand_commits_are_walked_iteratively() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let mut chain = vec![commit(&folio, &lock, "root", &[])];
    for index in 1..3000 {
        chain.push(commit(
            &folio,
            &lock,
            &index.to_string(),
            &[chain[index - 1]],
        ));
    }
    let limit = folio.stores().object_limit;
    assert!(is_ancestor(&folio.objects, chain[0], chain[2999], limit).unwrap());
    assert_eq!(
        merge_base(&folio.objects, chain[2999], chain[1500], limit).unwrap(),
        MergeBase::One(chain[1500])
    );
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                files.insert(entry.path(), std::fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

#[test]
fn missing_reachable_commits_return_store_errors_without_writes() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let missing = ObjectId::from_bytes([42; 32]);
    let broken = commit(&folio, &lock, "broken", &[root, missing]);
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &RefName::parse("topic").unwrap(),
            root,
            Expect::Absent,
        )
        .unwrap();
    let limit = folio.stores().object_limit;
    // Close SQLite (including WAL locks) and release the home lock before raw reads on Windows.
    drop(folio.registry);
    drop(lock);
    let before = snapshot(home.path());
    assert!(is_ancestor(&folio.objects, root, root, limit).unwrap());
    assert_eq!(
        merge_base(&folio.objects, root, root, limit).unwrap(),
        MergeBase::One(root)
    );
    for id in [missing, broken] {
        assert!(matches!(
            is_ancestor(&folio.objects, root, id, limit),
            Err(StoreError::Io { .. } | StoreError::NotRegularFile(_))
        ));
        for (a, b) in [(root, id), (id, root)] {
            assert!(matches!(
                merge_base(&folio.objects, a, b, limit),
                Err(StoreError::Io { .. } | StoreError::NotRegularFile(_))
            ));
        }
    }
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn corrupt_commits_and_object_read_limits_are_propagated() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let tip = commit(&folio, &lock, "tip", &[root]);
    let root_size = folio
        .objects
        .get_commit(root, folio.stores().object_limit)
        .unwrap()
        .encode_body()
        .unwrap()
        .len() as u64;
    for result in [
        is_ancestor(&folio.objects, root, tip, root_size - 1).map(|_| ()),
        merge_base(&folio.objects, root, tip, root_size - 1).map(|_| ()),
    ] {
        assert!(matches!(
            result,
            Err(StoreError::Object(ObjectError::LimitExceeded))
        ));
    }
    assert!(is_ancestor(&folio.objects, root, root, root_size).unwrap());
    let limit = folio.stores().object_limit;
    // The remaining work needs no registry or home lock; both lock files against raw reads on Windows.
    drop(folio.registry);
    drop(lock);
    let path = snapshot(home.path())
        .into_keys()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == root.to_string().as_str())
        })
        .unwrap();
    std::fs::write(path, b"corrupt").unwrap();
    let before = snapshot(home.path());
    assert!(is_ancestor(&folio.objects, root, tip, limit).is_err());
    assert!(merge_base(&folio.objects, root, tip, limit).is_err());
    assert_eq!(snapshot(home.path()), before);
}
