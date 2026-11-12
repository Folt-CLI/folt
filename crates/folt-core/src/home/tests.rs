use super::*;

#[test]
fn override_that_is_absolute_wins() {
    let folt_home = std::env::temp_dir().join("folt-home");
    for platform_home in [None, Some(std::env::temp_dir().join("platform-home"))] {
        let environment = Environment {
            folt_home: Some(folt_home.clone().into_os_string()),
            platform_home,
            ..Environment::default()
        };
        assert_eq!(resolve_home(&environment).unwrap(), folt_home);
    }
}

#[test]
fn empty_override_uses_the_platform_home() {
    let platform_home = std::env::temp_dir().join("platform-home");
    let environment = Environment {
        folt_home: Some("".into()),
        platform_home: Some(platform_home.clone()),
        ..Environment::default()
    };
    assert_eq!(
        resolve_home(&environment).unwrap(),
        platform_home.join(HOME_DIR_NAME)
    );
}

#[test]
fn relative_override_is_refused() {
    let environment = Environment {
        folt_home: Some("relative".into()),
        platform_home: Some(std::env::temp_dir().join("platform-home")),
        ..Environment::default()
    };
    assert!(matches!(
        resolve_home(&environment),
        Err(HomeError::InvalidFoltHome(_))
    ));
}

#[test]
fn missing_override_and_platform_home_is_refused() {
    assert!(matches!(
        resolve_home(&Environment::default()),
        Err(HomeError::NoHomeDirectory)
    ));
}

#[test]
fn open_keeps_the_validated_busy_timeout() {
    use crate::config::limits::DEFAULT_BUSY_TIMEOUT;
    use crate::test_support::TempDir;

    let directory = TempDir::new().unwrap();
    let environment = Environment {
        busy_timeout_ms: Some("250".into()),
        ..Environment::default()
    };
    let home = Home::open_or_create(directory.path().join("override"), &environment).unwrap();
    assert_eq!(home.busy_timeout(), Duration::from_millis(250));
    let home =
        Home::open_or_create(directory.path().join("default"), &Environment::default()).unwrap();
    assert_eq!(home.busy_timeout(), DEFAULT_BUSY_TIMEOUT);
}

#[test]
fn invalid_tuning_value_refuses_to_open() {
    use crate::test_support::TempDir;

    let directory = TempDir::new().unwrap();
    let path = directory.path().join("missing-parent").join("home");
    let environment = Environment {
        busy_timeout_ms: Some("invalid".into()),
        ..Environment::default()
    };
    assert!(matches!(
        Home::open_or_create(path.clone(), &environment),
        Err(HomeError::InvalidSettings(_))
    ));
    assert!(!path.parent().unwrap().exists());
}
