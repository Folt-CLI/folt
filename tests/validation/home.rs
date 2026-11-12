use folt_core::config::env::Environment;
use folt_core::config::settings::Settings;
use folt_core::home::{Home, HomeError};

use crate::common::TempHome;

#[cfg(unix)]
#[test]
fn created_home_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempHome::new();
    assert_eq!(
        std::fs::metadata(home.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn lock_is_exclusive() {
    let home = TempHome::new();
    let _lock = home.lock();
    assert!(matches!(
        home.home.try_lock(),
        Err(HomeError::Locked { .. })
    ));
}

#[test]
fn dropping_guard_releases_lock() {
    let home = TempHome::new();
    drop(home.lock());
    let reopened =
        Home::open_or_create(home.path().to_path_buf(), &Environment::default()).unwrap();
    assert!(reopened.try_lock().is_ok());
}

#[cfg(unix)]
#[test]
fn writable_by_others_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempHome::new();
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o722)).unwrap();
    assert!(matches!(
        Home::open_or_create(home.path().to_path_buf(), &Environment::default()),
        Err(HomeError::WritableByOthers { .. })
    ));
}

#[test]
fn reads_config_from_private_home() {
    let home = TempHome::new();
    assert_eq!(Settings::load(&home.home).unwrap(), Settings::default());
    std::fs::write(
        home.path().join("config.toml"),
        "username = 'Alice'\nemail = 'a@example.test'\n",
    )
    .unwrap();
    assert_eq!(
        Settings::load(&home.home)
            .unwrap()
            .author()
            .unwrap()
            .to_string(),
        "Alice <a@example.test>"
    );
}

#[test]
fn environment_override_changes_large_file_threshold() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("home");
    let environment = Environment {
        large_file_bytes: Some("4".into()),
        ..Environment::default()
    };
    let home = Home::open_or_create(path.clone(), &environment).unwrap();
    assert_eq!(Settings::load(&home).unwrap().large_file_bytes, 4);
    std::fs::write(path.join("config.toml"), "large_file_bytes = 9\n").unwrap();
    assert_eq!(Settings::load(&home).unwrap().large_file_bytes, 4);
    let home = Home::open_or_create(path, &Environment::default()).unwrap();
    assert_eq!(Settings::load(&home).unwrap().large_file_bytes, 9);
}
