use super::*;

#[test]
fn absent_username_uses_anonymous_even_with_email() {
    for email in [None, Some("a@example.test".into())] {
        assert_eq!(
            Config {
                username: None,
                email,
                ..Config::default()
            }
            .author()
            .unwrap(),
            Author::anonymous()
        );
    }
}

#[test]
fn username_only_has_no_email() {
    let config = Config {
        username: Some("Alice".into()),
        email: None,
        ..Config::default()
    };
    assert_eq!(config.author().unwrap().to_string(), "Alice");
}

#[test]
fn configured_identity_uses_both_fields() {
    let config = Config {
        username: Some("Alice".into()),
        email: Some("a@example.test".into()),
        ..Config::default()
    };
    assert_eq!(
        config.author().unwrap().to_string(),
        "Alice <a@example.test>"
    );
}

#[test]
fn invalid_identity_is_not_silently_replaced() {
    for username in ["<alice>", "ali\nce", ""] {
        assert!(
            Config {
                username: Some(username.into()),
                email: None,
                ..Config::default()
            }
            .author()
            .is_err()
        );
    }
    assert!(
        Config {
            username: None,
            email: Some("invalid".into()),
            ..Config::default()
        }
        .author()
        .is_err()
    );
}
