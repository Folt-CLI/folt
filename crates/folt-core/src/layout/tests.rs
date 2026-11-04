use super::*;

#[test]
fn canonical_version_round_trip() {
    let version: LayoutVersion = "12.3.45".parse().unwrap();
    assert_eq!(version, LayoutVersion::new(12, 3, 45));
    assert_eq!(version.to_string(), "12.3.45");
}

#[test]
fn rejects_noncanonical_versions() {
    for value in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.-2.3",
        "1.2.3 ",
        "1..3",
        "18446744073709551616.0.0",
    ] {
        assert!(value.parse::<LayoutVersion>().is_err(), "{value}");
    }
}

#[test]
fn ordering_is_numeric() {
    assert!(LayoutVersion::new(1, 10, 0) > LayoutVersion::new(1, 2, 99));
    assert!(LayoutVersion::new(2, 0, 0) > LayoutVersion::new(1, 99, 99));
    assert!(LayoutVersion::new(1, 2, 3) > LayoutVersion::new(1, 2, 2));
}
