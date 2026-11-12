use super::*;

fn assert_ambiguous_key(bytes: &[u8]) {
    let error = FrontMatterError::Ambiguous;
    assert_eq!(read_name(bytes), Err(error));
    assert_eq!(read_id(bytes), Err(error));
    assert_eq!(read_references(bytes), Err(error));
    for name in [Some("new"), None] {
        assert_eq!(set_name(bytes, name, false), Err(error));
    }
    for id in [Some(FolioId::parse(ID).unwrap()), None] {
        assert_eq!(set_id(bytes, id, false), Err(error));
    }
    assert_eq!(set_references(bytes, &[logo()], false), Err(error));
    assert_eq!(set_references(bytes, &[], false), Err(error));
    assert_eq!(
        set_reference(bytes, "logo", Some(&logo()), false),
        Err(error)
    );
    assert_eq!(set_reference(bytes, "logo", None, false), Err(error));
}

#[test]
fn owned_or_unresolved_null_explicit_keys_remain_ambiguous() {
    for ending in ["\n", "\r\n"] {
        for key in [
            "folio-name",
            "folio-id",
            "folio-references",
            "!!str other",
            "&a other",
            "*a",
            "[other]",
            "\"unterminated",
        ] {
            for foreign in [format!("? {key}\n"), format!("?\n  {key}\n")] {
                let input = format!("---\n{foreign}folio-name: old\n---\n").replace('\n', ending);
                assert_ambiguous_key(input.as_bytes());
            }
        }
        assert_ambiguous_key(format!("---{ending}? folio-name{ending}---{ending}").as_bytes());
    }
}

#[test]
fn refuses_owned_or_unresolved_multiline_explicit_keys() {
    for ending in ["\n", "\r\n"] {
        for key in OWNED {
            for spelling in [key.to_owned(), format!("'{key}'"), format!("\"{key}\"")] {
                let bytes =
                    format!("---{ending}?{ending}  {spelling}{ending}: old{ending}---{ending}");
                assert_ambiguous_key(bytes.as_bytes());
                let bytes = format!(
                    "---{ending}? {spelling}{ending}{ending}# key comment{ending}{key}: old{ending}---{ending}"
                );
                assert_ambiguous_key(bytes.as_bytes());
            }
        }
        for body in [
            "?\n: old\n",
            "?\n",
            "?\n  !!str other\n: old\n",
            "?\n  &a other\n: old\n",
            "?\n  *a\n: old\n",
            "?\n  \"\\u0066olio-name\"\n: old\n",
            "?\n  \"folio-\n  name\"\n: old\n",
        ] {
            let bytes = format!("---\n{body}---\n").replace('\n', ending);
            assert_ambiguous_key(bytes.as_bytes());
        }
    }
}

#[test]
fn explicit_keys_require_fully_decoded_foreign_scalars() {
    for ending in ["\n", "\r\n"] {
        for key in OWNED {
            for spelling in [
                key.to_owned(),
                format!("'{key}'"),
                format!("\"{key}\""),
                format!("|-\n  {key}"),
                format!(">-\n  {key}"),
                format!("[{key}]"),
                format!("{{{key}: other}}"),
                format!("!!str {key}"),
                format!("&a {key}"),
                format!("*{key}"),
                format!("'other' '{key}'"),
                format!("\"other\" \"{key}\""),
            ] {
                for body in [
                    format!("? {spelling}\n: old\n"),
                    format!("? {spelling}\n# before value\n\n: old\n"),
                    format!("?\n  {spelling}\n: old\n"),
                    format!("? # key\n\n# before key\n  {spelling}\n\n# before value\n: old\n"),
                ] {
                    let input = format!("---\n{body}---\n").replace('\n', ending);
                    assert_ambiguous_key(input.as_bytes());
                }
                // Also exercise the inline value indicator for single-line keys.
                if !spelling.contains('\n') {
                    let input = format!("---\n? {spelling}: old\n---\n").replace('\n', ending);
                    assert_ambiguous_key(input.as_bytes());
                }
            }
        }
        for spelling in ["[other]", "{other: value}", "|-\n  other", ">-\n  other"] {
            let input = format!("---\n? {spelling}\n: old\n---\n").replace('\n', ending);
            assert_ambiguous_key(input.as_bytes());
        }
    }
}

#[test]
fn refuses_key_properties_before_owned_key_lookup() {
    for ending in ["\n", "\r\n"] {
        for key in OWNED {
            for property in ["!!str", "!custom", "&a", "*a"] {
                for spelling in [key.to_owned(), format!("'{key}'"), format!("\"{key}\"")] {
                    for body in [
                        format!("? {property} {spelling}\n: old\n"),
                        format!("? {property} {spelling}: old\n"),
                        format!("{property} {spelling}: old\n"),
                        format!("?\n  {property} {spelling}\n: old\n"),
                    ] {
                        let bytes = format!("---\n{body}---\n").replace('\n', ending);
                        assert_ambiguous_key(bytes.as_bytes());
                    }
                }
            }
        }
    }
}

