use super::*;

fn stage_copy(
    home: &TempHome,
    lock: &folt_core::home::HomeLock,
    folio: &mut TestFolio,
    copy: &Placement,
) -> Placement {
    home.home
        .stage(
            &current(folio, copy),
            &PlacementFs::new(&home.home),
            &folio.objects,
            &folio.refs,
            &mut folio.registry,
            lock,
        )
        .unwrap();
    current(folio, copy)
}

#[test]
fn abort_rebase_clears_staged_resolution_and_refuses_a_later_commit() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
    start(&home, &mut lock, &mut folio, &copy, onto);
    std::fs::write(copy.path.to_path_buf(), b"resolved\n").unwrap();
    let fs = PlacementFs::new(&home.home);
    let staged = stage_copy(&home, &lock, &mut folio, &copy);
    assert_eq!(
        staged.staged.unwrap().hash,
        folt_core::object::hash_blob(b"resolved\n")
    );
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    home.home
        .abort_rebase(&staged, true, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    let restored = current(&folio, &copy);
    assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), b"ours\n");
    assert_eq!(restored.staged, None);
    assert_eq!(restored.conflict, None);
    assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_none());
    assert_eq!(restored.commit_id, copy.commit_id);
    let facts = home
        .home
        .gather_commit(&restored, &fs, &stores, &folio.registry)
        .unwrap();
    assert_eq!(
        home.home.judge_commit(&facts).unwrap_err(),
        HistoryRefusal::NothingStaged
    );
    assert_eq!(
        folio
            .refs
            .read(RefKind::Branch, &RefName::parse("main").unwrap())
            .unwrap(),
        copy.commit_id
    );
}

#[test]
fn abort_rebase_clears_references_captured_from_staged_resolution() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, mut copy, onto, _) = fixture(&home, &mut lock, false);
    let markdown_path = home.root().join("copy.md");
    std::fs::rename(copy.path.to_path_buf(), &markdown_path).unwrap();
    let new_path = folt_core::placement_path::PlacementPath::resolve(markdown_path).unwrap();
    folio
        .registry
        .write(&lock, |tx| {
            tx.move_placement(&copy.path, &new_path).map(|_| ())
        })
        .unwrap();
    copy.path = new_path;
    let original = b"---\nfolio-references:\n  kept: {name: original-child}\n---\nours\n";
    let tip = folio.commit_internal(&home, &mut lock, original);
    let fs = PlacementFs::new(&home.home);
    let content = folio.objects.get_commit(tip, 16384).unwrap().content;
    copy.base = fs
        .refresh(
            copy.id,
            &copy.to_placement().1,
            &folio.objects,
            content,
            None,
        )
        .unwrap()
        .base;
    copy.commit_id = Some(tip);
    folio
        .registry
        .write(&lock, |tx| tx.update_placement(&copy))
        .unwrap();
    assert_eq!(stage_copy(&home, &lock, &mut folio, &copy).staged, None);
    let before = folio.registry.references(copy.id, &copy.head).unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].child_label, "original-child");
    start(&home, &mut lock, &mut folio, &copy, onto);
    std::fs::write(
        copy.path.to_path_buf(),
        b"---\nfolio-references:\n  child: {name: canceled-child}\n---\nresolved\n",
    )
    .unwrap();
    let staged = stage_copy(&home, &lock, &mut folio, &copy);
    assert!(staged.staged.is_some());
    let references = folio.registry.references(copy.id, &copy.head).unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].child_label, "canceled-child");
    let stores = HistoryStores {
        objects: &folio.objects,
        refs: &folio.refs,
        object_limit: 16384,
    };
    home.home
        .abort_rebase(&staged, true, &fs, &stores, &mut folio.registry, &mut lock)
        .unwrap();
    assert_eq!(
        folio.registry.references(copy.id, &copy.head).unwrap(),
        before
    );
    assert_eq!(current(&folio, &copy).staged, None);
    assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), original);
}
