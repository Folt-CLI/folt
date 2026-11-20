use super::super::keys::{decoded_key, edit_key_checks, explicit_key, quoted_spelling, top_level};
use super::*;

#[test]
fn explicit_key_requires_yaml_separation_and_preserves_the_tail() {
    for (spelling, expected) in [
        ("?", Some("")),
        ("? name", Some(" name")),
        ("?\tname", Some("\tname")),
        ("?  'name' ", Some("  'name' ")),
        ("?name", None),
        ("?? name", None),
        (" ? name", None),
        ("?\nname", None),
        ("", None),
        ("name", None),
    ] {
        assert_eq!(explicit_key(spelling), expected, "{spelling:?}");
    }
}

#[test]
fn top_level_classifies_physical_key_lines() {
    for (raw, expected) in [
        ("name: value", true),
        ("? name", true),
        ("'name': value", true),
        ("-name: value", true),
        ("-", true),
        ("-\tname", true),
        ("", false),
        (" name: value", false),
        ("\tname: value", false),
        ("# name: value", false),
        ("- name: value", false),
    ] {
        assert_eq!(top_level(raw), expected, "{raw:?}");
    }
}

#[test]
fn decoded_key_decodes_scalars_and_retains_raw_spelling_on_failure() {
    for (spelling, expected) in [
        (" name ", "name"),
        ("'it''s'", "it's"),
        (r#""line\nname""#, "line\nname"),
        ("\"\u{e9}\"", "\u{e9}"),
        ("", ""),
        (" 'unterminated ", " 'unterminated "),
        (r#""\u0066olio-name""#, r#""\u0066olio-name""#),
        (" !!str name ", " !!str name "),
        ("? name", "? name"),
    ] {
        assert_eq!(decoded_key(spelling), expected, "{spelling:?}");
    }
}

#[test]
fn quoted_spelling_checks_the_first_character_without_trimming() {
    for (spelling, expected) in [
        ("'name'", true),
        ("\"name\"", true),
        ("'unterminated", true),
        ("\"", true),
        (" 'name'", false),
        ("? 'name'", false),
        ("name'", false),
        ("", false),
    ] {
        assert_eq!(quoted_spelling(spelling), expected, "{spelling:?}");
    }
}

#[test]
fn edit_key_checks_preserves_key_acceptance_and_ambiguity_errors() {
    for (body, expected) in [
        ("", Ok(())),
        ("other: value\n", Ok(())),
        ("folio-name: old\n", Ok(())),
        ("'other': value\n", Ok(())),
        ("\"other\": value\n", Ok(())),
        ("'folio-name': old\n", Ok(())),
        ("? other: value\n", Ok(())),
        ("?\n  other\n: value\n", Ok(())),
        ("? \"other\\nkey\": value\n", Ok(())),
        ("? 'other\\key': value\n", Ok(())),
        ("\"other\\nkey\": value\n", Err(FrontMatterError::Ambiguous)),
        ("'other\\key': value\n", Err(FrontMatterError::Ambiguous)),
        (
            "\"\\u0066olio-name\": old\n",
            Err(FrontMatterError::Ambiguous),
        ),
        ("'unterminated: value\n", Ok(())),
        ("!!str other: value\n", Err(FrontMatterError::Ambiguous)),
        ("&a other: value\n", Err(FrontMatterError::Ambiguous)),
        ("*a: value\n", Err(FrontMatterError::Ambiguous)),
        ("? folio-name: old\n", Err(FrontMatterError::Ambiguous)),
        ("?\n  folio-name\n: old\n", Err(FrontMatterError::Ambiguous)),
        ("? !!str other: value\n", Err(FrontMatterError::Ambiguous)),
    ] {
        let input = format!("---\n{body}---\n");
        let bytes = input.as_bytes();
        let block = locate(bytes).unwrap().unwrap();
        assert_eq!(edit_key_checks(bytes, &block), expected, "{body:?}");
    }
}
