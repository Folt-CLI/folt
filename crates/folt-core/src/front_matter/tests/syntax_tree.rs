use super::*;

#[test]
fn scenario_1_plain_node_byte_spans() {
    let block = locate(b"---\nkey: value # note\n---\n").unwrap().unwrap();
    assert_eq!(block.body, 4..22);
    assert_eq!(block.nodes.len(), 1);
    let n = &block.nodes[0];
    assert_eq!(n.key, "key");
    assert_eq!(n.key_range, 4..7);
    assert_eq!(n.value, 9..14);
    assert_eq!(n.comment, 15..21);
    assert_eq!(n.range, 4..22);
    assert_eq!(n.continuation, 22..22);
    assert!(!n.ambiguous);
}

#[test]
fn scenario_2_quoted_keys_decode_and_remain_ambiguous() {
    for (spelling, expected) in [
        ("'key'", "key"),
        ("\"key\"", "key"),
        ("'it''s'", "it's"),
        (r#""key\tname""#, "key\tname"),
    ] {
        let input = format!("---\n{spelling}: value\n---\n");
        let block = locate(input.as_bytes()).unwrap().unwrap();
        let n = &block.nodes[0];
        assert_eq!(n.key, expected);
        assert_eq!(n.key_range, 4..4 + spelling.len());
        assert!(n.ambiguous);
    }
}

#[test]
fn scenario_3_explicit_keys_resolve_space_tab_and_end_of_line() {
    for spelling in ["? key", "?\tkey", "?\n  key"] {
        let input = format!("---\n{spelling}\n: value\n---\n");
        let block = locate(input.as_bytes()).unwrap().unwrap();
        assert_eq!(block.nodes[0].key, "key");
        assert!(block.nodes[0].ambiguous);
    }
}

#[test]
fn scenario_4_wrapped_flow_spans_end_at_final_brace() {
    let input = "---\nkey: {a: {b: 1},\nc: 2\n} # final\n# after\nnext: x\n---\n";
    let block = locate(input.as_bytes()).unwrap().unwrap();
    assert_eq!(block.nodes.len(), 2);
    let n = &block.nodes[0];
    let close = input.find("} # final").unwrap() + 1;
    let after = input.find("# after").unwrap();
    let next = input.find("next:").unwrap();
    assert_eq!(n.value, 9..close);
    assert_eq!(n.comment, close..after - 1);
    assert_eq!(n.continuation, after..next);
    assert_eq!(n.range, 4..next);
    assert_eq!(&input[n.value.clone()], "{a: {b: 1},\nc: 2\n}");
}

#[test]
fn scenario_5_preamble_and_trailing_comments_keep_their_spans() {
    let input = "---\n# before\n\nkey: value   # trailing\n---\n";
    let block = locate(input.as_bytes()).unwrap().unwrap();
    assert_eq!(block.preamble, 4..14);
    assert_eq!(&input[block.preamble], "# before\n\n");
    let n = &block.nodes[0];
    assert_eq!(n.key_range, 14..17);
    assert_eq!(n.value, 19..24);
    assert_eq!(n.comment, 27..37);
    assert_eq!(&input[n.value.clone()], "value");
}

#[test]
fn scenario_6_crlf_shifts_all_spans_by_preceding_extra_bytes() {
    let lf = "---\n# before\n\nkey: {a: 1,\nb: 2} # comment\nnext: x\n---\nbody";
    let crlf = lf.replace('\n', "\r\n");
    let plain = locate(lf.as_bytes()).unwrap().unwrap();
    let windows = locate(crlf.as_bytes()).unwrap().unwrap();
    let shift = |offset: usize| {
        offset
            + lf.as_bytes()[..offset]
                .iter()
                .filter(|b| **b == b'\n')
                .count()
    };
    let shifted = |range: &Range<usize>| shift(range.start)..shift(range.end);
    assert_eq!(plain.line_ending, "\n");
    assert_eq!(windows.line_ending, "\r\n");
    assert_eq!(windows.range, shifted(&plain.range));
    assert_eq!(windows.body, shifted(&plain.body));
    assert_eq!(windows.preamble, shifted(&plain.preamble));
    assert_eq!(windows.nodes.len(), plain.nodes.len());
    for (original, converted) in plain.nodes.iter().zip(&windows.nodes) {
        assert_eq!(converted.key, original.key);
        assert_eq!(converted.ambiguous, original.ambiguous);
        for (old, new) in [
            (&original.range, &converted.range),
            (&original.key_range, &converted.key_range),
            (&original.value, &converted.value),
            (&original.comment, &converted.comment),
            (&original.continuation, &converted.continuation),
        ] {
            assert_eq!(*new, shifted(old));
        }
    }
}

#[test]
fn scenario_7_bom_preserves_existing_block_and_body_offsets() {
    let input = b"\xef\xbb\xbf---\nkey: value\n---\nbody";
    let block = locate(input).unwrap().unwrap();
    // Existing ranges start after the BOM, including the block's range.
    assert_eq!(block.range, 3..22);
    assert_eq!(block.body, 7..18);
    assert_eq!(block.nodes[0].key_range, 7..10);
    assert_eq!(block.nodes[0].value, 12..17);
}

#[test]
fn scenario_8_closing_delimiter_may_cross_but_not_start_at_scan_limit() {
    for delimiter in ["---", "..."] {
        for offset in [SCAN_LIMIT - 1, SCAN_LIMIT, SCAN_LIMIT + 1] {
            let input = format!("---\n#{}\n{delimiter}\n", "x".repeat(offset - 6));
            let result = locate(input.as_bytes());
            if offset < SCAN_LIMIT {
                let block = result.unwrap().unwrap();
                assert_eq!(block.body.end, offset);
                assert_eq!(block.range.end, offset + 4);
            } else {
                assert_eq!(result, Err(FrontMatterError::Unterminated));
            }
        }
    }
}

#[test]
fn scenario_9_name_results_remain_unchanged() {
    for (input, expected) in [
        ("body", None),
        ("---\nother: value\n---\n", None),
        ("---\nfolio-name: plain\n---\n", Some("plain")),
        ("---\nfolio-name: 'it''s'\n---\n", Some("it's")),
        (
            "---\nfolio-name: \"a\\n\\t\\r\\\"\\\\b\"\n---\n",
            Some("a\n\t\r\"\\b"),
        ),
        ("---\nother:\n  folio-name: nested\n---\n", None),
    ] {
        assert_eq!(read_name(input.as_bytes()).unwrap().as_deref(), expected);
    }
}

#[test]
fn scenarios_9_and_10_scalar_errors_keep_the_same_order() {
    use FrontMatterError::{
        Ambiguous, Binary, Duplicate, Hidden, InvalidValue, Unsupported, Unterminated,
    };

    for key in ["folio-name", "folio-id"] {
        for (body, expected) in [
            (format!("{key}:\n"), InvalidValue),
            (format!("{key}: >\n  folded\n"), Unsupported),
            (format!("{key}: plain\n  multiline\n"), Unsupported),
            (format!("{key}: invalid\n{key}: invalid\n"), Duplicate),
            (format!("'{key}': invalid\n"), Ambiguous),
            (format!("flow: {{a: 1,\n{key}: invalid\n}}\n"), Hidden),
            (format!("{key}: invalid\n\tcontinuation\n"), Unsupported),
            // Key checks precede duplicate detection and scalar validation.
            (
                format!("? {key}\n{key}: invalid\n{key}: invalid\n"),
                Ambiguous,
            ),
            (format!("'{key}': invalid\n{key}: invalid\n"), Duplicate),
            // Supported constructs are checked before empty scalar values.
            (format!("{key}:\n\tcontinuation\n"), Unsupported),
            (format!("{key}:\n\0"), Binary),
        ] {
            let input = format!("---\n{body}---\n");
            let result = if key == "folio-name" {
                read_name(input.as_bytes()).map(|_| ())
            } else {
                read_id(input.as_bytes()).map(|_| ())
            };
            assert_eq!(result, Err(expected), "{input:?}");
        }
        for (input, expected) in [
            (format!("---\n{key}: invalid\n"), Unterminated),
            (format!("---\n{key}: invalid\n\0"), Binary),
        ] {
            assert_eq!(read_name(input.as_bytes()), Err(expected));
            assert_eq!(read_id(input.as_bytes()), Err(expected));
        }
    }
}

#[test]
fn scenario_10_ids_require_canonical_lowercase_hyphenated_form() {
    assert_eq!(read_id(b"body").unwrap(), None);
    assert_eq!(read_id(b"---\nother: x\n---\n").unwrap(), None);
    for spelling in [ID.to_owned(), format!("'{ID}'"), format!("\"{ID}\"")] {
        let input = format!("---\nfolio-id: {spelling}\n---\n");
        assert_eq!(
            read_id(input.as_bytes()).unwrap(),
            Some(FolioId::parse(ID).unwrap())
        );
    }
    for invalid in [
        ID.to_uppercase(),
        "12345678".to_owned(),
        "not-a-uuid".to_owned(),
    ] {
        let input = format!("---\nfolio-id: {invalid}\n---\n");
        assert_eq!(
            read_id(input.as_bytes()),
            Err(FrontMatterError::InvalidValue)
        );
    }
}
