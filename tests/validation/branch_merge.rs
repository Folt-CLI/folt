use std::path::{Path, PathBuf};

use folt_core::folio::name::FolioName;
use folt_core::front_matter::Reference;
use folt_core::history::Expect;
use folt_core::home::HomeLock;
use folt_core::id::ObjectId;
use folt_core::ops::history::{
    HistoryError, HistoryRefusal, HistoryStores, MergeOutcome, MergeRequest,
};
use folt_core::placement::{PlacementKind, PlacementOps};
use folt_core::placement_fs::PlacementFs;
use folt_core::placement_path::PlacementPath;
use folt_core::ref_name::{RefKind, RefName};
use folt_core::registry::Placement;

use crate::common::TempHome;
use crate::support::{TestFolio, metadata};

fn merge(
    home: &TempHome,
    folio: &mut TestFolio,
    lock: &mut HomeLock,
    row: &Placement,
) -> Result<ObjectId, HistoryError> {
    home.home
        .merge_branch(
            row,
            MergeRequest {
                other: &RefName::parse("topic").unwrap(),
                metadata: metadata(),
            },
            &PlacementFs::new(&home.home),
            &HistoryStores {
                objects: &folio.objects,
                refs: &folio.refs,
                object_limit: 16384,
            },
            &mut folio.registry,
            lock,
        )
        .map(|outcome| match outcome {
            MergeOutcome::Merged(id) => id,
            MergeOutcome::Conflicted(_) => panic!("expected a clean merge"),
        })
}

pub(super) fn snapshot(path: &Path, home_lock: &Path) -> Vec<(PathBuf, ObjectId)> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(snapshot(&path, home_lock));
        // Windows prevents reading the held home lock. SQLite shared memory
        // changes during reads; neither file contains the state under test.
        } else if path != home_lock && path.file_name().unwrap() != "registry.db-shm" {
            files.push((
                path.clone(),
                folt_core::object::hash_blob(&std::fs::read(path).unwrap()),
            ));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn refused(
    home: &TempHome,
    folio: &mut TestFolio,
    lock: &mut HomeLock,
    row: &Placement,
) -> HistoryRefusal {
    let home_lock = home.path().join("lock");
    let before = snapshot(home.root(), &home_lock);
    let error = merge(home, folio, lock, row).unwrap_err();
    assert_eq!(
        snapshot(home.root(), &home_lock),
        before,
        "refusal must write nothing"
    );
    let HistoryError::Refused(refusal) = error else {
        panic!("unexpected error: {error}");
    };
    refusal
}

#[test]
fn snapshot_skips_held_home_lock_but_keeps_working_file_named_lock() {
    let home = TempHome::new();
    let _lock = home.lock();
    let home_lock = home.path().join("lock");
    let working_file = home.root().join("lock");
    std::fs::write(&working_file, b"working content").unwrap();
    let files = snapshot(home.root(), &home_lock);
    assert!(!files.iter().any(|(path, _)| path == &home_lock));
    assert!(files.contains(&(
        working_file.clone(),
        folt_core::object::hash_blob(b"working content")
    )));
    std::fs::write(&working_file, b"edited content").unwrap();
    assert_ne!(snapshot(home.root(), &home_lock), files);
}

pub(super) fn copy(
    home: &TempHome,
    folio: &mut TestFolio,
    lock: &HomeLock,
    branch: &str,
    path: &str,
) -> Placement {
    let mut row = folio.internal();
    row.path = PlacementPath::resolve(home.root().join(path)).unwrap();
    row.kind = PlacementKind::Copy;
    row.head = branch.into();
    let content = folio
        .objects
        .get_commit(row.commit_id.unwrap(), 16384)
        .unwrap()
        .content;
    row.base = PlacementFs::new(&home.home)
        .create(row.id, &row.to_placement().1, &folio.objects, content)
        .unwrap()
        .base;
    folio
        .registry
        .write(lock, |tx| {
            tx.add_placement(&folt_core::registry::NewPlacement {
                path: row.path.clone(),
                id: row.id,
                kind: row.kind,
                head: Some(row.head.clone()),
                commit_id: row.commit_id,
                base: row.base.clone(),
                staged: row.staged,
            })
        })
        .unwrap();
    row
}

#[test]
fn scenario_1_diverged_main_copy_records_parents_and_updates_internal_only() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"a\nb\nc\n");
    let other = folio.second_branch(&home, &mut lock, "topic", base, b"a\nb\nC\n");
    let ours = folio.commit_internal(&home, &mut lock, b"A\nb\nc\n");
    let row = copy(&home, &mut folio, &lock, "main", "copy.md");
    let untouched = copy(&home, &mut folio, &lock, "main", "another.md");
    let id = merge(&home, &mut folio, &mut lock, &row).unwrap();
    let commit = folio.objects.get_commit(id, 16384).unwrap();
    assert_eq!(commit.parents, vec![ours, other]);
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"A\nb\nC\n");
    assert_eq!(
        std::fs::read(folio.internal().path.to_path_buf()).unwrap(),
        b"A\nb\nC\n"
    );
    assert_eq!(folio.internal().commit_id, Some(id));
    assert_eq!(
        folio.registry.placement_at(&untouched.path).unwrap(),
        Some(untouched)
    );
    assert_eq!(
        folio
            .refs
            .read(RefKind::Branch, &RefName::parse("topic").unwrap())
            .unwrap(),
        Some(other)
    );
    let updated = folio.registry.placement_at(&row.path).unwrap().unwrap();
    assert_eq!(updated.commit_id, Some(id));
    assert_eq!(updated.base.as_ref().unwrap().hash, commit.content);
    assert!(updated.staged.is_none());
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &updated),
        HistoryRefusal::MergeAlreadyMerged
    );
}

