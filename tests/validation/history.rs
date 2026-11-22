use folt_core::history::Expect;
use folt_core::object::{Commit, Tag};
use folt_core::ops::history::{self, HistoryStores};
use folt_core::placement::PlacementOps;
use folt_core::placement_fs::PlacementFs;
use folt_core::ref_name::{RefKind, RefName};

use crate::common::TempHome;
use crate::support::{TestFolio, metadata};

#[test]
fn commit_publishes_content_ref_and_working_base() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let first = folio.commit_internal(&home, &mut lock, b"first");
    let second = folio.commit_internal(&home, &mut lock, b"second");
    let commit = folio.objects.get_commit(second, 16384).unwrap();
    assert_eq!(commit.parents, vec![first]);
    assert_eq!(
        folio
            .refs
            .read(RefKind::Branch, &RefName::parse("main").unwrap())
            .unwrap(),
        Some(second)
    );
    let row = folio.internal();
    assert_eq!(row.commit_id, Some(second));
    assert_eq!(row.staged, None);
    assert_eq!(row.base.unwrap().hash, commit.content);
}

#[test]
fn resumes_commit_published_before_registry_update() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let row = folio.internal();
    std::fs::write(row.path.to_path_buf(), b"staged").unwrap();
    let fs = PlacementFs::new(&home.home);
    home.home
        .stage(
            &row,
            &fs,
            &folio.objects,
            &folio.refs,
            &mut folio.registry,
            &lock,
        )
        .unwrap();
    let row = folio.internal();
    let metadata = metadata();
    let published = folio
        .objects
        .put_commit(
            &lock,
            &Commit {
                content: row.staged.unwrap().hash,
                parents: vec![],
                author: metadata.author.clone(),
                time: metadata.time,
                message: metadata.message.clone(),
                body: None,
                group: None,
            },
        )
        .unwrap();
    folio
        .refs
        .update(
            &mut lock,
            RefKind::Branch,
            &RefName::parse("main").unwrap(),
            published,
            Expect::Absent,
        )
        .unwrap();
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_commit(&row, &fs, &stores, &folio.registry)
        .unwrap();
    let plan = home.home.judge_commit(&facts).unwrap();
    assert_eq!(plan.resumed_commit(), Some(published));
    assert_eq!(
        home.home
            .apply_commit(
                &plan,
                metadata,
                &fs,
                &stores,
                &mut folio.registry,
                &mut lock
            )
            .unwrap(),
        published
    );
    assert_eq!(folio.internal().commit_id, Some(published));
    assert_eq!(folio.internal().staged, None);
}

#[test]
fn reset_moves_tip_and_restores_working_content() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let first = folio.commit_internal(&home, &mut lock, b"first");
    folio.commit_internal(&home, &mut lock, b"second");
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let main = RefName::parse("main").unwrap();
    let facts = home
        .home
        .gather_reset(
            folio.folio.id(),
            &main,
            first,
            &fs,
            &stores,
            &folio.registry,
        )
        .unwrap();
    let plan = home.home.judge_reset(&facts, false).unwrap();
    home.home
        .apply_reset(&plan, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    assert_eq!(
        folio.refs.read(RefKind::Branch, &main).unwrap(),
        Some(first)
    );
    let row = folio.internal();
    assert_eq!(row.commit_id, Some(first));
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"first");
    assert_eq!(
        fs.hash_placement(&row.to_placement().1).unwrap(),
        row.base.unwrap().hash
    );
}

#[test]
fn deletes_unused_branch_but_keeps_main_with_internal_placement() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let commit = folio.commit_internal(&home, &mut lock, b"first");
    let topic = RefName::parse("topic").unwrap();
    home.home
        .create_branch(
            folio.folio.id(),
            &topic,
            commit,
            &folio.refs,
            &mut folio.registry,
            &mut lock,
        )
        .unwrap();
    home.home
        .delete_branch(
            folio.folio.id(),
            &topic,
            commit,
            &folio.refs,
            &folio.registry,
            &mut lock,
        )
        .unwrap();
    assert_eq!(folio.refs.read(RefKind::Branch, &topic).unwrap(), None);
    assert!(matches!(
        home.home.delete_branch(
            folio.folio.id(),
            &RefName::parse("main").unwrap(),
            commit,
            &folio.refs,
            &folio.registry,
            &mut lock
        ),
        Err(history::HistoryError::Refused(
            history::HistoryRefusal::BranchInUse(_)
        ))
    ));
}

