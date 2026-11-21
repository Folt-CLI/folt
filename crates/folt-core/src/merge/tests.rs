use super::*;

#[test]
fn whole_file_rules_precede_binary_detection() {
    for (base, ours, theirs, expected) in [
        (
            &b"\0base"[..],
            &b"\0base"[..],
            &b"theirs"[..],
            &b"theirs"[..],
        ),
        (b"\0base", b"ours", b"\0base", b"ours"),
        (b"\0base", b"same", b"same", b"same"),
        (b"", b"", b"", b""),
    ] {
        assert_eq!(
            merge3(base, ours, theirs),
            MergeResult::Clean(expected.to_vec())
        );
    }
}

#[test]
fn binary_probe_boundary() {
    for (offset, binary) in [(7_999, true), (8_000, false)] {
        let mut bytes = vec![b'x'; 8_001];
        bytes[offset] = 0;
        assert_eq!(is_binary(&bytes), binary);
        for inputs in [
            (&bytes[..], &b"ours\n"[..], &b"theirs\n"[..]),
            (&b"base\n"[..], &bytes[..], &b"theirs\n"[..]),
            (&b"base\n"[..], &b"ours\n"[..], &bytes[..]),
        ] {
            let MergeResult::Conflicted { merged, .. } = merge3(inputs.0, inputs.1, inputs.2)
            else {
                panic!("different inputs must conflict");
            };
            assert_eq!(merged.is_none(), binary);
        }
    }
}

#[test]
fn disjoint_and_adjacent_edits() {
    for (base, ours, theirs, expected) in [
        (
            &b"a\nb\nc\n"[..],
            &b"A\nb\nc\n"[..],
            &b"a\nb\nC\n"[..],
            &b"A\nb\nC\n"[..],
        ),
        (b"a\nb\n", b"A\nb\n", b"a\nB\n", b"A\nB\n"),
        (b"a\nb\nc\n", b"b\nc\n", b"a\nb\n", b"b\n"),
        (b"a\nb\n", b"x\na\nb\n", b"a\nb\ny\n", b"x\na\nb\ny\n"),
        (
            b"a\nb\nc\n",
            b"a\nx\nb\nc\n",
            b"a\nb\ny\nc\n",
            b"a\nx\nb\ny\nc\n",
        ),
    ] {
        assert_eq!(
            merge3(base, ours, theirs),
            MergeResult::Clean(expected.to_vec())
        );
    }
}

#[test]
fn crlf_and_unterminated_lines_survive_clean_merge() {
    assert_eq!(
        merge3(b"a\r\nb\r\nc", b"A\r\nb\r\nc", b"a\r\nb\r\nC"),
        MergeResult::Clean(b"A\r\nb\r\nC".to_vec())
    );
}

#[test]
fn same_line_edits_record_exact_marker_lines() {
    assert_eq!(
        merge3(
            b"before\nbase\nafter\n",
            b"before\nours\nafter\n",
            b"before\ntheirs\nafter\n"
        ),
        MergeResult::Conflicted {
            merged: Some(b"before\n<<<<<<<\nours\n=======\ntheirs\n>>>>>>>\nafter\n".to_vec()),
            marker_lines: vec![
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec()
            ],
        }
    );
}

#[test]
fn unterminated_conflicts_have_separate_marker_lines() {
    assert_eq!(
        merge3(b"base", b"ours", b"theirs"),
        MergeResult::Conflicted {
            merged: Some(b"<<<<<<<\nours\n=======\ntheirs\n>>>>>>>\n".to_vec()),
            marker_lines: vec![
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec()
            ],
        }
    );
}

#[test]
fn binary_conflict_has_no_merged_bytes() {
    assert_eq!(
        merge3(b"base", b"ours\0", b"theirs"),
        MergeResult::Conflicted {
            merged: None,
            marker_lines: Vec::new()
        }
    );
}

#[test]
fn identical_overlapping_edits_are_clean() {
    assert_eq!(
        merge3(b"a\nb\nc\nd\n", b"A\nb\nc\nd\n", b"A\nb\nc\nD\n"),
        MergeResult::Clean(b"A\nb\nc\nD\n".to_vec())
    );
}