#[test]
fn scenario_2_fast_forward_writes_merge_commit_and_contained_branch_writes_nothing() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base");
    let other = folio.second_branch(&home, &mut lock, "topic", base, b"theirs");
    let row = folio.internal();
    let id = merge(&home, &mut folio, &mut lock, &row).unwrap();
    assert_ne!(id, other);
    assert_eq!(
        folio.objects.get_commit(id, 16384).unwrap().parents,
        vec![base, other]
    );
    let internal = folio.internal();
    let new_topic = folio.second_branch(&home, &mut lock, "target", id, b"target");
    let mut row = copy(&home, &mut folio, &lock, "target", "copy.bin");
    let target = folio.objects.get_commit(new_topic, 16384).unwrap();
    row.base = PlacementFs::new(&home.home)
        .refresh(
            row.id,
            &row.to_placement().1,
            &folio.objects,
            target.content,
            None,
        )
        .unwrap()
        .base;
    row.commit_id = Some(new_topic);
    folio
        .registry
        .write(&lock, |tx| tx.update_placement(&row))
        .unwrap();
    // The requested branch is already contained by target, so it writes nothing.
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeAlreadyMerged
    );
    assert_eq!(folio.internal(), internal);
}

#[test]
fn scenario_3_binary_conflict_and_no_base_write_nothing() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base\n");
    folio.second_branch(&home, &mut lock, "topic", base, b"theirs\0\n");
    folio.commit_internal(&home, &mut lock, b"ours\n");
    let row = folio.internal();
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeConflict
    );
    let unrelated = folio.commit_object(&lock, b"unrelated");
    let topic = RefName::parse("topic").unwrap();
    let old = folio.refs.read(RefKind::Branch, &topic).unwrap().unwrap();
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &topic,
            unrelated,
            Expect::Is(old),
        )
        .unwrap();
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeNoBase
    );
}

#[test]
fn scenario_4_stage_edits_missing_branch_and_behind_write_nothing() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base");
    let row = folio.internal();
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MissingBranch
    );
    folio.second_branch(&home, &mut lock, "topic", base, b"other");
    std::fs::write(row.path.to_path_buf(), b"edits").unwrap();
    assert!(matches!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeEdited(_)
    ));
    home.home
        .stage(
            &row,
            &PlacementFs::new(&home.home),
            &folio.objects,
            &folio.refs,
            &mut folio.registry,
            &lock,
        )
        .unwrap();
    let staged = folio.internal();
    assert!(matches!(
        refused(&home, &mut folio, &mut lock, &staged),
        HistoryRefusal::MergeEdited(_)
    ));
    home.home
        .unstage(
            &staged,
            &PlacementFs::new(&home.home),
            &mut folio.registry,
            &lock,
        )
        .unwrap();
    std::fs::write(row.path.to_path_buf(), b"base").unwrap();
    let advanced = folio.commit_with_parents(&lock, b"advanced", &[base]);
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &RefName::parse("main").unwrap(),
            advanced,
            Expect::Is(base),
        )
        .unwrap();
    assert!(matches!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::BranchMoved { .. }
    ));
}