#[test]
fn lightweight_and_annotated_tags_round_trip() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let folio = TestFolio::new(&home, &lock);
    let commit = folio.commit_object(&lock, b"content");
    let light = RefName::parse("v1").unwrap();
    assert_eq!(
        home.home
            .create_tag(&light, commit, None, &folio.stores(), &mut lock)
            .unwrap(),
        commit
    );
    let name = RefName::parse("v2").unwrap();
    let meta = metadata();
    let annotation = Tag {
        target: commit,
        name: name.to_string(),
        tagger: meta.author,
        time: meta.time,
        message: "release".into(),
    };
    let tagged = home
        .home
        .create_tag(&name, commit, Some(&annotation), &folio.stores(), &mut lock)
        .unwrap();
    assert_eq!(folio.objects.get_tag(tagged, 16384).unwrap(), annotation);
    assert_eq!(folio.refs.read(RefKind::Tag, &name).unwrap(), Some(tagged));
    home.home
        .delete_tag(&light, commit, &folio.refs, &mut lock)
        .unwrap();
    assert_eq!(folio.refs.read(RefKind::Tag, &light).unwrap(), None);
    assert_eq!(folio.refs.read(RefKind::Tag, &name).unwrap(), Some(tagged));
}

#[test]
fn rebase_publishes_once_moves_placements_and_retains_original_refs() {
    use folt_core::id::FolioId;
    use folt_core::object::ObjectKind;
    use folt_core::ops::history::LeftBehind;
    use folt_core::ref_name::pin_name;
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let copy = folio.copy(&home, &lock);
    let root = folio.commit_internal(&home, &mut lock, b"a\nb\nc\n");
    let old = folio.commit_internal(&home, &mut lock, b"a\nb\nC\n");
    let onto = folio.commit_with_parents(&lock, b"A\nb\nc\n", &[root]);
    let light = RefName::parse("light").unwrap();
    let annotated = RefName::parse("annotated").unwrap();
    let pin = pin_name(FolioId::generate(), old);
    let meta = metadata();
    let tag = Tag {
        target: old,
        name: annotated.to_string(),
        tagger: meta.author,
        time: meta.time,
        message: "keep old".into(),
    };
    home.home
        .create_tag(&light, old, None, &folio.stores(), &mut lock)
        .unwrap();
    let tag_id = home
        .home
        .create_tag(&annotated, old, Some(&tag), &folio.stores(), &mut lock)
        .unwrap();
    folio
        .refs
        .update(&mut lock, RefKind::Pin, &pin, old, Expect::Absent)
        .unwrap();
    let main = RefName::parse("main").unwrap();
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_rebase(folio.folio.id(), &main, onto, &fs, &stores, &folio.registry)
        .unwrap();
    let plan = home.home.judge_rebase(&facts, false).unwrap();
    assert_eq!(folio.refs.read(RefKind::Branch, &main).unwrap(), Some(old));
    assert_eq!(
        plan.left_behind(),
        &[
            LeftBehind {
                kind: RefKind::Tag,
                name: annotated.clone(),
                commit: old
            },
            LeftBehind {
                kind: RefKind::Tag,
                name: light.clone(),
                commit: old
            },
            LeftBehind {
                kind: RefKind::Pin,
                name: pin.clone(),
                commit: old
            },
        ]
    );
    let tip = home
        .home
        .apply_rebase(&plan, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    assert_eq!(tip, plan.tip());
    assert_eq!(folio.refs.read(RefKind::Branch, &main).unwrap(), Some(tip));
    let replayed = folio.objects.get_commit(tip, stores.object_limit).unwrap();
    let original = folio.objects.get_commit(old, stores.object_limit).unwrap();
    let mut expected = original.clone();
    expected.parents = vec![onto];
    expected.content = folt_core::object::hash_blob(b"A\nb\nC\n");
    assert_eq!(replayed, expected);
    assert_eq!(
        folio
            .objects
            .get(original.content, ObjectKind::Blob, stores.object_limit)
            .unwrap(),
        b"a\nb\nC\n"
    );
    for row in [
        folio.internal(),
        folio.registry.placement_at(&copy.path).unwrap().unwrap(),
    ] {
        assert_eq!(row.commit_id, Some(tip));
        assert_eq!(row.base.unwrap().hash, replayed.content);
        assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"A\nb\nC\n");
    }
    assert_eq!(folio.refs.read(RefKind::Tag, &light).unwrap(), Some(old));
    assert_eq!(
        folio.refs.read(RefKind::Tag, &annotated).unwrap(),
        Some(tag_id)
    );
    assert_eq!(folio.refs.read(RefKind::Pin, &pin).unwrap(), Some(old));
    assert!(
        home.home
            .apply_rebase(&plan, &fs, &stores, &mut folio.registry, &mut lock)
            .is_err()
    );
    assert_eq!(folio.refs.read(RefKind::Branch, &main).unwrap(), Some(tip));
}

