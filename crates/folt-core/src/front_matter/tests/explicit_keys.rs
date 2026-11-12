use super::*;

#[test]
fn foreign_null_explicit_keys_remain_readable_and_untouched() {
    for ending in ["\n", "\r\n"] {
        for foreign in ["? other\n", "?\n  other\n"] {
            let foreign = foreign.replace('\n', ending);
            let prefix = format!("---{ending}{foreign}");
            for (key, value) in [
                ("folio-name", "old".to_owned()),
                ("folio-id", ID.to_owned()),
                ("folio-references", "{logo: {name: old-image}}".to_owned()),
            ] {
                let input = format!("{prefix}{key}: {value}{ending}---{ending}body");
                let bytes = input.as_bytes();
                let id = FolioId::parse("12345678-1234-4234-8234-123456789abd").unwrap();
                let edits = match key {
                    "folio-name" => {
                        assert_eq!(read_name(bytes).unwrap().as_deref(), Some("old"));
                        [
                            set_name(bytes, Some("new"), false),
                            set_name(bytes, None, false),
                        ]
                    }
                    "folio-id" => {
                        assert_eq!(read_id(bytes).unwrap(), Some(FolioId::parse(ID).unwrap()));
                        [set_id(bytes, Some(id), false), set_id(bytes, None, false)]
                    }
                    _ => {
                        let refs = read_references(bytes).unwrap();
                        assert_eq!(refs.len(), 1);
                        assert_eq!(refs[0].alias, "logo");
                        assert_eq!(refs[0].name.as_deref(), Some("old-image"));
                        [
                            set_references(bytes, &[logo()], false),
                            set_references(bytes, &[], false),
                        ]
                    }
                };
                for (index, edit) in edits.into_iter().enumerate() {
                    let edited = apply_edit(bytes, &edit.unwrap().unwrap()).unwrap();
                    assert!(edited.starts_with(prefix.as_bytes()));
                    let removed = index == 1;
                    match key {
                        "folio-name" => assert_eq!(
                            read_name(&edited).unwrap().as_deref(),
                            if removed { None } else { Some("new") }
                        ),
                        "folio-id" => assert_eq!(
                            read_id(&edited).unwrap(),
                            if removed { None } else { Some(id) }
                        ),
                        _ => assert_eq!(
                            read_references(&edited).unwrap(),
                            if removed { vec![] } else { vec![logo()] }
                        ),
                    }
                }
            }
        }
    }
}

#[test]
fn foreign_multiline_explicit_keys_remain_readable_and_untouched() {
    for ending in ["\n", "\r\n"] {
        for key in [
            "other",
            "'other'",
            "\"other\"",
            "\"other\\nkey\"",
            r"'\u0066olio-name'",
            "'other''key'",
            "other\n  key",
            "folio-name\n  suffix",
            "folio-id\n  suffix",
            "folio-references\n  suffix",
        ] {
            for foreign in [
                format!("? {key}\n"),
                format!("?\n  {key}\n"),
                format!("? {key}\n: value\n"),
                format!("?\n  {key}\n: value\n"),
                format!("? {key} # key\n\n# before value\n: value\n"),
                format!("? # key\n\n# before key\n  {key}\n\n# before value\n: value\n"),
            ] {
                let foreign = foreign.replace('\n', ending);
                let bytes = format!(
                    "---{ending}{foreign}folio-name: name{ending}folio-id: {ID}{ending}folio-references: {{logo: {{name: logo-image, pin: abc123}}}}{ending}---{ending}body"
                );
                let bytes = bytes.as_bytes();
                assert_eq!(read_name(bytes).unwrap().as_deref(), Some("name"));
                assert_eq!(read_id(bytes).unwrap(), Some(FolioId::parse(ID).unwrap()));
                assert_eq!(read_references(bytes).unwrap(), vec![logo()]);
                let edits = [
                    set_name(bytes, Some("new"), false),
                    set_name(bytes, None, false),
                    set_id(bytes, Some(FolioId::parse(ID).unwrap()), false),
                    set_id(bytes, None, false),
                    set_references(bytes, &[logo()], false),
                    set_references(bytes, &[], false),
                    set_reference(bytes, "logo", Some(&logo()), false),
                    set_reference(bytes, "logo", None, false),
                ];
                for edit in edits {
                    let edited = apply_edit(bytes, &edit.unwrap().unwrap()).unwrap();
                    assert!(edited.starts_with(format!("---{ending}{foreign}").as_bytes()));
                }
                assert_eq!(
                    rename(bytes, "new"),
                    text(bytes)
                        .unwrap()
                        .replace("folio-name: name", "folio-name: \"new\"")
                        .as_bytes()
                );
            }
        }
        // Ordinary foreign keys, including quoted property characters,
        // remain opaque and retain their spelling during edits.
        let bytes = format!(
            "---{ending}other: x{ending}'&a folio-name': x{ending}\"!!str folio-name\": x{ending}folio-name: name{ending}---{ending}"
        );
        assert_eq!(
            read_name(bytes.as_bytes()).unwrap().as_deref(),
            Some("name")
        );
        assert_eq!(
            rename(bytes.as_bytes(), "new"),
            bytes
                .replace("folio-name: name", "folio-name: \"new\"")
                .as_bytes()
        );
    }
}

#[test]
fn implicit_escaped_foreign_keys_remain_opaque_for_edits() {
    let bytes = br"---
'\u0066olio-name': old
---
";
    assert_eq!(read_name(bytes).unwrap(), None);
    assert_eq!(
        set_name(bytes, Some("new"), false),
        Err(FrontMatterError::Ambiguous)
    );
}

#[test]
fn reads_allow_decodable_foreign_explicit_and_quoted_keys() {
    for ending in ["\n", "\r\n"] {
        for key in [
            "? other",
            "?\tother",
            "? 'other'",
            r#"? "other\nkey""#,
            r#""other\nkey""#,
        ] {
            for owned in [false, true] {
                let metadata = if owned {
                    format!(
                        "folio-name: name{ending}folio-id: {ID}{ending}folio-references: {{}}{ending}"
                    )
                } else {
                    String::new()
                };
                let input = format!("---{ending}{key}: value{ending}{metadata}---{ending}");
                let bytes = input.as_bytes();
                assert_eq!(
                    read_name(bytes).unwrap().as_deref(),
                    owned.then_some("name"),
                    "{input}"
                );
                assert_eq!(
                    read_id(bytes).unwrap(),
                    owned.then(|| FolioId::parse(ID).unwrap()),
                    "{input}"
                );
                assert!(read_references(bytes).unwrap().is_empty(), "{input}");
            }
        }
    }
}
