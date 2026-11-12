use super::*;

#[test]
fn selected_alias_inline_comments_survive_set_and_remove() {
    for ending in ["\n", "\r\n"] {
        for entry in [
            "  logo: {name: old} # keep\n",
            "  logo: # keep\n    name: old\n",
        ] {
            for value in [Some(logo()), None] {
                let prefix =
                    "---\nfolio-references:\n  before: {name: before} # neighbour before\n";
                let suffix = "  after: # neighbour after\n    name: after\n---\nbody\n";
                let input = format!("{prefix}{entry}{suffix}").replace('\n', ending);
                let selected = if value.is_some() {
                    "  logo: # keep\n    name: \"logo-image\"\n    pin: \"abc123\"\n"
                } else {
                    "  # keep\n"
                };
                let expected = format!("{prefix}{selected}{suffix}").replace('\n', ending);
                let output = apply_edit(
                    input.as_bytes(),
                    &set_reference(input.as_bytes(), "logo", value.as_ref(), false)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(text(&output).unwrap(), expected);
                let repeated = set_reference(&output, "logo", value.as_ref(), false).unwrap();
                if let Some(edit) = repeated {
                    assert_eq!(apply_edit(&output, &edit).unwrap(), output);
                }
            }
        }
    }
}

#[test]
fn nested_field_inline_comments_survive_set_and_remove() {
    for ending in ["\n", "\r\n"] {
        for indent in [2, 4] {
            for (fields, desired, rewritten) in [
                (
                    "    name: old # keep name\n    pin: abc # keep pin\n",
                    Some(logo()),
                    "    name: \"logo-image\" # keep name\n    pin: \"abc123\" # keep pin\n",
                ),
                (
                    "    name: old # keep name\n    pin: abc # keep pin\n",
                    Some(Reference {
                        pin: None,
                        ..logo()
                    }),
                    "    name: \"logo-image\" # keep name\n    # keep pin\n",
                ),
                (
                    "    id: ID # keep id\n    pin: abc # keep pin\n",
                    Some(Reference {
                        pin: None,
                        ..logo()
                    }),
                    "    name: \"logo-image\"\n    # keep id\n    # keep pin\n",
                ),
                (
                    "    id: ID # keep id\n    pin: abc # keep pin\n",
                    Some(Reference {
                        name: None,
                        id: Some(FolioId::parse(ID).unwrap()),
                        ..logo()
                    }),
                    "    id: \"ID\" # keep id\n    pin: \"abc123\" # keep pin\n",
                ),
                (
                    "    name: old # keep name\n",
                    Some(Reference {
                        name: None,
                        id: Some(FolioId::parse(ID).unwrap()),
                        ..logo()
                    }),
                    "    id: \"ID\"\n    pin: \"abc123\"\n    # keep name\n",
                ),
                (
                    "    name: old # keep name\n\n    # between fields\n    pin: abc # keep pin\n",
                    None,
                    "  # keep name\n\n    # between fields\n  # keep pin\n",
                ),
                (
                    "    name: old # keep name\n    pin: abc # keep pin\n",
                    None,
                    "  # keep name\n  # keep pin\n",
                ),
                (
                    "    pin: abc # keep pin\n    id: ID # keep id\n",
                    None,
                    "  # keep pin\n  # keep id\n",
                ),
            ] {
                let prefix = "---\nfolio-references: # references\n  before: {name: before} # neighbour before\n";
                let suffix = "  after: # neighbour after\n    name: after\n---\nbody\n";
                let header = if desired.is_some() {
                    "  logo: # alias\n"
                } else {
                    "  # alias\n"
                };
                let input = format!("{prefix}  logo: # alias\n{fields}{suffix}");
                let expected = format!("{prefix}{header}{rewritten}{suffix}");
                let adapt = |s: String| {
                    s.replace("ID", ID)
                        .split_inclusive('\n')
                        .map(|line| {
                            if line.starts_with(' ') {
                                format!("{}{line}", " ".repeat(indent - 2))
                            } else {
                                line.to_owned()
                            }
                        })
                        .collect::<String>()
                        .replace('\n', ending)
                };
                let input = adapt(input);
                let expected = adapt(expected);
                let output = apply_edit(
                    input.as_bytes(),
                    &set_reference(input.as_bytes(), "logo", desired.as_ref(), false)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(text(&output).unwrap(), expected);
                if let Some(edit) = set_reference(&output, "logo", desired.as_ref(), false).unwrap()
                {
                    assert_eq!(apply_edit(&output, &edit).unwrap(), output);
                }
            }
        }
    }
}

#[test]
fn reference_value_edits_keep_comments_and_other_entries() {
    let input = b"---\nfolio-references: # header\n  logo: {name: old}\n  other: {name: other-image}\n# tail\nforeign: x\n---\nbody";
    let output = apply_edit(
        input,
        &set_reference(input, "logo", Some(&logo()), false)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(read_references(&output).unwrap().len(), 2);
    assert!(text(&output).unwrap().contains("# header\n"));
    let output = apply_edit(
        &output,
        &set_reference(&output, "logo", None, false)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(read_references(&output).unwrap().len(), 1);
    assert!(
        text(&output)
            .unwrap()
            .contains("# tail\nforeign: x\n---\nbody")
    );
}

#[test]
fn flow_comments_are_whitespace_in_reads_and_edits() {
    let inner_values = [
        "{name: # keep\n    image}",
        "{ # keep\n    name: image, pin: abc123}",
        "{name: # keep\n    image, pin: abc123}",
        "{name: image, # keep\n    pin: abc123}",
        "{name: image, pin: abc123 # keep\n    }",
        "{name # keep\n    : image, pin: abc123}",
    ];
    let mut cases = Vec::new();
    for inner in inner_values {
        cases.push((
            true,
            format!("{{logo: {inner}, other: {{name: other-image}}}}"),
        ));
        cases.push((
            false,
            format!("  logo: {inner}  # entry\n  other: {{name: other-image}}\n"),
        ));
    }
    // Exercise the same positions in the outer flow mapping as well.
    for outer in [
        "{ # keep\n    logo: {name: image}, other: {name: other-image}}",
        "{logo: # keep\n    {name: image}, other: {name: other-image}}",
        "{logo: {name: image}, # keep\n    other: {name: other-image}}",
        "{logo: {name: image}, other: {name: other-image} # keep\n    }",
        "{logo # keep\n    : {name: image}, other: {name: other-image}}",
    ] {
        cases.push((true, outer.into()));
    }
    for ending in ["\n", "\r\n"] {
        for (flow, value) in &cases {
            let collection = if *flow {
                format!("folio-references: {value} # header\n")
            } else {
                format!("folio-references: # header\n{value}")
            };
            let input =
                format!("---\n# before\n{collection}# tail\nforeign: untouched\n---\nbody\n")
                    .replace('\n', ending);
            let bytes = input.as_bytes();
            let original = read_references(bytes).unwrap();
            assert_eq!(original.len(), 2, "{input}");
            assert_eq!(original[0].alias, "logo");
            assert_eq!(original[0].name.as_deref(), Some("image"));
            assert_eq!(
                original[0].pin.as_deref(),
                value.contains("pin:").then_some("abc123")
            );
            assert_eq!(original[1].alias, "other");
            assert_eq!(original[1].name.as_deref(), Some("other-image"));
            let desired = [logo(), original[1].clone()];
            for operation in 0..4 {
                let planned = match operation {
                    0 => set_references(bytes, &desired, false),
                    1 => set_reference(bytes, "logo", Some(&desired[0]), false),
                    2 => set_reference(bytes, "logo", None, false),
                    _ => set_references(bytes, &[], false),
                }
                .unwrap()
                .unwrap();
                let output = apply_edit(bytes, &planned).unwrap();
                assert!(
                    text(&output)
                        .unwrap()
                        .starts_with(&format!("---{ending}# before{ending}"))
                );
                assert!(text(&output).unwrap().ends_with(&format!(
                    "# tail{ending}foreign: untouched{ending}---{ending}body{ending}"
                )));
                let expected_refs = match operation {
                    0 | 1 => desired.to_vec(),
                    2 => vec![original[1].clone()],
                    _ => Vec::new(),
                };
                assert_eq!(read_references(&output).unwrap(), expected_refs);
            }
        }
    }
}

#[test]
fn reference_rewrites_separate_preserved_header_comments() {
    for ending in ["\n", "\r\n"] {
        for collection in [
            "folio-references: # header\n",
            "folio-references: {} # header\n",
            "folio-references: {logo: {name: old}}# header\n",
            "folio-references: # header\n  logo:\n    name: old\n",
        ] {
            let input = format!("---\n{collection}# tail\nforeign: untouched\n---\nbody\n")
                .replace('\n', ending);
            for whole in [true, false] {
                let edit = if whole {
                    set_references(input.as_bytes(), &[logo()], false)
                } else {
                    set_reference(input.as_bytes(), "logo", Some(&logo()), false)
                }
                .unwrap()
                .unwrap();
                let output = apply_edit(input.as_bytes(), &edit).unwrap();
                assert_eq!(read_references(&output).unwrap(), vec![logo()]);
                if whole || !collection.contains('{') {
                    assert!(
                        text(&output)
                            .unwrap()
                            .contains(&format!("# header{ending}  logo:"))
                    );
                } else {
                    assert!(
                        text(&output)
                            .unwrap()
                            .contains(&format!("# header{ending}"))
                    );
                }
                assert!(text(&output).unwrap().ends_with(&format!(
                    "# tail{ending}foreign: untouched{ending}---{ending}body{ending}"
                )));
            }
        }
    }
}
