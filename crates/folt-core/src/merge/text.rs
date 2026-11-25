use std::ops::Range;
use std::time::Instant;

use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};

use crate::config::limits::MERGE_DIFF_COST_LIMIT;

use super::{MergeResult, render::Render};

struct Change<'a> {
    range: Range<usize>,
    lines: Vec<&'a [u8]>,
    side: usize,
}

fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|b| *b == b'\n').collect()
}

fn changes<'a>(base: &[&[u8]], side: &[&'a [u8]], index: usize) -> Vec<Change<'a>> {
    // similar 3.2.1 Myers falls back to delete + insert for remaining ranges
    // when its search deadline is reached. Capture/Replace therefore
    // returns a complete valid diff, though it may be less precise. Compaction
    // and preprocessing are outside the search deadline: it is not a hard cap.
    capture_diff_slices_deadline(
        Algorithm::Myers,
        base,
        side,
        Instant::now().checked_add(MERGE_DIFF_COST_LIMIT),
    )
    .into_iter()
    .filter_map(|op| {
        let (tag, old, new) = op.as_tag_tuple();
        (tag != DiffTag::Equal).then(|| Change {
            range: old,
            lines: side[new].to_vec(),
            side: index,
        })
    })
    .collect()
}

pub(super) fn merge(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeResult {
    let base = lines(base);
    let mut edits = changes(&base, &lines(ours), 0);
    edits.extend(changes(&base, &lines(theirs), 1));
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut output = Render::default();
    let mut cursor = 0;
    let mut index = 0;
    while index < edits.len() {
        let start = edits[index].range.start;
        output.bytes.extend(base[cursor..start].concat());
        let mut end = edits[index].range.end;
        let mut next = index + 1;
        // Adjacent edits remain independent unless a boundary insertion would
        // join its first line to an unterminated replacement on the other side.
        while next < edits.len()
            && (edits[next].range.start < end
                || edits[next].range.start == start
                || (edits[next].range.start == end
                    && conflicts_at_boundary(&edits[index..next], &edits[next])))
        {
            end = end.max(edits[next].range.end);
            next += 1;
        }
        let region = &edits[index..next];
        let ours = apply(&base, start..end, region, 0);
        let theirs = apply(&base, start..end, region, 1);
        if ours == theirs || !region.iter().any(|edit| edit.side == 1) {
            output.bytes.extend(ours);
        } else if !region.iter().any(|edit| edit.side == 0) {
            output.bytes.extend(theirs);
        } else {
            output.conflict(&ours, &theirs);
        }
        cursor = end;
        index = next;
    }
    output.bytes.extend(base[cursor..].concat());
    if output.marker_lines.is_empty() {
        MergeResult::Clean(output.bytes)
    } else {
        MergeResult::Conflicted {
            merged: Some(output.bytes),
            marker_lines: output.marker_lines,
        }
    }
}

fn apply(base: &[&[u8]], range: Range<usize>, edits: &[Change<'_>], side: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut cursor = range.start;
    for edit in edits.iter().filter(|edit| edit.side == side) {
        bytes.extend(base[cursor..edit.range.start].concat());
        bytes.extend(edit.lines.concat());
        cursor = edit.range.end;
    }
    bytes.extend(base[cursor..range.end].concat());
    bytes
}

fn conflicts_at_boundary(region: &[Change<'_>], insertion: &Change<'_>) -> bool {
    insertion.range.is_empty()
        && region.iter().any(|edit| {
            edit.side != insertion.side
                && edit.range.end == insertion.range.start
                && edit.lines.last().is_some_and(|line| !line.ends_with(b"\n"))
        })
}
