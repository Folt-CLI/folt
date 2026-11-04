use super::*;

#[test]
fn override_that_is_absolute_wins() {
    let folt_home = std::env::temp_dir().join("folt-home");
    for platform_home in [None, Some(std::env::temp_dir().join("platform-home"))] {
        let environment = Environment {
            folt_home: Some(folt_home.clone().into_os_string()),
            platform_home,
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
