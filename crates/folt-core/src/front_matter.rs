//! Lossless, bounded front matter inspection and byte-span edits.
//!
//! This is a deliberately small YAML subset, not a general YAML parser. Foreign
//! nodes remain opaque. An edit is valid only for the bytes it was planned on;
//! plan it again after applying another edit. Names are written as double-quoted
//! strings; ids remain unquoted.
use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::config::limits::{SCAN_LIMIT, SCAN_WINDOW};
use crate::config::names::{OWNED, OWNED_ID, OWNED_NAME, OWNED_REFERENCES};
use crate::id::FolioId;

mod edit;
mod scan;
mod tree;

use scan::{colon, explicit_key, line, punctuation};
pub use tree::{Block, Node};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontMatterError {
    Unterminated,
    Binary,
    Duplicate,
    Ambiguous,
    Hidden,
    Unsupported,
    InvalidValue,
}
impl fmt::Display for FrontMatterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "front matter: {}",
            match self {
                Self::Unterminated => "no closing delimiter within 64 KiB",
                Self::Binary => "NUL in input prefix",
                Self::Duplicate => "duplicate key",
                Self::Ambiguous => "quoted or complex owned key",
                Self::Hidden => "an owned key is hidden inside another node",
                Self::Unsupported => "unsupported YAML construct",
                Self::InvalidValue => "invalid value",
            }
        )
    }
}
impl Error for FrontMatterError {}
type Result<T> = std::result::Result<T, FrontMatterError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: Vec<u8>,
}

/// Applies one edit without I/O. Rejects an out-of-bounds or reversed range.
pub fn apply_edit(bytes: &[u8], edit: &Edit) -> Result<Vec<u8>> {
    if edit.range.start > edit.range.end || edit.range.end > bytes.len() {
        return Err(FrontMatterError::InvalidValue);
    }
    let mut result = Vec::with_capacity(bytes.len() - edit.range.len() + edit.replacement.len());
    result.extend_from_slice(&bytes[..edit.range.start]);
    result.extend_from_slice(&edit.replacement);
    result.extend_from_slice(&bytes[edit.range.end..]);
    Ok(result)
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| FrontMatterError::InvalidValue)
}
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

fn owned_spelling(spelling: &str) -> Option<usize> {
    OWNED.iter().position(|key| spelling.trim() == *key)
}

fn audit(bytes: &[u8], block: &Block) -> Result<()> {
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

fn node<'a>(block: &'a Block, key: &str) -> Result<Option<&'a Node>> {
    let nodes: Vec<_> = block.nodes.iter().filter(|n| n.key == key).collect();
    if nodes.len() > 1 {
        return Err(FrontMatterError::Duplicate);
    }
    if nodes.first().is_some_and(|n| n.ambiguous) {
        return Err(FrontMatterError::Ambiguous);
    }
    Ok(nodes.first().copied())
}
fn supported(bytes: &[u8], node: &Node) -> Result<()> {
    let rest = text(&bytes[node.value.start..node.range.end])?;
    for l in rest.lines().skip(1) {
        if l.starts_with('\t') || l.trim_start_matches(' ').starts_with('\t') {
            return Err(FrontMatterError::Unsupported);
        }
    }
    // YAML properties are meaningful only as tokens, never inside quotes.
    for token in ['&', '*', '!'] {
        if !punctuation(rest, token).is_empty() {
            return Err(FrontMatterError::Unsupported);
        }
    }
    Ok(())
}
fn scalar(s: &str) -> Result<Cow<'_, str>> {
    let s = s.trim();
    if s.starts_with(['&', '*', '!', '|', '>', '[', '{']) {
        return Err(FrontMatterError::Unsupported);
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(Cow::Owned(inner.replace("''", "'")));
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            out.push(if c == '\\' {
                match chars.next() {
                    Some('"') => '"',
                    Some('\\') => '\\',
                    Some('n') => '\n',
                    Some('t') => '\t',
                    Some('r') => '\r',
                    _ => return Err(FrontMatterError::InvalidValue),
                }
            } else {
                c
            });
        }
        return Ok(Cow::Owned(out));
    }
    if s.contains(['\n', '\r', '\'', '"'])
        || s.starts_with(['?', ':'])
        || s.strip_prefix('-')
            .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
    {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(Cow::Borrowed(s))
}
fn meaningful(s: &str) -> bool {
    !s.trim().is_empty() && !s.trim_start().starts_with('#')
}
fn read_scalar<'a>(bytes: &'a [u8], key: &str) -> Result<Option<Cow<'a, str>>> {
    let Some(block) = locate(bytes)? else {
        return Ok(None);
    };
    unresolved_key_checks(bytes, &block)?;
    let Some(n) = node(&block, key)? else {
        return Ok(None);
    };
    supported(bytes, n)?;
    let continuation = text(&bytes[n.continuation.clone()])?;
    if continuation.lines().any(meaningful) {
        return Err(FrontMatterError::Unsupported);
    }
    let spelling = text(&bytes[n.value.clone()])?;
    let value = scalar(spelling)?;
    if value.is_empty() {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(Some(value))
}

