use super::*;

#[test]
fn preserves_bytes_and_owned_line_trivia() {
    for ending in ["\n", "\r\n"] {
        let bytes = format!("\u{feff}---{ending}title: |{ending}  ---{ending}tags:{ending}- a{ending}- b{ending}folio-name  :   'old'   # keep{ending}# separate{ending}other: yes{ending}...{ending}body\0").into_bytes();
        // Binary detection covers the prefix, including the body.
        assert_eq!(locate(&bytes), Err(FrontMatterError::Binary));
        let bytes = &bytes[..bytes.len() - 1];
        let result = rename(bytes, "new");
        let expected = text(bytes).unwrap().replace("'old'", "\"new\"");
        assert_eq!(result, expected.as_bytes());
        assert_eq!(rename(&result, "new"), result);
        let block = locate(bytes).unwrap().unwrap();
        assert_eq!(block.nodes[1].key, "tags");
        assert!(
            text(&bytes[block.nodes[1].range.clone()])
                .unwrap()
                .contains("- b")
        );
    }
    let bytes = b"---\r\nforeign: |\n  ---\r\nfolio-name: old # c\nother: x\r\n---\nbody\r\n";
    assert_eq!(
        rename(bytes, "new"),
        b"---\r\nforeign: |\n  ---\r\nfolio-name: \"new\" # c\nother: x\r\n---\nbody\r\n"
    );
}

#[test]
fn edits_keep_the_closing_delimiter_within_the_scan_limit() {
    let bytes = format!("---\n#{}\nfolio-name: \"x\"\n---\n", "a".repeat(65513));
    assert_eq!(locate(bytes.as_bytes()).unwrap().unwrap().body.end, 65535);
    assert_eq!(
        locate(bytes.as_bytes()).and_then(|block| Block::set_name(
            block.as_ref(),
            bytes.as_bytes(),
            Some("xx"),
            false
        )),
        Err(FrontMatterError::Unterminated)
    );
    let unchanged = rename(bytes.as_bytes(), "x");
    assert_eq!(unchanged, bytes.as_bytes());
    assert_eq!(read_name(&unchanged).unwrap().as_deref(), Some("x"));

    fn at_boundary(body: &str) -> Vec<u8> {
        format!(
            "---\n#{}\n{body}---\n",
            "a".repeat(SCAN_LIMIT - 7 - body.len())
        )
        .into_bytes()
    }
    let id = FolioId::parse(ID).unwrap();
    for body in ["", "folio-id: 12345678-1234-4234-8234-123456789abc\n"] {
        let bytes = at_boundary(body);
        assert_eq!(
            locate(&bytes).and_then(|block| Block::set_name(
                block.as_ref(),
                &bytes,
                Some("new"),
                false
            )),
            Err(FrontMatterError::Unterminated)
        );
    }
    let bytes = at_boundary("");
    assert_eq!(
        locate(&bytes).and_then(|block| Block::set_id(block.as_ref(), &bytes, Some(id), false)),
        Err(FrontMatterError::Unterminated)
    );
    for body in [
        "",
        "folio-references: {}\n",
        "folio-references: {logo: {name: x}}\n",
        "folio-references:\n  logo: {name: x}\n",
        "folio-references:\n  logo:\n    name: x\n",
    ] {
        let bytes = at_boundary(body);
        assert_eq!(
            locate(&bytes).and_then(|block| Block::set_references(
                block.as_ref(),
                &bytes,
                &[logo()],
                false
            )),
            Err(FrontMatterError::Unterminated),
            "{body}"
        );
        if !body.contains("    name:") {
            for alias in ["logo", "other"] {
                let mut reference = logo();
                reference.alias = alias.into();
                assert_eq!(
                    locate(&bytes).and_then(|block| Block::set_reference(
                        block.as_ref(),
                        &bytes,
                        alias,
                        Some(&reference),
                        false
                    )),
                    Err(FrontMatterError::Unterminated),
                    "{body}: {alias}"
                );
            }
        }
    }
    assert_eq!(
        locate(b"body").and_then(|block| Block::set_name(
            block.as_ref(),
            b"body",
            Some(&"x".repeat(SCAN_LIMIT)),
            true
        )),
        Err(FrontMatterError::Unterminated)
    );
}

#[test]
fn enforces_scan_boundary_and_delimiters() {
    for (offset, success) in [(SCAN_LIMIT - 1, true), (SCAN_LIMIT, false)] {
        let mut bytes = b"---\n#".to_vec();
        bytes.resize(offset - 1, b'x');
        bytes.extend_from_slice(b"\n---\nbody");
        assert_eq!(locate(&bytes).is_ok(), success);
    }
    assert!(locate(b" ---\n---\n").unwrap().is_none());
    assert!(locate(b"--- \t\r\n...\r\n").unwrap().is_some());
    assert_eq!(locate(b"---\n  ---\n"), Err(FrontMatterError::Unterminated));
    assert_eq!(locate(b"---\rfolio-name: a\r---"), Ok(None));
    assert_eq!(locate(b"---\r"), Ok(None));
    assert_eq!(locate(b"---\x0b\n---\n"), Ok(None));
}

#[test]
fn creates_inserts_and_removes() {
    assert!(
        locate(b"body")
            .and_then(|block| Block::set_name(block.as_ref(), b"body", Some("new"), false))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        rename(b"\xef\xbb\xbfbody", "new"),
        b"\xef\xbb\xbf---\nfolio-name: \"new\"\n---\nbody"
    );
    assert_eq!(
        rename(b"---\n---\nbody", "new"),
        b"---\nfolio-name: \"new\"\n---\nbody"
    );
    let input = b"---\r\nforeign: a\n---\r\nbody";
    assert_eq!(
        rename(input, "new"),
        b"---\r\nforeign: a\nfolio-name: \"new\"\r\n---\r\nbody"
    );
    let input = format!("---\nfolio-id: {ID}\nforeign: a\n---\n");
    let output = rename(input.as_bytes(), "new");
    assert!(
        text(&output)
            .unwrap()
            .contains("folio-name: \"new\"\nforeign:")
    );
    assert_eq!(read_id(&output).unwrap().unwrap().to_string(), ID);
    let id = FolioId::parse(ID).unwrap();
    let output = apply_edit(
        b"---\n---\n",
        &locate(b"---\n---\n")
            .and_then(|block| Block::set_id(block.as_ref(), b"---\n---\n", Some(id), false))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(read_id(&output).unwrap(), Some(id));
    assert_eq!(
        read_id(b"---\nfolio-id: INVALID\n---\n"),
        Err(FrontMatterError::InvalidValue)
    );
    for input in [
        b"---\nfolio-references:\n  logo: {name: image}\n---\nbody".as_slice(),
        b"---\nfolio-references: {logo: {name: image}}\n---\nbody",
    ] {
        let output = apply_edit(
            input,
            &locate(input)
                .and_then(|block| Block::set_reference(block.as_ref(), input, "logo", None, false))
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(read_references(&output).unwrap().is_empty());
        assert!(text(&output).unwrap().contains("folio-references:"));
        assert!(output.ends_with(b"---\nbody"));
    }
    assert_eq!(
        apply_edit(b"x", &edit(2..3, Vec::new())),
        Err(FrontMatterError::InvalidValue)
    );
}
