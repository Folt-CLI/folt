use super::*;

#[test]
fn folio_id_parses_only_the_lowercase_hyphenated_form() {
    let canonical = "12345678-1234-4234-8234-123456789abc";
    assert_eq!(FolioId::parse(canonical).unwrap().to_string(), canonical);
    for value in [
        canonical.to_uppercase(),
        format!("{{{canonical}}}"),
        canonical.replace('-', ""),
        format!("urn:uuid:{canonical}"),
        String::new(),
    ] {
        let error = FolioId::parse(&value).unwrap_err();
        assert!(matches!(&error, InvalidFolioId(input) if input == &value));
        assert_eq!(error.to_string(), format!("invalid Folio id: {value}"));
    }
}

#[test]
fn folio_id_round_trips_bytes_and_text() {
    let id = FolioId::generate();
    assert_eq!(FolioId::from_bytes(*id.as_bytes()), id);
    assert_eq!(FolioId::parse(&id.to_string()).unwrap(), id);
}

#[test]
fn object_id_parses_exactly_64_lowercase_hex_digits() {
    let canonical = "0123456789abcdef".repeat(4);
    assert_eq!(
        canonical.parse::<ObjectId>().unwrap().to_string(),
        canonical
    );
    for value in [
        "a".repeat(63),
        "a".repeat(65),
        canonical.to_uppercase(),
        "g".repeat(64),
    ] {
        let error = value.parse::<ObjectId>().unwrap_err();
        assert!(matches!(error, InvalidObjectId));
        assert_eq!(error.to_string(), "invalid object hash");
    }
}

#[test]
fn object_id_round_trips_bytes_and_text() {
    let bytes = std::array::from_fn(|index| index as u8);
    let id = ObjectId::from_bytes(bytes);
    assert_eq!(id.as_bytes(), &bytes);
    assert_eq!(id.to_string().parse::<ObjectId>().unwrap(), id);
}