/// Reads a scalar name, decoding minimal single/double quoted forms.
pub fn read_name(bytes: &[u8]) -> Result<Option<Cow<'_, str>>> {
    read_scalar(bytes, OWNED_NAME)
}
/// Reads the canonical lowercase hyphenated Folio id.
pub fn read_id(bytes: &[u8]) -> Result<Option<FolioId>> {
    read_scalar(bytes, OWNED_ID)?
        .map(|s| FolioId::parse(&s).map_err(|_| FrontMatterError::InvalidValue))
        .transpose()
}

/// An alias with exactly one of name or canonical id, and an optional opaque
/// pin. Name grammar and commit validation belong to callers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference<'a> {
    pub alias: Cow<'a, str>,
    pub name: Option<Cow<'a, str>>,
    pub id: Option<FolioId>,
    pub pin: Option<Cow<'a, str>>,
}
struct ReferenceNode<'a> {
    reference: Reference<'a>,
    range: Range<usize>,
    key_range: Range<usize>,
    value: Range<usize>,
}
// Flow whitespace may include comments through their line ending. Keep slices
// into the original input so borrowed scalars and edit offsets stay valid.
fn skip_flow_space(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if !s.starts_with('#') {
            return s;
        }
        s = s.split_once('\n').map_or(&s[s.len()..], |(_, tail)| tail);
    }
}
fn flow_token(s: &str) -> Result<&str> {
    let s = skip_flow_space(s);
    let end = punctuation(s, '#').first().copied().unwrap_or(s.len());
    if !skip_flow_space(&s[end..]).is_empty() {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(s[..end].trim_end())
}
fn mapping(s: &str) -> Result<Vec<(&str, &str)>> {
    let inner = s
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or(FrontMatterError::InvalidValue)?;
    let mut starts = vec![0];
    starts.extend(punctuation(inner, ',').into_iter().map(|n| n + 1));
    let mut result = Vec::new();
    for (i, start) in starts.iter().copied().enumerate() {
        let end = starts.get(i + 1).map_or(inner.len(), |n| n - 1);
        let part = &inner[start..end];
        let clean = skip_flow_space(part);
        if clean.is_empty() {
            continue;
        }
        let c = colon(clean).ok_or(FrontMatterError::InvalidValue)?;
        result.push((flow_token(&clean[..c])?, flow_token(&clean[c + 1..])?));
    }
    Ok(result)
}
fn reference<'a>(
    alias: &'a str,
    value: &'a str,
    fields: Option<Vec<(&'a str, &'a str)>>,
) -> Result<Reference<'a>> {
    let alias = scalar(alias)?;
    if alias.is_empty() {
        return Err(FrontMatterError::InvalidValue);
    }
    let mut result = Reference {
        alias,
        name: None,
        id: None,
        pin: None,
    };
    let fields = if let Some(fields) = fields {
        fields
    } else if value.trim_start().starts_with('{') {
        mapping(value)?
    } else {
        scalar(value)?;
        return Err(FrontMatterError::InvalidValue);
    };
    let mut seen = Vec::new();
    for (key, value) in fields {
        if seen.contains(&key) {
            return Err(FrontMatterError::Duplicate);
        }
        seen.push(key);
        let value = scalar(value)?;
        if value.is_empty() {
            return Err(FrontMatterError::InvalidValue);
        }
        match key {
            "name" => result.name = Some(value),
            "id" => {
                result.id =
                    Some(FolioId::parse(&value).map_err(|_| FrontMatterError::InvalidValue)?)
            }
            "pin" => result.pin = Some(value),
            _ => return Err(FrontMatterError::Unsupported),
        }
    }
    if result.name.is_some() == result.id.is_some() {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(result)
}
fn references<'a>(bytes: &'a [u8], n: &Node) -> Result<Vec<ReferenceNode<'a>>> {
    supported(bytes, n)?;
    let mut result = Vec::new();
    for entry in tree::entries(bytes, n)? {
        let entry = entry?;
        result.push(ReferenceNode {
            reference: reference(
                text(&bytes[entry.key_range.clone()])?,
                text(&bytes[entry.value.clone()])?,
                entry.fields,
            )?,
            range: entry.range,
            key_range: entry.key_range,
            value: entry.value,
        });
    }
    let mut aliases = Vec::new();
    for entry in &result {
        if aliases.contains(&entry.reference.alias.as_ref()) {
            return Err(FrontMatterError::Duplicate);
        }
        aliases.push(entry.reference.alias.as_ref());
    }
    Ok(result)
}
/// Reads block or flow reference mappings; absent metadata yields no entries.
pub fn read_references(bytes: &[u8]) -> Result<Vec<Reference<'_>>> {
    let Some(block) = locate(bytes)? else {
        return Ok(Vec::new());
    };
    unresolved_key_checks(bytes, &block)?;
    let Some(n) = node(&block, OWNED_REFERENCES)? else {
        return Ok(Vec::new());
    };
    Ok(references(bytes, n)?
        .into_iter()
        .map(|n| n.reference)
        .collect())
}

// Resolve the entire explicit key, rather than trusting the tokenizer's
// best-effort key spelling. Only fully decoded scalar keys are safe to ignore.
fn resolved_explicit_key(bytes: &[u8], n: &Node, block: &Block) -> Result<String> {
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

fn unresolved_key_checks(bytes: &[u8], block: &Block) -> Result<()> {
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

#[cfg(test)]
mod tests;
