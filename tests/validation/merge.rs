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

#[test]
fn merge_large_scattered_edits_are_deterministic_and_not_whole_file() {
    const LINE_COUNT: usize = 100_000;
    const EDIT_COUNT: usize = 2_000;

    fn scattered_indices(mut state: u64) -> std::collections::BTreeSet<usize> {
        let mut indices = std::collections::BTreeSet::new();
        while indices.len() < EDIT_COUNT {
            // Fixed-seed xorshift64; keep the first and last two lines unchanged.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let index = usize::try_from(state % (LINE_COUNT as u64 - 4)).unwrap() + 2;
            indices.insert(index);
        }
        indices
    }

    let base: Vec<String> = (0..LINE_COUNT).map(|i| format!("line {i}\n")).collect();
    let mut ours = base.clone();
    let mut theirs = base.clone();
    for index in scattered_indices(0x1234_5678_9abc_def0) {
        ours[index] = format!("ours line {index}\n");
    }
    for index in scattered_indices(0xfedc_ba98_7654_3210) {
        theirs[index] = format!("theirs line {index}\n");
    }
    let base = base.concat().into_bytes();
    let ours = ours.concat().into_bytes();
    let theirs = theirs.concat().into_bytes();
    let first = merge3(&base, &ours, &theirs);
    let second = merge3(&base, &ours, &theirs);
    assert_eq!(first, second);

    match first {
        MergeResult::Clean(merged) => {
            println!("conflict region count: 0 (clean merge)");
            assert!(merged.starts_with(b"line 0\nline 1\n"));
        }
        MergeResult::Conflicted {
            merged: Some(merged),
            marker_lines,
        } => {
            let region_count = marker_lines.len() / 3;
            println!("conflict region count: {region_count}");
            assert_eq!(marker_lines.len() % 3, 0);
            assert!(region_count > 1);
            assert!(merged.starts_with(b"line 0\nline 1\n"));
            assert!(merged.ends_with(b"line 99998\nline 99999\n"));
        }
        result => panic!("expected a clean merge or complete text conflicts, got {result:?}"),
    }
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

#[test]
fn merge_diff_decision_and_removed_limit_are_recorded() {
    let manifest = include_str!("../../Cargo.toml");
    let text = include_str!("../../crates/folt-core/src/merge/text.rs");
    let module = include_str!("../../crates/folt-core/src/merge/mod.rs");
    let limits = include_str!("../../crates/folt-core/src/config/limits.rs");
    let stack = include_str!("../../docs/dev/stack.md");
    let glossary = include_str!("../../GLOSSARY.md");

    assert!(manifest.contains("similar = { version = \"=3.2.1\""));
    assert!(text.contains("Algorithm::Histogram"));
    assert!(!text.contains("deadline"));
    assert!(stack.contains("3.2.1"));
    assert!(stack.contains("Histogram"));
    for (name, source) in [
        ("limits.rs", limits),
        ("text.rs", text),
        ("mod.rs", module),
        ("stack.md", stack),
        ("GLOSSARY.md", glossary),
    ] {
        assert!(
            !source.contains("MERGE_DIFF_COST_LIMIT"),
            "{name} still contains the removed diff limit"
        );
    }
    assert!(!module.contains("budget"));
    assert!(!stack.contains("budget"));
}

#[test]
fn merge_newline_removal_and_append_report_conflicting_line_boundary() {
    assert_eq!(
        merge3(b"a\nb\n", b"a\nb", b"a\nb\nc\n"),
        MergeResult::Conflicted {
            merged: Some(b"a\n<<<<<<<\nb\n=======\nb\nc\n>>>>>>>\n".to_vec()),
            marker_lines: vec![
                b"<<<<<<<\n".to_vec(),
                b"=======\n".to_vec(),
                b">>>>>>>\n".to_vec()
            ],
        }
    );
}