#[test]
fn scenario_5_duplicate_and_ambiguous_keys_write_nothing() {
    for invalid in [
        "---\nfolio-name: a\nfolio-name: b\n---\n",
        "---\n? [folio-name]\n: value\n---\n",
    ] {
        let home = TempHome::new();
        let mut lock = home.lock();
        let mut folio = TestFolio::new(&home, &lock);
        let base = folio.commit_internal(&home, &mut lock, b"base");
        folio.second_branch(&home, &mut lock, "topic", base, invalid.as_bytes());
        let row = copy(&home, &mut folio, &lock, "main", "copy.md");
        assert!(matches!(
            refused(&home, &mut folio, &mut lock, &row),
            HistoryRefusal::MergeInvalid(_)
        ));
    }
    // Each side is valid, but independent additions produce a duplicate alias.
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base_bytes = b"---\nfolio-name: parent\nfolio-references:\n  first: {name: missing}\n  second: {name: missing}\n---\nbody\n";
    let ours = b"---\nfolio-name: parent\nfolio-references:\n  repeated: {name: left}\n  first: {name: missing}\n  second: {name: missing}\n---\nbody\n";
    let theirs = b"---\nfolio-name: parent\nfolio-references:\n  first: {name: missing}\n  second: {name: missing}\n  repeated: {name: right}\n---\nbody\n";
    assert!(folt_core::front_matter::read_references(ours).is_ok());
    assert!(folt_core::front_matter::read_references(theirs).is_ok());
    assert!(matches!(
        folt_core::merge::merge3(base_bytes, ours, theirs),
        folt_core::merge::MergeResult::Clean(_)
    ));
    let base = folio.commit_internal(&home, &mut lock, base_bytes);
    folio.second_branch(&home, &mut lock, "topic", base, theirs);
    folio.commit_internal(&home, &mut lock, ours);
    let row = folio.internal();
    assert!(matches!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeInvalid(_)
    ));
}

#[test]
fn scenario_6_reference_cycles_follow_cached_main_edges() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base");
    let child = TestFolio::new(&home, &lock);
    let grandchild = TestFolio::new(&home, &lock);
    let id = folio.folio.id();
    folio
        .registry
        .write(&lock, |tx| {
            tx.set_name(child.folio.id(), &FolioName::parse("child").unwrap())?;
            tx.rebuild_references(
                child.folio.id(),
                "main",
                &[Reference {
                    alias: "next".into(),
                    name: None,
                    id: Some(grandchild.folio.id()),
                    pin: None,
                }],
            )?;
            tx.rebuild_references(
                grandchild.folio.id(),
                "main",
                &[Reference {
                    alias: "back".into(),
                    name: None,
                    id: Some(id),
                    pin: None,
                }],
            )
        })
        .unwrap();
    let bytes = b"---\nfolio-name: parent\nfolio-references:\n  child: {name: child}\n---\n";
    folio.second_branch(&home, &mut lock, "topic", base, bytes);
    let row = copy(&home, &mut folio, &lock, "main", "copy.md");
    let refusal = refused(&home, &mut folio, &mut lock, &row);
    assert!(
        matches!(&refusal, HistoryRefusal::MergeInvalid(message) if message.contains("reference cycle")),
        "{refusal}"
    );
    // A cycle among referenced Folios that never returns here is allowed.
    folio
        .registry
        .write(&lock, |tx| {
            tx.rebuild_references(
                grandchild.folio.id(),
                "main",
                &[Reference {
                    alias: "elsewhere".into(),
                    name: None,
                    id: Some(child.folio.id()),
                    pin: None,
                }],
            )
        })
        .unwrap();
    let id = merge(&home, &mut folio, &mut lock, &row).unwrap();
    assert_eq!(
        folio
            .registry
            .placement_at(&row.path)
            .unwrap()
            .unwrap()
            .commit_id,
        Some(id)
    );
    let cached = folio.registry.references(folio.folio.id(), "main").unwrap();
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].child, child.folio.id().to_string());
    assert_eq!(cached[0].key, "child");
}

