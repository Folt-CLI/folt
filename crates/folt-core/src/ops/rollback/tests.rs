use super::*;
use crate::fsx::fault::FaultFileSystem;
use crate::fsx::{RealFileSystem, atomic_rewrite, signature};
use crate::test_support::TempHome;

fn rewrite(guard: &mut Rollback<'_>, path: &Path, bytes: &[u8]) {
    let original = fs::read(path).unwrap();
    let expected = signature(&fs::symlink_metadata(path).unwrap());
    atomic_rewrite(&RealFileSystem, path, bytes, &expected, |signature| {
        guard.record_rewrite(path.to_path_buf(), original, signature);
    })
    .unwrap();
}

#[test]
fn drop_restores_files_and_removes_rewritten_markers_but_keeps_directories() {
    let fixture = TempHome::new().unwrap();
    let file = fixture.root.join("file.md");
    let marker = fixture.root.join("__folio__.md");
    let directory = fixture.dir("folio-directory");
    fs::write(&file, b"original").unwrap();
    fs::write(&marker, b"").unwrap();
    {
        let mut guard = Rollback::new(&RealFileSystem);
        guard.record_marker(marker.clone(), signature(&fs::metadata(&marker).unwrap()));
        guard.record_directory(directory.clone());
        rewrite(&mut guard, &file, b"with name");
        rewrite(&mut guard, &marker, b"marker with name");
    }
    assert_eq!(fs::read(file).unwrap(), b"original");
    assert!(!marker.exists());
    assert!(directory.is_dir());
}

#[test]
fn explicit_failure_removes_directories_and_markers_and_restores_files() {
    let fixture = TempHome::new().unwrap();
    let file = fixture.root.join("file.md");
    let marker = fixture.root.join("__folio__.md");
    let directory = fixture.dir("folio-directory");
    fs::write(&file, b"original").unwrap();
    fs::write(&marker, b"").unwrap();
    let mut guard = Rollback::new(&RealFileSystem);
    guard.record_marker(marker.clone(), signature(&fs::metadata(&marker).unwrap()));
    guard.record_directory(directory.clone());
    rewrite(&mut guard, &file, b"with name");
    rewrite(&mut guard, &marker, b"marker with name");
    assert!(guard.fail().is_empty());
    assert_eq!(fs::read(file).unwrap(), b"original");
    assert!(!marker.exists());
    assert!(!directory.exists());
}

#[test]
fn committed_guard_preserves_all_writes_and_artifacts() {
    let fixture = TempHome::new().unwrap();
    let file = fixture.root.join("file.md");
    let marker = fixture.root.join("__folio__.md");
    let directory = fixture.dir("folio-directory");
    fs::write(&file, b"original").unwrap();
    fs::write(&marker, b"").unwrap();
    let mut guard = Rollback::new(&RealFileSystem);
    guard.record_marker(marker.clone(), signature(&fs::metadata(&marker).unwrap()));
    guard.record_directory(directory.clone());
    assert!(guard.owns_marker(&marker));
    assert!(!guard.owns_marker(&file));
    rewrite(&mut guard, &file, b"with name");
    rewrite(&mut guard, &marker, b"marker with name");
    guard.commit();
    assert_eq!(fs::read(file).unwrap(), b"with name");
    assert_eq!(fs::read(marker).unwrap(), b"marker with name");
    assert!(directory.is_dir());
}

#[test]
fn replaced_marker_is_preserved_on_drop_and_explicit_failure() {
    for explicit in [false, true] {
        let fixture = TempHome::new().unwrap();
        let marker = fixture.root.join("__folio__.md");
        fs::write(&marker, b"").unwrap();
        let mut guard = Rollback::new(&RealFileSystem);
        guard.record_marker(marker.clone(), signature(&fs::metadata(&marker).unwrap()));
        rewrite(&mut guard, &marker, b"marker with name");
        let replacement = fixture.root.join("replacement");
        fs::write(&replacement, b"external replacement").unwrap();
        fs::remove_file(&marker).unwrap();
        fs::rename(replacement, &marker).unwrap();
        if explicit {
            assert_eq!(
                guard.fail(),
                vec![format!(
                    "{} keeps the name: file changed after our write",
                    marker.display()
                )]
            );
        } else {
            drop(guard);
        }
        assert_eq!(fs::read(marker).unwrap(), b"external replacement");
    }
}

#[test]
fn failed_restores_are_reported_in_reverse_write_order_and_cleanup_continues() {
    let fixture = TempHome::new().unwrap();
    let paths = [
        fixture.root.join("first.md"),
        fixture.root.join("second.md"),
    ];
    let directory = fixture.dir("folio-directory");
    let marker = fixture.root.join("__folio__.md");
    fs::write(&marker, b"").unwrap();
    let mut guard = Rollback::new(&RealFileSystem);
    guard.record_marker(marker.clone(), signature(&fs::metadata(&marker).unwrap()));
    guard.record_directory(directory.clone());
    for path in &paths {
        fs::write(path, b"original").unwrap();
        rewrite(&mut guard, path, b"with name");
        fs::write(path, b"external edit").unwrap();
    }
    assert_eq!(
        guard.fail(),
        paths
            .iter()
            .rev()
            .map(|path| format!(
                "{} keeps the name: file changed after our write",
                path.display()
            ))
            .collect::<Vec<_>>()
    );
    for path in paths {
        assert_eq!(fs::read(path).unwrap(), b"external edit");
    }
    assert!(!directory.exists());
    assert!(!marker.exists());
}

#[test]
fn restore_sync_failure_reports_original_bytes_and_still_removes_marker() {
    let fixture = TempHome::new().unwrap();
    let file = fixture.root.join("file.md");
    let marker = fixture.root.join("__folio__.md");
    fs::write(&file, b"original").unwrap();
    fs::write(&marker, b"").unwrap();
    let fs = FaultFileSystem::new().sync_failure(&fixture.root, 0);
    let mut guard = Rollback::new(&fs);
    guard.record_marker(marker.clone(), signature(&fs::metadata(&marker).unwrap()));
    rewrite(&mut guard, &file, b"with name");
    rewrite(&mut guard, &marker, b"marker with name");
    let failures = guard.fail();
    assert_eq!(failures.len(), 2);
    for (failure, path) in failures.iter().zip([&marker, &file]) {
        assert_eq!(
            failure,
            &format!(
                "{}: restoration completed but the directory sync failed; original bytes are back, durability of the restore is not confirmed: injected directory sync failure",
                path.display()
            )
        );
    }
    assert_eq!(fs::read(file).unwrap(), b"original");
    assert!(!marker.exists());
}

#[test]
fn restore_failure_before_publication_reports_retained_name() {
    let fixture = TempHome::new().unwrap();
    let file = fixture.root.join("file.md");
    fs::write(&file, b"original").unwrap();
    let mut permissions = fs::metadata(&file).unwrap().permissions();
    let original_permissions = permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&file, permissions).unwrap();
    let mut guard = Rollback::new(&RealFileSystem);
    guard.record_rewrite(
        file.clone(),
        b"before write".to_vec(),
        signature(&fs::metadata(&file).unwrap()),
    );
    let failures = guard.fail();
    fs::set_permissions(&file, original_permissions).unwrap();
    assert_eq!(
        failures,
        vec![format!(
            "{} keeps the name: restore failed: read-only file",
            file.display()
        )]
    );
    assert_eq!(fs::read(file).unwrap(), b"original");
}
