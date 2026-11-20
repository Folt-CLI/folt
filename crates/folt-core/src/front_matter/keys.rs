use std::borrow::Cow;

use crate::config::names::OWNED;

use super::error::{FrontMatterError, Result};
use super::scalar::{meaningful, scalar, text};
use super::scan::{colon, explicit_key, line, punctuation};
use super::tree::{Block, Node};

fn owned_spelling(spelling: &str) -> Option<usize> {
    OWNED.iter().position(|key| spelling.trim() == *key)
}

pub(super) fn audit(bytes: &[u8], block: &Block) -> Result<()> {
    let mut counts = [0i32; OWNED.len()];
    for range in block.key_ranges() {
        if let Some(index) = owned_spelling(text(&bytes[range.clone()])?) {
            counts[index] += 1;
        }
    }
    for node in &block.nodes {
        if let Some(index) = owned_spelling(text(&bytes[node.key_range.clone()])?) {
            counts[index] -= 1;
        }
    }
    if counts != [0; OWNED.len()] {
        return Err(FrontMatterError::Hidden);
    }
    Ok(())
}

pub(super) fn node<'a>(block: &'a Block, key: &str) -> Result<Option<&'a Node>> {
    let nodes: Vec<_> = block.nodes.iter().filter(|n| n.key == key).collect();
    if nodes.len() > 1 {
        return Err(FrontMatterError::Duplicate);
    }
    if nodes.first().is_some_and(|n| n.ambiguous) {
        return Err(FrontMatterError::Ambiguous);
    }
    Ok(nodes.first().copied())
}

// Resolve the entire explicit key, rather than trusting the tokenizer's
// best-effort key spelling. Only fully decoded scalar keys are safe to ignore.
pub(super) fn resolved_explicit_key(bytes: &[u8], n: &Node, block: &Block) -> Result<String> {
    let ambiguous = FrontMatterError::Ambiguous;
    let spelling = text(&bytes[n.key_range.clone()])?;
    let inline = explicit_key(spelling).ok_or(ambiguous)?.trim();
    let first_line = line(bytes, n.range.start, block.body.end);
    let inline_value = colon(text(&bytes[n.range.start..first_line.end])?).is_some();
    let mut parts = Vec::new();
    if !inline.is_empty() {
        parts.push(inline);
    }
    if !inline_value {
        // The tokenizer ends this node at the next top-level key or explicit
        // value indicator. An omitted value indicator means a null value;
        // either way, only the continuation belongs to this key.
        for raw in text(&bytes[n.continuation.clone()])?.lines() {
            if !meaningful(raw) {
                continue;
            }
            if !raw.starts_with(' ') {
                return Err(ambiguous);
            }
            let end = punctuation(raw, '#').first().copied().unwrap_or(raw.len());
            let spelling = raw[..end].trim();
            if colon(spelling).is_some() {
                return Err(ambiguous);
            }
            parts.push(spelling);
        }
    }
    if parts.is_empty() || (parts.len() > 1 && parts.iter().any(|s| s.contains(['\'', '"']))) {
        return Err(ambiguous);
    }
    for part in &parts {
        // A closing quote must end the scalar. The general scalar decoder is
        // deliberately permissive about interior quotes in opaque values.
        if let Some(quote) = part.chars().next().filter(|c| matches!(c, '\'' | '"')) {
            let mut chars = part.char_indices().skip(1).peekable();
            let mut closed = false;
            while let Some((i, c)) = chars.next() {
                if quote == '"' && c == '\\' {
                    chars.next();
                } else if c == quote {
                    if quote == '\'' && chars.peek().is_some_and(|(_, c)| *c == quote) {
                        chars.next();
                    } else {
                        closed = i + c.len_utf8() == part.len();
                        break;
                    }
                }
            }
            if !closed {
                return Err(ambiguous);
            }
        } else if part.starts_with([',', ']', '}', '#', '%', '@', '`']) {
            return Err(ambiguous);
        }
        scalar(part).map_err(|_| ambiguous)?;
    }
    // Plain multiline scalars fold their line breaks into spaces.
    scalar(&parts.join(" "))
        .map(Cow::into_owned)
        .map_err(|_| ambiguous)
}

pub(super) fn unresolved_key_checks(bytes: &[u8], block: &Block) -> Result<()> {
    for n in &block.nodes {
        let spelling = text(&bytes[n.key_range.clone()])?;
        if explicit_key(spelling).is_some() {
            let key = resolved_explicit_key(bytes, n, block)?;
            if OWNED.contains(&key.as_str()) {
                return Err(FrontMatterError::Ambiguous);
            }
            continue;
        }
        let spelling = spelling.trim();
        // Scalar decoding cannot resolve YAML key properties safely.
        if spelling.starts_with(['!', '&', '*']) {
            return Err(FrontMatterError::Ambiguous);
        }
        // An undecodable quoted key could hide an owned key.
        if spelling.starts_with(['\'', '"']) && scalar(spelling).is_err() {
            return Err(FrontMatterError::Ambiguous);
        }
    }
    Ok(())
}
