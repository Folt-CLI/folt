use super::*;
use crate::test_support::TempHome;

#[test]
fn rewrite_publication_signature_matches_renamed_file() {
    let home = TempHome::new().unwrap();
    let path = home.root.join("file.md");
    fs::write(&path, b"body").unwrap();
    let expected = signature(&fs::metadata(&path).unwrap());
    let mut published = None;
    atomic_rewrite(&path, b"rewritten body", &expected, |value| {
        published = Some(value);
    })
    .unwrap();
    assert_eq!(published, Some(signature(&fs::metadata(&path).unwrap())));
}

#[cfg(unix)]
#[test]
fn rewrite_temp_is_private_before_writing_and_rewrite_preserves_mode() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempHome::new().unwrap();
    let source = home.root.join("private.md");
    fs::write(&source, b"private content\n").unwrap();
    let mode = 0o600;
    fs::set_permissions(&source, fs::Permissions::from_mode(mode)).unwrap();
    let metadata = fs::metadata(&source).unwrap();
    let temporary = home.root.join(temp_name(".folt-name-test-", '-'));
    let output = create_rewrite_temp(&temporary, &metadata.permissions()).unwrap();
    let created = output.metadata().unwrap();
    assert_eq!(created.len(), 0);
    assert_eq!(created.permissions().mode() & 0o7777, 0o600);
    drop(output);
    fs::remove_file(&temporary).unwrap();

    let rewritten = b"---\nfolio-name: \"private\"\n---\nprivate content\n";
    atomic_rewrite(&source, rewritten, &signature(&metadata), |_| {}).unwrap();
    assert_eq!(fs::read(&source).unwrap(), rewritten);
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o7777,
        0o600
    );
}

#[test]
fn rewrite_refuses_other_hard_links() {
    let home = TempHome::new().unwrap();
    let path = home.root.join("source");
    fs::write(&path, b"original").unwrap();
    fs::hard_link(&path, home.root.join("other")).unwrap();
    let expected = signature(&fs::metadata(&path).unwrap());
    let error = atomic_rewrite(&path, b"new", &expected, |_| panic!("published")).unwrap_err();
    assert_eq!(error.to_string(), "source has other hard links");
    assert_eq!(fs::read(path).unwrap(), b"original");
}
