use folt_core::merge::{MergeResult, merge3};
use proptest::prelude::*;

// Scenario 1: arbitrary bytes, including binary content, obey the first rule.
proptest! {
    #![proptest_config(ProptestConfig { cases: 64, rng_seed: proptest::test_runner::RngSeed::Fixed(158), failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn merge_ours_equal_merge_base_returns_theirs(
        base in prop::collection::vec(any::<u8>(), 0..512),
        theirs in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        prop_assert_eq!(merge3(&base, &base, &theirs), MergeResult::Clean(theirs));
    }
}

#[test]
fn merge_whole_file_rules_and_binary_conflict() {
    assert_eq!(
        merge3(b"\0", b"ours", b"\0"),
        MergeResult::Clean(b"ours".to_vec())
    );
    assert_eq!(
        merge3(b"\0", b"same", b"same"),
        MergeResult::Clean(b"same".to_vec())
    );
    assert_eq!(
        merge3(b"base", b"ours\0", b"theirs"),
        MergeResult::Conflicted {
            merged: None,
            marker_lines: Vec::new(),
        }
    );
}

#[test]
fn merge_lines_preserve_crlf_and_record_conflicts() {
    assert_eq!(
        merge3(b"a\r\nb\r\n", b"A\r\nb\r\n", b"a\r\nB\r\n"),
        MergeResult::Clean(b"A\r\nB\r\n".to_vec())
    );
    assert_eq!(
        merge3(b"base\r\n", b"ours\r\n", b"theirs\r\n"),
        MergeResult::Conflicted {
            merged: Some(b"<<<<<<<\nours\r\n=======\ntheirs\r\n>>>>>>>\n".to_vec()),
            marker_lines: vec![
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec()
            ],
        }
    );
}

#[test]
fn merge_large_different_inputs_complete_with_all_bytes() {
    let base = b"base\n".repeat(20_000);
    let ours = b"ours\n".repeat(20_000);
    let theirs = b"theirs\n".repeat(20_000);
    let MergeResult::Conflicted {
        merged: Some(merged),
        marker_lines,
    } = merge3(&base, &ours, &theirs)
    else {
        panic!("expected a complete text conflict");
    };
    assert_eq!(
        merged,
        [
            b"<<<<<<<\n",
            ours.as_slice(),
            b"=======\n",
            theirs.as_slice(),
            b">>>>>>>\n"
        ]
        .concat()
    );
    assert_eq!(marker_lines.len(), 3);
}

// Scenarios 3–5: the public API is compiled with the normal dependency, and
// its dependency selection, vocabulary and module ownership are documented.
#[test]
fn merge_dependency_terms_and_ownership_are_documented() {
    let manifest = include_str!("../../crates/folt-core/Cargo.toml");
    assert!(
        manifest
            .split("[dependencies]")
            .nth(1)
            .unwrap()
            .split("[lints]")
            .next()
            .unwrap()
            .contains("similar.workspace = true")
    );
    let stack = include_str!("../../docs/dev/stack.md");
    assert!(stack.contains("`similar`"));
    assert!(stack.contains("Apache-2.0"));
    let glossary = include_str!("../../GLOSSARY.md");
    assert!(glossary.contains("### three-way merge"));
    assert!(glossary.contains("### conflict marker"));
    assert!(include_str!("../../docs/dev/library/layers.md").contains("| L1 | merge | config |"));
    assert!(include_str!("../../docs/dev/library/modules.md").contains("| `merge` |"));
}
