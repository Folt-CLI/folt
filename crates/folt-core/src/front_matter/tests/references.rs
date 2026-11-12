use super::*;

#[test]
fn reference_target_mapping_rules_apply_to_block_and_flow() {
    for fields in [
        "",
        "pin: opaque",
        "name: image, id: 12345678-1234-4234-8234-123456789abc",
    ] {
        for entry in [
            format!("  logo: {{{fields}}}\n"),
            format!("  logo:\n    {}\n", fields.replace(", ", "\n    ")),
        ] {
            let input = format!("---\nfolio-references:\n{entry}---\n");
            assert_eq!(
                read_references(input.as_bytes()),
                Err(FrontMatterError::InvalidValue)
            );
        }
    }
    for fields in [
        "    name: image\n    path: x\n",
        "    name: image\n      pin: x\n",
        "    name: image\n    name: other\n",
    ] {
        let input = format!("---\nfolio-references:\n  logo:\n{fields}---\n");
        assert!(read_references(input.as_bytes()).is_err());
    }
    let reference = Reference {
        alias: "logo".into(),
        name: Some("image".into()),
        id: None,
        pin: Some("a@pin with \"quotes\" and \\slashes".into()),
    };
    let input = b"# body";
    let output = apply_edit(
        input,
        &set_references(input, std::slice::from_ref(&reference), true)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(
        text(&output)
            .unwrap()
            .contains("  logo:\n    name: \"image\"\n    pin: \"")
    );
    assert_eq!(read_references(&output).unwrap(), vec![reference]);
}

#[test]
fn reads_all_reference_forms() {
    for target in ["name: logo-image", &format!("id: {ID}")] {
        for pin in ["", ", pin: abc123"] {
            for body in [
                format!("folio-references: {{logo: {{{target}{pin}}}}}\n"),
                format!("folio-references:\n  logo: {{{target}{pin}}}\n"),
                format!(
                    "folio-references:\n  logo:\n    {}\n",
                    format!("{target}{pin}").replace(", ", "\n    ")
                ),
            ] {
                let input = format!("---\n{body}---\nbody");
                let refs = read_references(input.as_bytes()).unwrap();
                assert_eq!(refs.len(), 1);
                assert_ne!(refs[0].name.is_some(), refs[0].id.is_some());
                assert_eq!(
                    refs[0].pin.as_deref(),
                    (!pin.is_empty()).then_some("abc123")
                );
                let edit = set_reference(input.as_bytes(), "logo", Some(&logo()), false)
                    .unwrap()
                    .unwrap();
                let output = apply_edit(input.as_bytes(), &edit).unwrap();
                assert_eq!(read_references(&output).unwrap(), vec![logo()]);
            }
        }
    }
    for value in [
        "logo-image",
        &format!("id:{ID}"),
        "{}",
        "{pin: abc}",
        &format!("{{name: image, id: {ID}}}"),
    ] {
        let input = format!("---\nfolio-references:\n  logo: {value}\n---\n");
        assert_eq!(
            read_references(input.as_bytes()),
            Err(FrontMatterError::InvalidValue)
        );
    }
}

#[test]
fn nested_flow_mapping_comments_are_understood() {
    for ending in ["\n", "\r\n"] {
        let input = "---\nfolio-references: {logo: {name: image, # keep\n    pin: abc123}}\n---\n"
            .replace('\n', ending);
        let mut expected = logo();
        expected.name = Some("image".into());
        assert_eq!(read_references(input.as_bytes()).unwrap(), vec![expected]);
    }
}

#[test]
fn block_entries_read_whole_wrapped_flow_values() {
    for ending in ["\n", "\r\n"] {
        for comment in ["", " # keep"] {
            let input = format!(
                "---\nfolio-references:\n  logo: {{name: image,{comment}\n    pin: abc123}}\n---\n"
            )
            .replace('\n', ending);
            let mut expected = logo();
            expected.name = Some("image".into());
            assert_eq!(read_references(input.as_bytes()).unwrap(), vec![expected]);
        }
    }
}

#[test]
fn nested_block_field_keys_allow_spaces_before_colons() {
    for ending in ["\n", "\r\n"] {
        let input = "---\nfolio-references:\n  logo:\n    name : image\n    pin  : abc123\n---\n"
            .replace('\n', ending);
        let mut expected = logo();
        expected.name = Some("image".into());
        assert_eq!(read_references(input.as_bytes()).unwrap(), vec![expected]);
        let duplicate = input.replace("pin  : abc123", "name: duplicate");
        assert_eq!(
            read_references(duplicate.as_bytes()),
            Err(FrontMatterError::Duplicate)
        );
    }
}

#[test]
fn nested_block_entries_reject_empty_aliases() {
    for ending in ["\n", "\r\n"] {
        for alias in ["''", "\"\""] {
            let input = format!("---\nfolio-references:\n  {alias}:\n    name: image\n---\n")
                .replace('\n', ending);
            let bytes = input.as_bytes();
            assert_eq!(read_references(bytes), Err(FrontMatterError::InvalidValue));
            assert_eq!(
                set_references(bytes, &[logo()], false),
                Err(FrontMatterError::InvalidValue)
            );
            assert_eq!(
                set_reference(bytes, "logo", Some(&logo()), false),
                Err(FrontMatterError::InvalidValue)
            );
            assert_eq!(
                set_reference(bytes, "logo", None, false),
                Err(FrontMatterError::InvalidValue)
            );
        }
    }
}

#[test]
fn wrapped_flow_comments_and_indentation_are_understood() {
    let bytes = b"---\nfolio-references: {\n# inside\nlogo: {name: logo-image}, # first\nother: {name: other-image} # last\n}  # outside\nforeign: [\n  {folio-name: foreign}\n]\n---\n";
    assert_eq!(read_references(bytes).unwrap().len(), 2);
    let output = apply_edit(
        bytes,
        &set_reference(bytes, "logo", Some(&logo()), false)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(read_references(&output).unwrap().len(), 2);
    assert!(
        text(&output)
            .unwrap()
            .contains("foreign: [\n  {folio-name: foreign}\n]")
    );

    let bytes = b"---\nfolio-references:\n    other: {name: other-image}\n# tail\n---\n";
    let output = apply_edit(
        bytes,
        &set_reference(bytes, "logo", Some(&logo()), false)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(read_references(&output).unwrap().len(), 2);
    assert!(
        text(&output)
            .unwrap()
            .contains("    logo:\n      name: \"logo-image\"")
    );
    let output = apply_edit(
        &output,
        &set_references(&output, &[], false).unwrap().unwrap(),
    )
    .unwrap();
    assert_eq!(output, b"---\n# tail\n---\n");
}

#[test]
fn empty_reference_targets_and_pins_are_invalid() {
    for ending in ["\n", "\r\n"] {
        let mut bodies = Vec::new();
        for field in ["name", "id", "pin"] {
            for value in ["", "''", "\"\""] {
                let other_block = if field == "name" {
                    String::new()
                } else {
                    format!("    name: image{ending}")
                };
                let other_flow = if field == "name" { "" } else { "name: image, " };
                bodies.push(format!(
                    "folio-references:{ending}  logo:{ending}{other_block}    {field}: {value}{ending}"
                ));
                bodies.push(format!(
                    "folio-references: {{logo: {{{other_flow}{field}: {value}}}}}{ending}"
                ));
                bodies.push(format!(
                    "folio-references:{ending}  logo: {{{other_flow}{field}: {value}}}{ending}"
                ));
            }
        }
        for value in ["", "@pin", "name@"] {
            bodies.push(format!("folio-references:{ending}  logo: {value}{ending}"));
            bodies.push(format!("folio-references: {{logo: {value}}}{ending}"));
        }
        for body in bodies {
            let input = format!("---{ending}{body}---{ending}");
            assert_eq!(
                read_references(input.as_bytes()),
                Err(FrontMatterError::InvalidValue),
                "{input:?}"
            );
        }
    }
}

#[test]
fn malformed_references_are_refused_without_partial_edits() {
    for (body, error) in [
        (
            "folio-references: {logo: {name: one}, logo: {name: two}}\n",
            FrontMatterError::Duplicate,
        ),
        (
            "folio-references:\n  logo: {name: one, name: two}\n",
            FrontMatterError::Duplicate,
        ),
        (
            "folio-references:\n  logo: *other\n",
            FrontMatterError::Unsupported,
        ),
        (
            "folio-references:\n  logo: !str one\n",
            FrontMatterError::Unsupported,
        ),
        (
            "folio-references:\n\tlogo: one\n",
            FrontMatterError::Unsupported,
        ),
        (
            "folio-references: {} garbage\n",
            FrontMatterError::Unsupported,
        ),
        (
            "folio-references:\n  logo: one@\n",
            FrontMatterError::InvalidValue,
        ),
    ] {
        let bytes = format!("---\n{body}---\n");
        assert_eq!(read_references(bytes.as_bytes()), Err(error), "{body}");
        assert_eq!(
            set_reference(bytes.as_bytes(), "logo", Some(&logo()), true),
            Err(error),
            "{body}"
        );
    }
}