#[test]
fn rebase_rechecks_edits_and_ref_movement_before_publishing() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let root = folio.commit_internal(&home, &mut lock, b"a\nb\nc\n");
    let old = folio.commit_internal(&home, &mut lock, b"a\nb\nC\n");
    let onto = folio.commit_with_parents(&lock, b"A\nb\nc\n", &[root]);
    let main = RefName::parse("main").unwrap();
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_rebase(folio.folio.id(), &main, onto, &fs, &stores, &folio.registry)
        .unwrap();
    let plan = home.home.judge_rebase(&facts, false).unwrap();
    std::fs::write(folio.internal().path.to_path_buf(), b"new edits").unwrap();
    assert!(
        home.home
            .apply_rebase(&plan, &fs, &stores, &mut folio.registry, &mut lock)
            .is_err()
    );
    assert!(
        folio
            .objects
            .get_commit(plan.tip(), stores.object_limit)
            .is_err()
    );
    assert_eq!(folio.refs.read(RefKind::Branch, &main).unwrap(), Some(old));
    folio
        .refs
        .update(&mut lock, RefKind::Branch, &main, onto, Expect::Is(old))
        .unwrap();
    assert!(
        home.home
            .apply_rebase(&plan, &fs, &stores, &mut folio.registry, &mut lock)
            .is_err()
    );
    assert!(
        folio
            .objects
            .get_commit(plan.tip(), stores.object_limit)
            .is_err()
    );
}

#[test]
fn rebase_reads_large_content_independently_of_commit_limit() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let mut root_content = vec![b'x'; 32 * 1024];
    root_content[..2].copy_from_slice(b"a\n");
    let end = root_content.len();
    root_content[end - 3..].copy_from_slice(b"\nz\n");
    let root = folio.commit_internal(&home, &mut lock, &root_content);
    let mut branch_content = root_content.clone();
    branch_content[0] = b'A';
    folio.commit_internal(&home, &mut lock, &branch_content);
    let mut destination_content = root_content;
    destination_content[end - 2] = b'Z';
    let onto = folio.commit_with_parents(&lock, &destination_content, &[root]);
    let main = RefName::parse("main").unwrap();
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_rebase(folio.folio.id(), &main, onto, &fs, &stores, &folio.registry)
        .unwrap();
    let plan = home.home.judge_rebase(&facts, false).unwrap();
    let tip = home
        .home
        .apply_rebase(&plan, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    branch_content[end - 2] = b'Z';
    assert_eq!(
        std::fs::read(folio.internal().path.to_path_buf()).unwrap(),
        branch_content
    );
    assert_eq!(folio.internal().commit_id, Some(tip));
    let limited = HistoryStores {
        object_limit: 1,
        ..stores
    };
    assert!(matches!(
        home.home.gather_rebase(
            folio.folio.id(),
            &main,
            onto,
            &fs,
            &limited,
            &folio.registry
        ),
        Err(history::HistoryError::Store(
            folt_core::store::StoreError::Object(folt_core::object::ObjectError::LimitExceeded)
        ))
    ));
}

#[test]
fn rebase_accepts_working_content_equal_to_final_replay_without_discard() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let mut folio = TestFolio::new(&home, &lock);
    let root = folio.commit_internal(&home, &mut lock, b"a\nb\nc\n");
    folio.commit_internal(&home, &mut lock, b"a\nb\nC\n");
    let onto = folio.commit_with_parents(&lock, b"A\nb\nc\n", &[root]);
    let row = folio.internal();
    std::fs::write(row.path.to_path_buf(), b"A\nb\nC\n").unwrap();
    let main = RefName::parse("main").unwrap();
    let fs = PlacementFs::new(&home.home);
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    let facts = home
        .home
        .gather_rebase(folio.folio.id(), &main, onto, &fs, &stores, &folio.registry)
        .unwrap();
    let plan = home.home.judge_rebase(&facts, false).unwrap();
    let tip = home
        .home
        .apply_rebase(&plan, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    assert_eq!(std::fs::read(row.path.to_path_buf()).unwrap(), b"A\nb\nC\n");
    let updated = folio.internal();
    assert_eq!(updated.commit_id, Some(tip));
    assert_eq!(
        updated.base.unwrap().hash,
        folt_core::object::hash_blob(b"A\nb\nC\n")
    );
    assert_eq!(folio.refs.read(RefKind::Branch, &main).unwrap(), Some(tip));
}