#[test]
fn refuses_unsafe_owned_nodes() {
    for (body, error) in [
        (
            "folio-name: a\nfolio-name: b\n",
            FrontMatterError::Duplicate,
        ),
        ("\"folio-name\": a\n", FrontMatterError::Ambiguous),
        ("'folio-name': a\n", FrontMatterError::Ambiguous),
        ("? folio-name\n: a\n", FrontMatterError::Ambiguous),
        ("? 'folio-name'\n: a\n", FrontMatterError::Ambiguous),
        ("? folio-name: a\n", FrontMatterError::Ambiguous),
        ("folio-name: &anchor a\n", FrontMatterError::Unsupported),
        ("folio-name: *anchor\n", FrontMatterError::Unsupported),
        ("folio-name: !!str a\n", FrontMatterError::Unsupported),
        (
            "folio-name: a\n\tcontinued\n",
            FrontMatterError::Unsupported,
        ),
        (
            "folio-name: a\n  continued\n",
            FrontMatterError::Unsupported,
        ),
    ] {
        let bytes = format!("---\n{body}---\n");
        assert_eq!(read_name(bytes.as_bytes()), Err(error), "{body}");
        assert_eq!(
            set_name(bytes.as_bytes(), Some("new"), true),
            Err(error),
            "{body}"
        );
    }
    for key in OWNED {
        let bytes = format!("---\n{key}: a\n{key}: b\n---\n");
        assert_eq!(
            set_name(bytes.as_bytes(), Some("new"), true),
            Err(FrontMatterError::Duplicate)
        );
    }
    assert_eq!(
        set_name(b"---\nno close", Some("new"), true),
        Err(FrontMatterError::Unterminated)
    );
    assert_eq!(
        set_name(b"body\0", Some("new"), true),
        Err(FrontMatterError::Binary)
    );
}

#[test]
fn refuses_escaped_or_undecodable_quoted_keys_before_insertion() {
    for key in [
        r#""\u0066olio-name""#,
        r#""\x66olio-name""#,
        r#""\U00000066olio-name""#,
        r#""\qolio-name""#,
        "'folio-name'",
        "\"folio-name\"",
    ] {
        for prefix in ["", "? "] {
            let bytes = format!("---\n{prefix}{key}: old\n---\n");
            assert_eq!(
                set_name(bytes.as_bytes(), Some("new"), false),
                Err(FrontMatterError::Ambiguous),
                "{bytes}"
            );
        }
    }
}

#[test]
fn explicit_owned_keys_with_comments_are_ambiguous() {
    let bytes = b"---\n? folio-name # keep\n: old\n---\n";
    assert_eq!(read_name(bytes), Err(FrontMatterError::Ambiguous));
    assert_eq!(
        set_name(bytes, Some("new"), false),
        Err(FrontMatterError::Ambiguous)
    );
}

#[test]
fn reads_refuse_unresolved_owned_keys() {
    for ending in ["\n", "\r\n"] {
        for key in OWNED {
            for body in [
                format!("?{ending}  {key}{ending}: old"),
                format!("? # keep{ending}  {key}{ending}: old"),
                format!(r#""\u0066{}": old"#, &key[1..]),
                format!(r#"? "\u0066{}": old"#, &key[1..]),
            ] {
                let input = format!("---{ending}{body}{ending}---{ending}");
                let bytes = input.as_bytes();
                assert_eq!(
                    read_name(bytes),
                    Err(FrontMatterError::Ambiguous),
                    "{input}"
                );
                assert_eq!(read_id(bytes), Err(FrontMatterError::Ambiguous), "{input}");
                assert_eq!(
                    read_references(bytes),
                    Err(FrontMatterError::Ambiguous),
                    "{input}"
                );
            }
        }
    }
}

#[test]
fn explicit_owned_keys_with_yaml_separation_refuse_edits() {
    let id = FolioId::parse(ID).unwrap();
    for key in OWNED {
        for prefix in ["? ", "?\t", "?\n  ", "?\r\n  "] {
            let bytes = format!("---\n{prefix}{key} # keep\n: old\n---\n");
            let bytes = bytes.as_bytes();
            assert_eq!(
                set_name(bytes, Some("new"), false),
                Err(FrontMatterError::Ambiguous)
            );
            assert_eq!(
                set_id(bytes, Some(id), false),
                Err(FrontMatterError::Ambiguous)
            );
            assert_eq!(
                set_references(bytes, &[logo()], false),
                Err(FrontMatterError::Ambiguous)
            );
            assert_eq!(
                set_reference(bytes, "logo", Some(&logo()), false),
                Err(FrontMatterError::Ambiguous)
            );
        }
    }
}
