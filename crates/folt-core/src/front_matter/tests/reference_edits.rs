use super::*;

#[test]
fn wrapped_flow_alias_edits_preserve_all_comments() {
    for ending in ["\n", "\r\n"] {
        for (entry, header, comments) in [
            (
                "  logo: {name: old,\n    pin: abc} # keep\n",
                "",
                vec!["# keep"],
            ),
            (
                "  logo: {name: old, # c1\n    pin: abc} # keep\n",
                " # c1",
                vec!["# keep"],
            ),
            (
                "  logo: {name: old, # c1\n # middle\n    pin: abc, # c2\n    } # keep\n",
                " # c1",
                vec!["# middle", "# c2", "# keep"],
            ),
        ] {
            let prefix = "---\nfolio-references:\n  before: {name: before} # untouched\n";
            let suffix = "  after: {name: after} # untouched\n---\nbody\n";
            let input = format!("{prefix}{entry}{suffix}").replace('\n', ending);
            for value in [Some(logo()), None] {
                let mut replacement = if value.is_some() {
                    format!("  logo:{header}\n    name: \"logo-image\"\n    pin: \"abc123\"\n")
                } else if header.is_empty() {
                    String::new()
                } else {
                    format!("  {}\n", header.trim_start())
                };
                for comment in &comments {
                    let spaces = if value.is_some() { "    " } else { "  " };
                    replacement.push_str(&format!("{spaces}{comment}\n"));
                }
                let expected = format!("{prefix}{replacement}{suffix}").replace('\n', ending);
                let output = apply_edit(
                    input.as_bytes(),
                    &locate(input.as_bytes())
                        .and_then(|block| {
                            Block::set_reference(
                                block.as_ref(),
                                input.as_bytes(),
                                "logo",
                                value.as_ref(),
                                false,
                            )
                        })
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(text(&output).unwrap(), expected);
                let again = locate(&output)
                    .and_then(|block| {
                        Block::set_reference(block.as_ref(), &output, "logo", value.as_ref(), false)
                    })
                    .unwrap();
                assert_eq!(
                    again.map_or_else(|| output.clone(), |e| apply_edit(&output, &e).unwrap()),
                    output
                );
            }
        }
    }
}

#[test]
fn single_alias_flow_edits_preserve_trailing_separators() {
    for collection in ["{}", "{other: {name: other}}", "{other: {name: other},}"] {
        let input = format!("---\nfolio-references: {collection}\n---\n");
        let output = apply_edit(
            input.as_bytes(),
            &locate(input.as_bytes())
                .and_then(|block| {
                    Block::set_reference(
                        block.as_ref(),
                        input.as_bytes(),
                        "logo",
                        Some(&logo()),
                        false,
                    )
                })
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let refs = read_references(&output).unwrap();
        assert!(refs.contains(&logo()));
        assert!(!text(&output).unwrap().contains(",,"));
    }
    let input = b"---\nfolio-references: {logo: {name: old},}\n---\n";
    let output = apply_edit(
        input,
        &locate(input)
            .and_then(|block| Block::set_reference(block.as_ref(), input, "logo", None, false))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(output, b"---\nfolio-references: {}\n---\n");
}

#[test]
fn single_alias_edits_preserve_other_document_bytes() {
    for ending in ["\n", "\r\n"] {
        for entry in ["  logo:\n    name: old\n", "  logo: {name: old}\n"] {
            let prefix =
                "---\nfolio-references: # header\n  before: {name: before}\n\n# keep before\n";
            let suffix =
                "\n# keep between\n  after: {name: after} # untouched\n\n# keep after\n---\nbody";
            let input = format!("{prefix}{entry}{suffix}").replace('\n', ending);
            for value in [Some(logo()), None] {
                let output = apply_edit(
                    input.as_bytes(),
                    &locate(input.as_bytes())
                        .and_then(|block| {
                            Block::set_reference(
                                block.as_ref(),
                                input.as_bytes(),
                                "logo",
                                value.as_ref(),
                                false,
                            )
                        })
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                let expected = format!(
                    "{prefix}{}{suffix}",
                    if value.is_some() {
                        render_reference(&logo(), "\n").unwrap()
                    } else {
                        String::new()
                    }
                )
                .replace('\n', ending);
                assert_eq!(output, expected.as_bytes());
                let again = locate(&output)
                    .and_then(|block| {
                        Block::set_reference(block.as_ref(), &output, "logo", value.as_ref(), false)
                    })
                    .unwrap();
                assert_eq!(
                    again.map_or_else(|| output.clone(), |e| apply_edit(&output, &e).unwrap()),
                    output
                );
            }
        }
        for alias in ["before", "logo", "after"] {
            let input = "---\nfolio-references: {before: {name: before}, # keep\n logo: {name: old}, after: {name: after}} # tail\n---\n".replace('\n', ending);
            let mut r = logo();
            r.alias = alias.into();
            for value in [Some(&r), None] {
                let output = apply_edit(
                    input.as_bytes(),
                    &locate(input.as_bytes())
                        .and_then(|block| {
                            Block::set_reference(
                                block.as_ref(),
                                input.as_bytes(),
                                alias,
                                value,
                                false,
                            )
                        })
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                let original = read_references(input.as_bytes()).unwrap();
                let actual = read_references(&output).unwrap();
                for other in original.iter().filter(|r| r.alias != alias) {
                    assert!(actual.contains(other));
                    let unchanged = format!(
                        "{}: {{name: {}}}",
                        other.alias,
                        other.name.as_ref().unwrap()
                    );
                    assert!(text(&output).unwrap().contains(&unchanged));
                }
                assert!(text(&output).unwrap().contains(&format!("# keep{ending}")));
                assert!(text(&output).unwrap().contains(&format!("# tail{ending}")));
                let again = locate(&output)
                    .and_then(|block| {
                        Block::set_reference(block.as_ref(), &output, alias, value, false)
                    })
                    .unwrap();
                assert_eq!(
                    again.map_or_else(|| output.clone(), |e| apply_edit(&output, &e).unwrap()),
                    output
                );
            }
        }
    }
}

#[test]
fn reference_value_replacement_preserves_yaml_separation() {
    for spacing in ["", " ", "   ", "\t"] {
        let input = format!("---\nfolio-references:{spacing}{{logo: {{name: old}}}}\n---\n");
        let output = apply_edit(
            input.as_bytes(),
            &locate(input.as_bytes())
                .and_then(|block| {
                    Block::set_reference(
                        block.as_ref(),
                        input.as_bytes(),
                        "logo",
                        Some(&logo()),
                        false,
                    )
                })
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(read_references(&output).unwrap(), vec![logo()]);
        assert!(
            text(&output)
                .unwrap()
                .contains("logo: {name: \"logo-image\", pin: \"abc123\"}")
        );
    }
}

#[test]
fn nested_reference_replacement_keeps_trailing_comments_and_blank_lines() {
    for tail in ["# keep\n", "\n# keep\n\n"] {
        let input = format!("---\nfolio-references:\n  logo:\n    name: old\n{tail}---\n");
        for whole in [true, false] {
            let edit = if whole {
                locate(input.as_bytes()).and_then(|block| {
                    Block::set_references(block.as_ref(), input.as_bytes(), &[logo()], false)
                })
            } else {
                locate(input.as_bytes()).and_then(|block| {
                    Block::set_reference(
                        block.as_ref(),
                        input.as_bytes(),
                        "logo",
                        Some(&logo()),
                        false,
                    )
                })
            }
            .unwrap()
            .unwrap();
            let output = apply_edit(input.as_bytes(), &edit).unwrap();
            assert_eq!(read_references(&output).unwrap(), vec![logo()]);
            assert!(text(&output).unwrap().ends_with(&format!("{tail}---\n")));
        }
    }
}

#[test]
fn wrapped_and_commented_reference_edits_preserve_surrounding_bytes() {
    for ending in ["\n", "\r\n"] {
        for flow in [false, true] {
            for comment in ["", " # keep"] {
                let old = format!("{{name: image,{comment}\n    pin: abc123}}");
                let collection = if flow {
                    format!(
                        "folio-references: {{logo: {old}, other: {{name: other-image}}}}  # outside\n"
                    )
                } else {
                    format!(
                        "folio-references: # header\n  logo: {old}  # outside\n  other: {{name: other-image}}\n"
                    )
                };
                let input =
                    format!("---\n# before\n{collection}# tail\nforeign: untouched\n---\nbody\n")
                        .replace('\n', ending);
                let bytes = input.as_bytes();
                let block = &locate(bytes).unwrap().unwrap();
                let mut other = logo();
                other.alias = "other".into();
                other.name = Some("other-image".into());
                other.pin = None;
                let desired = [logo(), other.clone()];
                for whole in [false, true] {
                    let planned = if whole {
                        Block::set_references(Some(block), bytes, &desired, false)
                    } else {
                        Block::set_reference(Some(block), bytes, "logo", Some(&logo()), false)
                    }
                    .unwrap()
                    .unwrap();
                    let output = apply_edit(bytes, &planned).unwrap();
                    let updated = &locate(&output).unwrap().unwrap();
                    assert!(
                        text(&output)
                            .unwrap()
                            .starts_with(&format!("---{ending}# before{ending}"))
                    );
                    assert!(text(&output).unwrap().ends_with(&format!(
                        "# tail{ending}foreign: untouched{ending}---{ending}body{ending}"
                    )));
                    assert_eq!(read_references(&output).unwrap(), desired);
                    let again = if whole {
                        Block::set_references(Some(updated), &output, &desired, false)
                    } else {
                        Block::set_reference(Some(updated), &output, "logo", Some(&logo()), false)
                    }
                    .unwrap()
                    .unwrap();
                    assert_eq!(apply_edit(&output, &again).unwrap(), output);
                }
                let edit = Block::set_reference(Some(block), bytes, "logo", None, false)
                    .unwrap()
                    .unwrap();
                let removed = apply_edit(bytes, &edit).unwrap();
                assert_eq!(read_references(&removed).unwrap(), vec![other]);
                let removed_block = &locate(&removed).unwrap().unwrap();
                assert_eq!(
                    Block::set_reference(Some(removed_block), &removed, "logo", None, false)
                        .unwrap(),
                    None
                );
                let empty = input.replace(&collection.replace('\n', ending), "");
                let edit = Block::set_references(Some(block), bytes, &[], false)
                    .unwrap()
                    .unwrap();
                let cleared = apply_edit(bytes, &edit).unwrap();
                assert_eq!(text(&cleared).unwrap(), empty);
                assert!(read_references(&cleared).unwrap().is_empty());
                let cleared_block = &locate(&cleared).unwrap().unwrap();
                assert_eq!(
                    Block::set_references(Some(cleared_block), &cleared, &[], false).unwrap(),
                    None
                );

                let edit = Block::set_reference(Some(block), bytes, "other", None, false)
                    .unwrap()
                    .unwrap();
                let only_logo = apply_edit(bytes, &edit).unwrap();
                let mut expected = logo();
                expected.name = Some("image".into());
                assert_eq!(read_references(&only_logo).unwrap(), vec![expected]);
                let block = &locate(&only_logo).unwrap().unwrap();
                assert_eq!(
                    Block::set_reference(Some(block), &only_logo, "other", None, false).unwrap(),
                    None
                );
                let edit = Block::set_reference(Some(block), &only_logo, "logo", None, false)
                    .unwrap()
                    .unwrap();
                let cleared = apply_edit(&only_logo, &edit).unwrap();
                assert!(read_references(&cleared).unwrap().is_empty());
                let block = &locate(&cleared).unwrap().unwrap();
                assert!(text(&cleared).unwrap().contains("folio-references:"));
                assert_eq!(
                    Block::set_reference(Some(block), &cleared, "logo", None, false).unwrap(),
                    None
                );
            }
        }
    }
}