#[test]
fn scenario_7_name_collision_and_markdown_gating() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base");
    let child = TestFolio::new(&home, &lock);
    folio
        .registry
        .write(&lock, |tx| {
            tx.set_name(child.folio.id(), &FolioName::parse("taken").unwrap())
        })
        .unwrap();
    folio.second_branch(
        &home,
        &mut lock,
        "topic",
        base,
        b"---\nfolio-name: taken\n---\nbody\n",
    );
    let row = folio.internal();
    assert!(
        matches!(refused(&home, &mut folio, &mut lock, &row), HistoryRefusal::MergeInvalid(message) if message.contains("different Folio"))
    );
    let mut row = copy(&home, &mut folio, &lock, "main", "copy.txt");
    // A non-Markdown copy on another branch does not validate front matter.
    let name = RefName::parse("binary-target").unwrap();
    home.home
        .create_branch(
            row.id,
            &name,
            base,
            &folio.refs,
            &mut folio.registry,
            &mut lock,
        )
        .unwrap();
    row.head = name.as_str().into();
    folio
        .registry
        .write(&lock, |tx| tx.update_placement(&row))
        .unwrap();
    let internal = folio.internal();
    assert!(merge(&home, &mut folio, &mut lock, &row).is_ok());
    assert_eq!(folio.internal(), internal);
    folio
        .registry
        .write(&lock, |tx| {
            tx.set_name(child.folio.id(), &FolioName::parse("other-name").unwrap())?;
            tx.set_name(folio.folio.id(), &FolioName::parse("taken").unwrap())?;
            Ok(())
        })
        .unwrap();
    assert!(merge(&home, &mut folio, &mut lock, &internal).is_ok());
}

// Install a clean target tip while preserving the two-parent history built by
// the fixture. This avoids making a staged commit that would add another tip.
fn install_tip(
    home: &TempHome,
    folio: &mut TestFolio,
    lock: &mut HomeLock,
    branch: &str,
    tip: ObjectId,
) {
    let name = RefName::parse(branch).unwrap();
    let old = folio.refs.read(RefKind::Branch, &name).unwrap().unwrap();
    folio
        .refs
        .update(lock, RefKind::Branch, &name, tip, Expect::Is(old))
        .unwrap();
    if branch == "main" {
        let mut row = folio.internal();
        let content = folio.objects.get_commit(tip, 16384).unwrap().content;
        row.base = PlacementFs::new(&home.home)
            .refresh(row.id, &row.to_placement().1, &folio.objects, content, None)
            .unwrap()
            .base;
        row.commit_id = Some(tip);
        folio
            .registry
            .write(lock, |tx| tx.update_placement(&row))
            .unwrap();
    }
}

#[test]
fn criss_cross_clean_virtual_base_stores_only_merged_blob_and_commit() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"a\nb\nc\nd\ne\n");
    let a = folio.second_branch(&home, &mut lock, "topic", base, b"A\nb\nc\nd\ne\n");
    let b = folio.commit_internal(&home, &mut lock, b"a\nb\nC\nd\ne\n");
    let ours = folio.commit_with_parents(&lock, b"A\nB\nC\nd\ne\n", &[a, b]);
    let theirs = folio.commit_with_parents(&lock, b"A\nb\nC\nd\nE\n", &[b, a]);
    install_tip(&home, &mut folio, &mut lock, "main", ours);
    install_tip(&home, &mut folio, &mut lock, "topic", theirs);
    let objects = folio.folio.path().to_path_buf();
    let before = snapshot(&objects, &home.path().join("lock"));
    let row = copy(&home, &mut folio, &lock, "main", "copy.md");
    let id = merge(&home, &mut folio, &mut lock, &row).unwrap();
    let commit = folio.objects.get_commit(id, 16384).unwrap();
    assert_eq!(commit.parents, vec![ours, theirs]);
    let expected = b"A\nB\nC\nd\nE\n";
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), expected);
    assert_eq!(
        std::fs::read(folio.internal().path.to_path_buf()).unwrap(),
        expected
    );
    assert_eq!(folio.internal().commit_id, Some(id));
    let after = snapshot(&objects, &home.path().join("lock"));
    let added: Vec<_> = after
        .iter()
        .filter(|(path, _)| !before.iter().any(|(old, _)| old == path))
        .collect();
    assert_eq!(
        added.len(),
        2,
        "only the merged blob and merge commit are stored"
    );
    assert_eq!(
        folio
            .objects
            .get(
                commit.content,
                folt_core::object::ObjectKind::Blob,
                u64::MAX
            )
            .unwrap(),
        expected
    );
    assert!(
        folio
            .objects
            .get(
                folt_core::object::hash_blob(b"A\nb\nC\nd\ne\n"),
                folt_core::object::ObjectKind::Blob,
                u64::MAX
            )
            .is_err()
    );
}

