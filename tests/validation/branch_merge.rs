use std::path::{Path, PathBuf};

use folt_core::folio::name::FolioName;
use folt_core::front_matter::Reference;
use folt_core::history::Expect;
use folt_core::home::HomeLock;
use folt_core::id::ObjectId;
use folt_core::ops::history::{HistoryError, HistoryRefusal, HistoryStores, MergeRequest};
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
    home.home.merge_branch(
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
}

fn snapshot(path: &Path, home_lock: &Path) -> Vec<(PathBuf, ObjectId)> {
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

fn copy(
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
fn scenario_3_conflict_no_base_and_criss_cross_write_nothing() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let base = folio.commit_internal(&home, &mut lock, b"base\n");
    folio.second_branch(&home, &mut lock, "topic", base, b"theirs\n");
    let ours = folio.commit_internal(&home, &mut lock, b"ours\n");
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
    let sibling = folio.commit_with_parents(&lock, b"sibling", &[base]);
    let left = folio.commit_with_parents(&lock, b"left", &[ours, sibling]);
    let right = folio.commit_with_parents(&lock, b"right", &[sibling, ours]);
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &topic,
            right,
            Expect::Is(unrelated),
        )
        .unwrap();
    let mut row = row;
    row.commit_id = Some(left);
    folio
        .registry
        .write(&lock, |tx| tx.update_placement(&row))
        .unwrap();
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &RefName::parse("main").unwrap(),
            left,
            Expect::Is(ours),
        )
        .unwrap();
    assert_eq!(
        refused(&home, &mut folio, &mut lock, &row),
        HistoryRefusal::MergeCrissCross
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
