use super::*;
use folt_core::placement_path::PlacementPath;

fn remove_copy(
    home: &TempHome,
    lock: &folt_core::home::HomeLock,
    folio: &mut TestFolio,
    copy: &Placement,
    delete_file: bool,
) {
    let fs = PlacementFs::new(&home.home);
    let facts = home
        .home
        .gather_facts(std::slice::from_ref(copy), &fs, |_| Ok(&folio.refs))
        .unwrap();
    let plan = home
        .home
        .judge_remove(&facts[0], delete_file, delete_file)
        .unwrap();
    home.home
        .apply_remove(&plan, &fs, &mut folio.registry, lock)
        .unwrap();
}

fn stored_files(path: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(stored_files(&path));
        } else {
            let bytes = std::fs::read(&path).unwrap();
            files.push((path, bytes));
        }
    }
    files.sort();
    files
}

#[test]
fn removing_stopped_copy_clears_progress_and_preserves_history() {
    for delete_file in [false, true] {
        let home = TempHome::new();
        let mut lock = home.lock();
        let (mut folio, copy, onto, _) = fixture(&home, &mut lock, true);
        start(&home, &mut lock, &mut folio, &copy, onto);
        let row = current(&folio, &copy);
        let bytes = std::fs::read(copy.path.to_path_buf()).unwrap();
        let internal = folio.internal();
        let directory = folio.folio.path().to_path_buf();
        let objects = stored_files(&directory);
        let refs: Vec<_> = [RefKind::Branch, RefKind::Tag, RefKind::Pin]
            .into_iter()
            .map(|kind| folio.refs.list(kind).unwrap())
            .collect();
        remove_copy(&home, &lock, &mut folio, &row, delete_file);
        assert!(folio.registry.placement_at(&copy.path).unwrap().is_none());
        assert!(folio.registry.stopped_rebase(copy.id).unwrap().is_none());
        assert_eq!(copy.path.to_path_buf().exists(), !delete_file);
        if !delete_file {
            assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), bytes);
        }
        assert_eq!(folio.internal(), internal);
        assert_eq!(stored_files(&directory), objects);
        for (kind, before) in [RefKind::Branch, RefKind::Tag, RefKind::Pin]
            .into_iter()
            .zip(refs)
        {
            assert_eq!(folio.refs.list(kind).unwrap(), before);
        }
    }
}

#[test]
fn removing_stopped_copy_allows_another_rebase() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
    start(&home, &mut lock, &mut folio, &copy, onto);
    let row = current(&folio, &copy);
    remove_copy(&home, &lock, &mut folio, &row, false);
    let kept = home.root().join("removed.txt");
    std::fs::rename(copy.path.to_path_buf(), &kept).unwrap();
    let mut next = folio.copy(&home, &lock);
    let internal = folio.internal();
    let tip = internal.commit_id.unwrap();
    next.base = PlacementFs::new(&home.home)
        .refresh(
            next.id,
            &next.to_placement().1,
            &folio.objects,
            folio.objects.get_commit(tip, 16384).unwrap().content,
            None,
        )
        .unwrap()
        .base;
    next.commit_id = Some(tip);
    folio
        .registry
        .write(&lock, |tx| tx.update_placement(&next))
        .unwrap();
    start(&home, &mut lock, &mut folio, &next, onto);
    let progress = folio.registry.stopped_rebase(next.id).unwrap().unwrap();
    assert_eq!(progress.copy, next.path);
    assert!(current(&folio, &next).conflict.is_some());
    assert!(kept.exists());
}

#[test]
fn removing_stopped_copy_allows_folio_removal() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, copy, onto, _) = fixture(&home, &mut lock, false);
    start(&home, &mut lock, &mut folio, &copy, onto);
    let row = current(&folio, &copy);
    remove_copy(&home, &lock, &mut folio, &row, false);
    let internal = folio.internal();
    folio
        .registry
        .write(&lock, |tx| tx.remove_folio(copy.id, &mut |_, _| Ok(false)))
        .unwrap();
    assert!(!folio.registry.folio_exists(copy.id).unwrap());
    assert!(
        folio
            .registry
            .placement_at(&internal.path)
            .unwrap()
            .is_none()
    );
    assert!(copy.path.to_path_buf().exists());
}

#[test]
fn removing_another_copy_preserves_initiating_copy_and_progress() {
    let home = TempHome::new();
    let mut lock = home.lock();
    let (mut folio, mut copy, onto, _) = fixture(&home, &mut lock, false);
    let path = home.root().join("initiating.txt");
    std::fs::rename(copy.path.to_path_buf(), &path).unwrap();
    let path = PlacementPath::resolve(path).unwrap();
    folio
        .registry
        .write(&lock, |tx| tx.move_placement(&copy.path, &path))
        .unwrap();
    copy.path = path;
    let other = folio.copy(&home, &lock);
    start(&home, &mut lock, &mut folio, &copy, onto);
    let before = current(&folio, &copy);
    let progress = folio.registry.stopped_rebase(copy.id).unwrap();
    let bytes = std::fs::read(copy.path.to_path_buf()).unwrap();
    remove_copy(&home, &lock, &mut folio, &other, true);
    assert!(folio.registry.placement_at(&other.path).unwrap().is_none());
    assert!(!other.path.to_path_buf().exists());
    assert_eq!(current(&folio, &copy), before);
    assert_eq!(folio.registry.stopped_rebase(copy.id).unwrap(), progress);
    assert_eq!(std::fs::read(copy.path.to_path_buf()).unwrap(), bytes);
}