#[test]
fn transitive_overlaps_keep_both_complete_regions() {
    assert_eq!(
        merge3(b"a\nb\nc\nd\ne\n", b"a\nB\nc\nD\ne\n", b"a\nother\ne\n"),
        MergeResult::Conflicted {
            merged: Some(b"a\n<<<<<<<\nB\nc\nD\n=======\nother\n>>>>>>>\ne\n".to_vec()),
            marker_lines: vec![
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec()
            ],
        }
    );
}

#[test]
fn completely_different_large_inputs_produce_valid_complete_conflict() {
    let base = b"base\n".repeat(20_000);
    let ours = b"ours\n".repeat(20_000);
    let theirs = b"theirs\n".repeat(20_000);
    let MergeResult::Conflicted {
        merged: Some(merged),
        marker_lines,
    } = merge3(&base, &ours, &theirs)
    else {
        panic!("expected text conflict");
    };
    let expected = [
        b"<<<<<<<\n",
        ours.as_slice(),
        b"=======\n",
        theirs.as_slice(),
        b">>>>>>>\n",
    ]
    .concat();
    assert_eq!(merged, expected);
    assert_eq!(
        marker_lines,
        [
            b"<<<<<<<\n".to_vec(),
            b"=======\n".to_vec(),
            b">>>>>>>\n".to_vec()
        ]
    );
}

#[test]
fn multiple_conflicts_record_marker_lines_in_file_order() {
    let MergeResult::Conflicted {
        merged: Some(merged),
        marker_lines,
    } = merge3(b"a\nb\nc\n", b"A\nb\nC\n", b"X\nb\nZ\n")
    else {
        panic!("expected two conflicts");
    };
    assert_eq!(
        merged,
        b"<<<<<<<\nA\n=======\nX\n>>>>>>>\nb\n<<<<<<<\nC\n=======\nZ\n>>>>>>>\n"
    );
    assert_eq!(
        marker_lines,
        [b"<<<<<<<\n", b"=======\n", b">>>>>>>\n"]
            .repeat(2)
            .into_iter()
            .map(|line| line.to_vec())
            .collect::<Vec<_>>()
    );
}

#[test]
fn differing_insertions_at_the_same_location_conflict() {
    assert_eq!(
        merge3(b"a\n", b"ours\na\n", b"theirs\na\n"),
        MergeResult::Conflicted {
            merged: Some(b"<<<<<<<\nours\n=======\ntheirs\n>>>>>>>\na\n".to_vec()),
            marker_lines: vec![
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec()
            ],
        }
    );
}

#[test]
fn removing_final_newline_conflicts_with_boundary_append() {
    for (base, shortened, appended, forward, reverse) in [
        (
            &b"a\nb\n"[..],
            &b"a\nb"[..],
            &b"a\nb\nc\n"[..],
            &b"a\n<<<<<<<\nb\n=======\nb\nc\n>>>>>>>\n"[..],
            &b"a\n<<<<<<<\nb\nc\n=======\nb\n>>>>>>>\n"[..],
        ),
        (
            &b"a\r\nb\r\n"[..],
            &b"a\r\nb"[..],
            &b"a\r\nb\r\nc\r\n"[..],
            &b"a\r\n<<<<<<<\nb\n=======\nb\r\nc\r\n>>>>>>>\n"[..],
            &b"a\r\n<<<<<<<\nb\r\nc\r\n=======\nb\n>>>>>>>\n"[..],
        ),
    ] {
        for (ours, theirs, expected) in [
            (shortened, appended, forward),
            (appended, shortened, reverse),
        ] {
            assert_eq!(
                merge3(base, ours, theirs),
                MergeResult::Conflicted {
                    merged: Some(expected.to_vec()),
                    marker_lines: vec![
                        b"<<<<<<<\n".to_vec(),
                        b"=======\n".to_vec(),
                        b">>>>>>>\n".to_vec()
                    ],
                }
            );
        }
    }
}

#[test]
fn terminated_replacement_and_boundary_append_remain_clean() {
    for (ours, theirs) in [
        (&b"a\nB\n"[..], &b"a\nb\nc\n"[..]),
        (&b"a\nb\nc\n"[..], &b"a\nB\n"[..]),
    ] {
        assert_eq!(
            merge3(b"a\nb\n", ours, theirs),
            MergeResult::Clean(b"a\nB\nc\n".to_vec())
        );
    }
}
