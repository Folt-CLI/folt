use super::*;

fn assert_hidden_key(bytes: &[u8]) {
    let error = FrontMatterError::Hidden;
    assert_eq!(locate(bytes), Err(error), "{}", text(bytes).unwrap());
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
}

#[test]
fn hidden_owned_keys_refuse_reads_and_duplicate_creating_edits() {
    for ending in ["\n", "\r\n"] {
        for (key, value) in [
            ("folio-id", ID),
            ("folio-name", "old"),
            ("folio-references", "{}"),
        ] {
            for spacing in ["", " \t"] {
                let owned = format!("{key}{spacing}: {value}\n");
                for flow in [
                    // Later openers restore the permissive counter after
                    // the stray closer, swallowing the owned line too.
                    format!("flow: {{a:]1,\n  b: 2}}\n{owned}tail: {{{{{{}}}}}}\n"),
                    format!("flow: {{a: 1,\n{owned}# far below\n\n  b: 2}}\n"),
                ] {
                    for prefix in ["", owned.as_str()] {
                        let input = format!("---\n{prefix}{flow}---\n").replace('\n', ending);
                        assert_hidden_key(input.as_bytes());
                    }
                }
            }
        }
    }
}

#[test]
fn owned_key_audit_ignores_plain_scalars_in_foreign_flows() {
    for ending in ["\n", "\r\n"] {
        for key in OWNED {
            for separator in [":", " :"] {
                let input = format!(
                    "---\nforeign: [\n{key}{separator}manual\n]\nfolio-name: actual\nfolio-id: {ID}\nfolio-references: {{}}\n---\n"
                )
                .replace('\n', ending);
                assert_owned_values_and_edits(input.as_bytes());
            }
        }
    }
}

fn assert_owned_values_and_edits(bytes: &[u8]) {
    let id = FolioId::parse(ID).unwrap();
    assert_eq!(read_name(bytes).unwrap().as_deref(), Some("actual"));
    assert_eq!(read_id(bytes).unwrap(), Some(id));
    assert!(read_references(bytes).unwrap().is_empty());

    let output = apply_edit(
        bytes,
        &set_name(bytes, Some("new"), false).unwrap().unwrap(),
    )
    .unwrap();
    assert_eq!(read_name(&output).unwrap().as_deref(), Some("new"));
    let output = apply_edit(bytes, &set_id(bytes, Some(id), false).unwrap().unwrap()).unwrap();
    assert_eq!(read_id(&output).unwrap(), Some(id));
    let output = apply_edit(
        bytes,
        &set_references(bytes, &[logo()], false).unwrap().unwrap(),
    )
    .unwrap();
    assert_eq!(read_references(&output).unwrap(), vec![logo()]);
}

#[test]
fn owned_key_audit_counts_unicode_whitespace_consistently() {
    for ending in ["\n", "\r\n"] {
        for key in OWNED {
            for whitespace in ['\u{00a0}', '\u{2003}', '\u{3000}'] {
                for spelling in [format!("{whitespace}{key}"), format!("{key}{whitespace}")] {
                    let input = format!(
                        "---\nfolio-name: actual\nfolio-id: {ID}\nfolio-references: {{}}\n---\n"
                    )
                    .replace(key, &spelling)
                    .replace('\n', ending);
                    assert_owned_values_and_edits(input.as_bytes());
                }
            }
        }
    }
}

#[test]
fn owned_key_audit_ignores_tab_indented_keys() {
    for ending in ["\n", "\r\n"] {
        let input = format!(
            "---\n\tfolio-name: ignored\n\tfolio-id: {ID}\n\tfolio-references: {{}}\n---\n"
        )
        .replace('\n', ending);
        let bytes = input.as_bytes();
        assert!(locate(bytes).unwrap().unwrap().nodes.is_empty());
        assert_eq!(read_name(bytes).unwrap(), None);
        assert_eq!(read_id(bytes).unwrap(), None);
        assert!(read_references(bytes).unwrap().is_empty());

        let output = apply_edit(
            bytes,
            &set_name(bytes, Some("actual"), false).unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(read_name(&output).unwrap().as_deref(), Some("actual"));
        let output = apply_edit(
            bytes,
            &set_id(bytes, Some(FolioId::parse(ID).unwrap()), false)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(read_id(&output).unwrap(), Some(FolioId::parse(ID).unwrap()));
        let output = apply_edit(
            bytes,
            &set_references(bytes, &[logo()], false).unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(read_references(&output).unwrap(), vec![logo()]);
    }
}

#[test]
fn owned_key_audit_counts_tab_and_empty_values_consistently() {
    for ending in ["\n", "\r\n"] {
        let input =
            format!("---\nfolio-name:\tactual\nfolio-id:\t{ID}\nfolio-references:\t{{}}\n---\n")
                .replace('\n', ending);
        assert_owned_values_and_edits(input.as_bytes());

        let input = "---\nfolio-name:\nfolio-id:\nfolio-references:\n---\n".replace('\n', ending);
        let bytes = input.as_bytes();
        let block = locate(bytes).unwrap().unwrap();
        assert_eq!(block.nodes.len(), OWNED.len());
        assert_eq!(read_name(bytes), Err(FrontMatterError::InvalidValue));
        assert_eq!(read_id(bytes), Err(FrontMatterError::InvalidValue));
        assert!(read_references(bytes).unwrap().is_empty());
        assert_eq!(
            set_name(bytes, Some("new"), false),
            Err(FrontMatterError::InvalidValue)
        );
        assert_eq!(
            set_id(bytes, Some(FolioId::parse(ID).unwrap()), false),
            Err(FrontMatterError::InvalidValue)
        );
        let output = apply_edit(
            bytes,
            &set_references(bytes, &[logo()], false).unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(read_references(&output).unwrap(), vec![logo()]);
    }
}

#[test]
fn checked_edits_reject_newly_swallowed_owned_lines() {
    for ending in ["\n", "\r\n"] {
        let input = "---\nflow: {a: 1,\n  b: 2}\n---\n".replace('\n', ending);
        let bytes = input.as_bytes();
        assert!(locate(bytes).unwrap().is_some());
        let position = input.find("  b:").unwrap();
        for key in OWNED {
            let replacement = format!("{key}: x{ending}").into_bytes();
            assert_eq!(
                checked_edit(bytes, edit(position..position, replacement)),
                Err(FrontMatterError::Hidden)
            );
        }
    }
}

#[test]
fn owned_key_audit_accepts_ordinary_and_clean_wrapped_flows() {
    for ending in ["\n", "\r\n"] {
        for foreign in ["foreign: plain\n", "foreign: {a: 1,\nb: 2}\n"] {
            let input = format!(
                "---\n{foreign}folio-id: {ID}\nfolio-name: old\nfolio-references: {{}}\n---\n"
            )
            .replace('\n', ending);
            let bytes = input.as_bytes();
            assert_eq!(read_name(bytes).unwrap().as_deref(), Some("old"));
            assert_eq!(read_id(bytes).unwrap(), Some(FolioId::parse(ID).unwrap()));
            assert!(read_references(bytes).unwrap().is_empty());
            for edit in [
                set_name(bytes, Some("new"), false),
                set_id(bytes, Some(FolioId::parse(ID).unwrap()), false),
                set_references(bytes, &[logo()], false),
            ] {
                let output = apply_edit(bytes, &edit.unwrap().unwrap()).unwrap();
                assert!(locate(&output).unwrap().is_some());
            }
        }
    }
}
