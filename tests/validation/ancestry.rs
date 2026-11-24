use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use folt_core::history::{Expect, MergeBase, is_ancestor, merge_base, merge_base_from};
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
        assert_eq!(
            merge_base_from(&folio.objects, &[a], b, limit).unwrap(),
            merge_base(&folio.objects, a, b, limit).unwrap()
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
    assert_eq!(
        merge_base_from(&folio.objects, &[a], b, folio.stores().object_limit).unwrap(),
        merge_base(&folio.objects, a, b, folio.stores().object_limit).unwrap()
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
        assert_eq!(
            merge_base_from(&folio.objects, &[a], b, folio.stores().object_limit).unwrap(),
            merge_base(&folio.objects, a, b, folio.stores().object_limit).unwrap()
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

#[test]
fn commit_range_handles_linear_fork_and_merge_histories() {
    use folt_core::history::commit_range;
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let left = commit(&folio, &lock, "left", &[root]);
    let tip = commit(&folio, &lock, "tip", &[left]);
    let right = commit(&folio, &lock, "right", &[root]);
    let merge = commit(&folio, &lock, "merge", &[tip, right]);
    let limit = folio.stores().object_limit;
    for (tip, exclude, expected) in [
        (tip, root, vec![left, tip]),
        (tip, right, vec![left, tip]),
        (merge, left, vec![tip, right, merge]),
        (left, merge, vec![]),
        (root, root, vec![]),
    ] {
        assert_eq!(
            commit_range(&folio.objects, tip, exclude, limit).unwrap(),
            expected.into_iter().collect()
        );
    }
    assert!(matches!(
        commit_range(&folio.objects, tip, right, 1),
        Err(StoreError::Object(ObjectError::LimitExceeded))
    ));
    let missing = ObjectId::from_bytes([77; 32]);
    assert!(commit_range(&folio.objects, tip, missing, limit).is_err());
    let broken = commit(&folio, &lock, "broken", &[root, missing]);
    assert!(commit_range(&folio.objects, broken, broken, limit).is_err());
}

#[test]
fn merge_base_from_unites_starts_without_writing_objects_or_refs() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let a = commit(&folio, &lock, "a", &[]);
    let b = commit(&folio, &lock, "b", &[]);
    let s1 = commit(&folio, &lock, "s1", &[a]);
    let s2 = commit(&folio, &lock, "s2", &[b]);
    let t = commit(&folio, &lock, "t", &[a, b]);
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &RefName::parse("topic").unwrap(),
            t,
            Expect::Absent,
        )
        .unwrap();
    let limit = folio.stores().object_limit;
    let mut expected = vec![a, b];
    expected.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    // Release database and home lock files before raw reads on Windows.
    drop(folio.registry);
    drop(lock);
    let before = snapshot(home.path());
    for starts in [vec![s1, s2], vec![s2, s1], vec![s1, s2, s1, s2]] {
        assert_eq!(
            merge_base_from(&folio.objects, &starts, t, limit).unwrap(),
            MergeBase::Several(expected.clone())
        );
    }
    assert_eq!(
        merge_base_from(&folio.objects, &[], t, limit).unwrap(),
        MergeBase::None
    );
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn merge_base_from_reduces_common_ancestors_across_starts() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let a = commit(&folio, &lock, "a", &[]);
    let b = commit(&folio, &lock, "b", &[a]);
    let s1 = commit(&folio, &lock, "s1", &[a]);
    let s2 = commit(&folio, &lock, "s2", &[b]);
    let t = commit(&folio, &lock, "t", &[b]);
    let limit = folio.stores().object_limit;
    assert_eq!(
        merge_base_from(&folio.objects, &[s1], t, limit).unwrap(),
        MergeBase::One(a)
    );
    for starts in [vec![s1, s2], vec![s2, s1], vec![s2, s2, s1]] {
        assert_eq!(
            merge_base_from(&folio.objects, &starts, t, limit).unwrap(),
            MergeBase::One(b)
        );
    }
}

#[test]
fn merge_base_from_validates_all_starts_and_the_other_side() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let valid = commit(&folio, &lock, "valid", &[root]);
    let missing = ObjectId::from_bytes([42; 32]);
    let broken = commit(&folio, &lock, "broken", &[root, missing]);
    let limit = folio.stores().object_limit;
    drop(folio.registry);
    drop(lock);
    let before = snapshot(home.path());
    for id in [missing, broken] {
        for (starts, other) in [
            (vec![id, valid], root),
            (vec![valid, id], root),
            (vec![root, valid], id),
            (vec![], id),
        ] {
            let error = merge_base_from(&folio.objects, &starts, other, limit).unwrap_err();
            let underlying = folio.objects.get_commit(missing, limit).unwrap_err();
            match (error, underlying) {
                (
                    StoreError::Io { path, source },
                    StoreError::Io {
                        path: expected_path,
                        source: expected_source,
                    },
                ) => {
                    assert_eq!(path, expected_path);
                    assert_eq!(source.kind(), expected_source.kind());
                }
                (StoreError::NotRegularFile(path), StoreError::NotRegularFile(expected)) => {
                    assert_eq!(path, expected);
                }
                errors => panic!("unexpected store errors: {errors:?}"),
            }
        }
    }
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn merge_base_from_propagates_corruption_and_per_object_limits() {
    let home = TempHome::new();
    let lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let root = commit(&folio, &lock, "root", &[]);
    let large = commit(&folio, &lock, &"large".repeat(100), &[root]);
    let broken = commit(&folio, &lock, "broken", &[root, large]);
    let limit = folio.stores().object_limit;
    let root_size = folio
        .objects
        .get_commit(root, limit)
        .unwrap()
        .encode_body()
        .unwrap()
        .len() as u64;
    assert_eq!(
        merge_base_from(&folio.objects, &[root, root], root, root_size).unwrap(),
        MergeBase::One(root)
    );
    for (starts, other) in [
        (vec![large, root], root),
        (vec![root, large], root),
        (vec![root], broken),
        (vec![], broken),
    ] {
        assert!(matches!(
            merge_base_from(&folio.objects, &starts, other, root_size),
            Err(StoreError::Object(ObjectError::LimitExceeded))
        ));
    }
    drop(folio.registry);
    drop(lock);
    let path = snapshot(home.path())
        .into_keys()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == large.to_string().as_str())
        })
        .unwrap();
    std::fs::write(path, b"corrupt").unwrap();
    let before = snapshot(home.path());
    for (starts, other) in [
        (vec![broken, root], root),
        (vec![root, broken], root),
        (vec![root], broken),
        (vec![], broken),
    ] {
        assert!(matches!(
            merge_base_from(&folio.objects, &starts, other, limit),
            Err(StoreError::Object(_))
        ));
    }
    assert_eq!(snapshot(home.path()), before);
}
