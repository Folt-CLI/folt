use crate::config::limits::{SCAN_LIMIT, SCAN_WINDOW};

use super::error::{FrontMatterError, Result};
use super::keys::audit;
use super::scan::line;
use super::tree::{self, Block};

/// Finds a block only at the start (after an optional UTF-8 BOM). The closing
/// delimiter must start before byte 65536; its own bytes may cross that bound.
pub fn locate(bytes: &[u8]) -> Result<Option<Block>> {
    if bytes[..bytes.len().min(SCAN_LIMIT)].contains(&0) {
        return Err(FrontMatterError::Binary);
    }
    let start = if bytes.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    };
    let opening = line(bytes, start, bytes.len().min(SCAN_LIMIT));
    if &bytes[start
        ..bytes[start..opening.end]
            .iter()
            .rposition(|b| !matches!(b, b' ' | b'\t'))
            .map_or(start, |n| start + n + 1)]
        != b"---"
    {
        return Ok(None);
    }
    let ending = if bytes.get(opening.end..opening.next) == Some(b"\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut pos = opening.next;
    while pos < bytes.len() && pos < SCAN_LIMIT {
        // Only inspect enough bytes past the bound to finish a delimiter.
        let current = line(bytes, pos, bytes.len().min(SCAN_WINDOW));
        let content = &bytes[pos..current.end];
        if content == b"---" || content == b"..." {
            let mut block = Block {
                range: start..current.next,
                body: opening.next..pos,
                nodes: Vec::new(),
                preamble: opening.next..pos,
                line_ending: ending,
            };
            tree::build(bytes, &mut block)?;
            audit(bytes, &block)?;
            return Ok(Some(block));
        }
        pos = current.next;
    }
    Err(FrontMatterError::Unterminated)
}