#[test]
fn criss_cross_text_conflicts_in_virtual_base_still_merge() {
    use folt_core::merge::{MergeResult, merge3};
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let original = b"start\nspacer\nmiddle\nspacer\nend\n";
    let left: &[u8] = b"start\nspacer\nleft\nspacer\nend\n";
    let right: &[u8] = b"start\nspacer\nright\nspacer\nend\n";
    let base = folio.commit_internal(&home, &mut lock, original);
    let a = folio.second_branch(&home, &mut lock, "topic", base, left);
    let b = folio.commit_internal(&home, &mut lock, right);
    let (first, second) = if a.as_bytes() < b.as_bytes() {
        (left, right)
    } else {
        (right, left)
    };
    let MergeResult::Conflicted {
        merged: Some(virtual_bytes),
        marker_lines,
    } = merge3(original, first, second)
    else {
        panic!("ancestors must produce text conflicts");
    };
    assert_eq!(
        marker_lines,
        vec![
            b"<<<<<<<\n".to_vec(),
            b"=======\n".to_vec(),
            b">>>>>>>\n".to_vec()
        ]
    );
    let our_bytes = b"START\nspacer\nresolved\nspacer\nend\n";
    let their_bytes = b"start\nspacer\nresolved\nspacer\nEND\n";
    let expected = b"START\nspacer\nresolved\nspacer\nEND\n";
    assert_eq!(
        merge3(&virtual_bytes, our_bytes, their_bytes),
        MergeResult::Clean(expected.to_vec())
    );
    let ours = folio.commit_with_parents(&lock, our_bytes, &[a, b]);
    let theirs = folio.commit_with_parents(&lock, their_bytes, &[b, a]);
    install_tip(&home, &mut folio, &mut lock, "main", ours);
    install_tip(&home, &mut folio, &mut lock, "topic", theirs);
    let row = copy(&home, &mut folio, &lock, "main", "copy.md");
    let id = merge(&home, &mut folio, &mut lock, &row).unwrap();
    assert_eq!(
        folio.objects.get_commit(id, 16384).unwrap().parents,
        vec![ours, theirs]
    );
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), expected);
    let internal = folio.internal();
    assert_eq!(
        std::fs::read(internal.path.to_path_buf()).unwrap(),
        expected
    );
    assert_eq!(internal.commit_id, Some(id));
    let updated = folio.registry.placement_at(&row.path).unwrap().unwrap();
    assert_eq!(updated.commit_id, Some(id));
    assert_eq!(
        updated.base.as_ref().unwrap().hash,
        folt_core::object::hash_blob(expected)
    );
    assert!(updated.staged.is_none());
}

#[test]
fn criss_cross_binary_conflict_between_ancestors_writes_nothing() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base");
    let a = folio.second_branch(&home, &mut lock, "topic", base, b"left\0");
    let b = folio.commit_internal(&home, &mut lock, b"right");
    // Equal final tips would merge cleanly with a missing base. The ancestor
    // refusal must take precedence even when their content agrees.
    let ours = folio.commit_with_parents(&lock, b"resolved", &[a, b]);
    let theirs = folio.commit_with_parents(&lock, b"resolved", &[b, a]);
    install_tip(&home, &mut folio, &mut lock, "main", ours);
    install_tip(&home, &mut folio, &mut lock, "topic", theirs);
    let row = folio.internal();
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeBaseBinaryConflict
    );
}

