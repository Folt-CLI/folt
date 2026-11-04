use super::*;

#[test]
fn accepts_portable_names() {
    for name in ["main", "feature/one", "v1.2", "a_b-c", "0", "com0", "lpt10"] {
        assert_eq!(RefName::parse(name).unwrap().as_str(), name);
    }
}

#[test]
fn refuses_invalid_grammar() {
    for name in [
        "", "Main", "a b", "a..b", "/a", "a/", "a//b", ".a", "-a", "a.", "a.lock", "a/.b", "é",
        "a\\b",
    ] {
        assert!(RefName::parse(name).is_err(), "{name:?}");
    }
}

#[test]
fn refuses_device_names_in_any_component() {
    for name in [
        "con", "prn.txt", "aux", "nul", "com1", "com9.txt", "lpt1", "x/lpt9",
    ] {
        assert!(RefName::parse(name).is_err(), "{name}");
    }
}

#[test]
fn refuses_only_exactly_64_hex_digits() {
    assert!(RefName::parse(&"a".repeat(64)).is_err());
    for name in ["a".repeat(63), "a".repeat(65), "z".repeat(64)] {
        assert!(RefName::parse(&name).is_ok());
    }
}

#[test]
fn enforces_100_byte_limit() {
    assert!(RefName::parse(&"x".repeat(100)).is_ok());
    assert!(RefName::parse(&"x".repeat(101)).is_err());
}

#[test]
fn pin_names_round_trip_outside_user_grammar() {
    let parent = FolioId::from_bytes([1; 16]);
    let commit = crate::object::hash_blob(b"pin");
    let name = pin_name(parent, commit);
    assert_eq!(name.as_str().len(), 101);
    assert_eq!(name.pin_parts().unwrap(), (parent, commit));
    assert!(RefName::parse(name.as_str()).is_err());
    assert!(parse_pin("main").is_err());
}
