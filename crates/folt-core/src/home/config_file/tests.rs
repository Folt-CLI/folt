use super::*;

#[test]
fn absent_username_uses_anonymous_even_with_email() {
    for email in [None, Some("a@example.test".into())] {
        assert_eq!(
            Settings {
                username: None,
                email,
                ..Settings::default()
            }
            .author()
            .unwrap(),
            Author::anonymous()
        );
    }
}

#[test]
fn username_only_has_no_email() {
    let config = Settings {
        username: Some("Alice".into()),
        email: None,
        ..Settings::default()
    };
    assert_eq!(config.author().unwrap().to_string(), "Alice");
}

#[test]
fn configured_identity_uses_both_fields() {
    let config = Settings {
        username: Some("Alice".into()),
        email: Some("a@example.test".into()),
        ..Settings::default()
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
            Settings {
                username: Some(username.into()),
                email: None,
                ..Settings::default()
            }
            .author()
            .is_err()
        );
    }
    assert!(
        Settings {
            username: None,
            email: Some("invalid".into()),
            ..Settings::default()
        }
        .author()
        .is_err()
    );
}
