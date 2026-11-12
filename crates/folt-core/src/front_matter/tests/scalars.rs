use super::*;

#[test]
fn names_are_double_quoted_in_existing_and_new_blocks() {
    for name in ["notes", "1234", "null", "true", "yes", "1e3", "0x10"] {
        for (input, expected) in [
            (
                "---\nforeign: keep\n---\nbody",
                format!("---\nforeign: keep\nfolio-name: \"{name}\"\n---\nbody"),
            ),
            ("body", format!("---\nfolio-name: \"{name}\"\n---\nbody")),
            (
                "---\r\nforeign: keep\r\n---\r\nbody\r\n",
                format!("---\r\nforeign: keep\r\nfolio-name: \"{name}\"\r\n---\r\nbody\r\n"),
            ),
        ] {
            let output = rename(input.as_bytes(), name);
            assert_eq!(output, expected.as_bytes());
            assert_eq!(read_name(&output).unwrap().as_deref(), Some(name));
        }
    }
}

#[test]
fn name_replacements_quote_plain_and_quoted_values_and_keep_endings() {
    for ending in ["\n", "\r\n"] {
        for old in ["old", "\"old\"", "'old'", "new", "\"new\"", "'new'"] {
            let input = format!("---{ending}folio-name: {old} # keep{ending}---{ending}body");
            let expected = format!("---{ending}folio-name: \"new\" # keep{ending}---{ending}body");
            let planned = set_name(input.as_bytes(), Some("new"), false)
                .unwrap()
                .expect("same values still produce an edit");
            let output = apply_edit(input.as_bytes(), &planned).unwrap();
            assert_eq!(output, expected.as_bytes());
            assert_eq!(rename(&output, "new"), output);
        }
    }
}

#[test]
fn ids_remain_unquoted_when_inserted_or_replaced() {
    let id = FolioId::parse(ID).unwrap();
    for input in [
        "body".to_owned(),
        "---\n---\nbody".to_owned(),
        format!("---\nfolio-id: '{ID}'\n---\nbody"),
    ] {
        let planned = set_id(input.as_bytes(), Some(id), true).unwrap().unwrap();
        let output = apply_edit(input.as_bytes(), &planned).unwrap();
        assert_eq!(output, format!("---\nfolio-id: {ID}\n---\nbody").as_bytes());
    }
}

#[test]
fn written_plain_scalars_read_back_and_repeat_identically() {
    let empty = b"---\n---\n";
    for name in ["-logo", "--", "logo"] {
        let output =
            apply_edit(empty, &set_name(empty, Some(name), false).unwrap().unwrap()).unwrap();
        assert_eq!(read_name(&output).unwrap().as_deref(), Some(name));
        assert_eq!(rename(&output, name), output);
    }
    let id = FolioId::parse(ID).unwrap();
    let output = apply_edit(empty, &set_id(empty, Some(id), false).unwrap().unwrap()).unwrap();
    assert_eq!(read_id(&output).unwrap(), Some(id));
    let twice = apply_edit(&output, &set_id(&output, Some(id), false).unwrap().unwrap()).unwrap();
    assert_eq!(twice, output);

    let reference = Reference {
        alias: "-logo".into(),
        name: Some("-image".into()),
        id: None,
        pin: Some("-pin".into()),
    };
    let output = apply_edit(
        empty,
        &set_reference(empty, "-logo", Some(&reference), false)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(read_references(&output).unwrap(), vec![reference.clone()]);
    let twice = apply_edit(
        &output,
        &set_reference(&output, "-logo", Some(&reference), false)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(twice, output);
    assert_eq!(
        set_name(empty, Some("-"), false),
        Err(FrontMatterError::InvalidValue)
    );
    let mut invalid = reference.clone();
    invalid.alias = "-".into();
    assert_eq!(
        set_references(empty, &[invalid], false),
        Err(FrontMatterError::InvalidValue)
    );
    for value in ["-", "- logo"] {
        let bytes = format!("---\nfolio-name: {value}\n---\n");
        assert_eq!(
            read_name(bytes.as_bytes()),
            Err(FrontMatterError::InvalidValue)
        );
    }
}

#[test]
fn scalar_replacements_preserve_spacing_and_refuse_empty_values() {
    let id = FolioId::parse(ID).unwrap();
    for spacing in [" ", "\t"] {
        let input = format!("---\nfolio-name:{spacing}old\nfolio-id:{spacing}{ID}\n---\n");
        let output = apply_edit(
            input.as_bytes(),
            &set_name(input.as_bytes(), Some("new"), false)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            text(&output).unwrap(),
            format!("---\nfolio-name:{spacing}\"new\"\nfolio-id:{spacing}{ID}\n---\n")
        );
        let output = apply_edit(
            input.as_bytes(),
            &set_id(input.as_bytes(), Some(id), false).unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(output, input.as_bytes());
    }
    // Empty scalar values are invalid and must not be rewritten.
    for spacing in ["", " ", "\t"] {
        let input = format!("---\nfolio-name:{spacing}\n---\n");
        assert_eq!(
            set_name(input.as_bytes(), Some("new"), false),
            Err(FrontMatterError::InvalidValue)
        );
        let input = format!("---\nfolio-id:{spacing}\n---\n");
        assert_eq!(
            set_id(input.as_bytes(), Some(id), false),
            Err(FrontMatterError::InvalidValue)
        );
    }
}

#[test]
fn scalar_rewrites_preserve_header_comment_spacing() {
    for ending in ["\n", "\r\n"] {
        for spacing in [" ", "   "] {
            for key in ["folio-name", "folio-id"] {
                let old = if key == "folio-name" { "old" } else { ID };
                let input =
                    format!("---{ending}{key}: {old}{spacing}# header{ending}---{ending}body");
                let planned = if key == "folio-name" {
                    set_name(input.as_bytes(), Some("new"), false)
                } else {
                    set_id(input.as_bytes(), Some(FolioId::parse(ID).unwrap()), false)
                }
                .unwrap()
                .unwrap();
                let output = apply_edit(input.as_bytes(), &planned).unwrap();
                assert_eq!(
                    text(&output).unwrap(),
                    input.replace(old, if key == "folio-name" { "\"new\"" } else { ID })
                );
                assert!(text(&output).unwrap().contains(" # header"));
            }
        }
    }
}