fn ordered_ancestor(
    folio: &TestFolio,
    lock: &HomeLock,
    bytes: &[u8],
    parent: ObjectId,
    range: std::ops::Range<u8>,
) -> ObjectId {
    use folt_core::object::{ObjectKind, hash_object};
    let seed = folio.commit_with_parents(lock, bytes, &[parent]);
    let mut candidate = folio.objects.get_commit(seed, 16384).unwrap();
    for nonce in 0..256 {
        candidate.message = format!("ordered ancestor {nonce}");
        let id = hash_object(ObjectKind::Commit, &candidate.encode_body().unwrap());
        if range.contains(&id.as_bytes()[0]) {
            assert_eq!(folio.objects.put_commit(lock, &candidate).unwrap(), id);
            return id;
        }
    }
    panic!("bounded metadata search must find an ordered ancestor");
}

#[test]
fn accumulated_ancestors_merge_stores_only_final_blob_and_commit() {
    use folt_core::history::{MergeBase, merge_base, merge_base_from};
    use folt_core::object::{ObjectKind, hash_blob};
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let r = folio.commit_internal(&home, &mut lock, b"a\nb\nc\nd\ne\n");
    folio.second_branch(&home, &mut lock, "topic", r, b"topic\n");
    let d = folio.commit_with_parents(&lock, b"A\nb\nc\nd\ne\n", &[r]);
    let a = ordered_ancestor(&folio, &lock, b"A\nb\nc\nd\ne\n", d, 0..64);
    let b = ordered_ancestor(&folio, &lock, b"a\nb\nc\nd\ne\n", r, 64..128);
    let c = ordered_ancestor(&folio, &lock, b"a\nb\nc\nd\ne\n", d, 128..255);
    assert!(a.as_bytes() < b.as_bytes() && b.as_bytes() < c.as_bytes());
    assert_eq!(
        merge_base(&folio.objects, b, c, 16384).unwrap(),
        MergeBase::One(r)
    );
    assert_eq!(
        merge_base_from(&folio.objects, &[a, b], c, 16384).unwrap(),
        MergeBase::One(d)
    );
    let ab = folio.commit_with_parents(&lock, b"A\nb\nc\nd\ne\n", &[a, b]);
    let bc = folio.commit_with_parents(&lock, b"a\nb\nc\nd\ne\n", &[b, c]);
    let ours = folio.commit_with_parents(&lock, b"A\nb\nc\nd\ne\n", &[ab, c]);
    let theirs = folio.commit_with_parents(&lock, b"a\nb\nc\nd\nE\n", &[a, bc]);
    assert_ne!(ab, bc);
    assert_eq!(
        merge_base(&folio.objects, ours, theirs, 16384).unwrap(),
        MergeBase::Several(vec![a, b, c])
    );
    install_tip(&home, &mut folio, &mut lock, "main", ours);
    install_tip(&home, &mut folio, &mut lock, "topic", theirs);
    let row = copy(&home, &mut folio, &lock, "main", "copy.md");
    let object_dir = folio.folio.path().to_path_buf();
    let before = snapshot(&object_dir, &home.path().join("lock"));
    let id = merge(&home, &mut folio, &mut lock, &row).unwrap();
    let commit = folio.objects.get_commit(id, 16384).unwrap();
    assert_eq!(commit.parents, vec![ours, theirs]);
    let expected = b"A\nb\nc\nd\nE\n";
    assert_eq!(
        folio
            .objects
            .get(commit.content, ObjectKind::Blob, u64::MAX)
            .unwrap(),
        expected
    );
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), expected);
    assert_eq!(
        std::fs::read(folio.internal().path.to_path_buf()).unwrap(),
        expected
    );
    assert_eq!(folio.internal().commit_id, Some(id));
    assert_eq!(
        folio
            .registry
            .placement_at(&row.path)
            .unwrap()
            .unwrap()
            .commit_id,
        Some(id)
    );
    let after = snapshot(&object_dir, &home.path().join("lock"));
    let added: Vec<_> = after
        .iter()
        .filter(|(path, _)| !before.iter().any(|(old, _)| old == path))
        .collect();
    let mut expected_ids = vec![id.to_string(), hash_blob(expected).to_string()];
    expected_ids.sort();
    let mut added_ids: Vec<_> = added
        .iter()
        .map(|(path, _)| path.file_stem().unwrap().to_str().unwrap().to_owned())
        .collect();
    added_ids.sort();
    assert_eq!(
        added_ids, expected_ids,
        "only the final merged blob and merge commit are stored"
    );
}
