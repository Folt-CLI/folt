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

mod scan;
mod tree;

use scan::{colon, explicit_key, line, punctuation, scan_punctuation};
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

fn edit_checks(bytes: &[u8], block: &Block) -> Result<()> {
    unresolved_key_checks(bytes, block)?;
    for n in &block.nodes {
        let spelling = text(&bytes[n.key_range.clone()])?;
        if explicit_key(spelling).is_some() {
            continue;
        }
        let spelling = spelling.trim();
        // Keep escaped quoted spellings opaque when editing foreign nodes.
        if spelling.starts_with(['\'', '"']) && spelling.contains('\\') {
            return Err(FrontMatterError::Ambiguous);
        }
    }
    for key in OWNED {
        if let Some(n) = node(block, key)? {
            supported(bytes, n)?;
        }
    }
    Ok(())
}
fn edit(range: Range<usize>, replacement: impl Into<Vec<u8>>) -> Edit {
    Edit {
        range,
        replacement: replacement.into(),
    }
}
// Replacing a value must retain YAML separation even when its original span
// started immediately after the colon (for example, an empty block header).
fn value_edit(bytes: &[u8], range: Range<usize>, value: impl AsRef<[u8]>) -> Edit {
    let mut replacement = Vec::new();
    if range.start > 0 && bytes[range.start - 1] == b':' {
        replacement.push(b' ');
    }
    replacement.extend_from_slice(value.as_ref());
    if bytes.get(range.end) == Some(&b'#') {
        replacement.push(b' ');
    }
    edit(range, replacement)
}
// Validate only the bounded prefix of the virtual result, including enough
// bytes past the scan limit to finish a closing delimiter.
fn checked_edit(bytes: &[u8], edit: Edit) -> Result<Option<Edit>> {
    let prefix: Vec<_> = bytes[..edit.range.start]
        .iter()
        .chain(&edit.replacement)
        .chain(&bytes[edit.range.end..])
        .take(SCAN_WINDOW)
        .copied()
        .collect();
    locate(&prefix)?;
    Ok(Some(edit))
}

fn insert(
    bytes: &[u8],
    block: Option<&Block>,
    key: &str,
    value: &str,
    create: bool,
) -> Result<Option<Edit>> {
    if let Some(block) = block {
        let position = block
            .nodes
            .iter()
            .rev()
            .find(|n| OWNED.contains(&n.key.as_str()))
            .map_or(block.body.end, |n| n.range.end);
        checked_edit(
            bytes,
            edit(
                position..position,
                format!("{key}: {value}{}", block.line_ending).into_bytes(),
            ),
        )
    } else if create {
        let position = if bytes.starts_with(b"\xef\xbb\xbf") {
            3
        } else {
            0
        };
        checked_edit(
            bytes,
            edit(
                position..position,
                format!("---\n{key}: {value}\n---\n").into_bytes(),
            ),
        )
    } else {
        Ok(None)
    }
}
fn plain(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._/".contains(&b))
    {
        return Err(FrontMatterError::InvalidValue);
    }
    scalar(value)?;
    Ok(())
}
fn set_scalar(bytes: &[u8], key: &str, value: Option<&str>, create: bool) -> Result<Option<Edit>> {
    if let Some(value) = value {
        plain(value)?;
    }
    let value = value.map(|value| {
        if key == OWNED_NAME {
            Cow::Owned(format!("\"{value}\""))
        } else {
            Cow::Borrowed(value)
        }
    });
    let value = value.as_deref();
    let block = locate(bytes)?;
    if let Some(block) = &block {
        edit_checks(bytes, block)?;
        if let Some(n) = node(block, key)? {
            read_scalar(bytes, key)?;
            return checked_edit(
                bytes,
                match value {
                    Some(value) => value_edit(bytes, n.value.clone(), value.as_bytes()),
                    None => edit(n.range.start..n.continuation.start, Vec::new()),
                },
            );
        }
    }
    value.map_or(Ok(None), |value| {
        insert(bytes, block.as_ref(), key, value, create)
    })
}
/// Sets or removes the name. `None` means no edit is needed or block creation
/// was disabled. Values are written as double-quoted strings, but must still
/// pass the plain-token safety check; full name validation remains the caller's
/// responsibility. An existing value produces an edit even when it is equal.
pub fn set_name(bytes: &[u8], value: Option<&str>, create_block: bool) -> Result<Option<Edit>> {
    set_scalar(bytes, OWNED_NAME, value, create_block)
}
/// Sets or removes the id, creating a missing block only when requested.
pub fn set_id(bytes: &[u8], value: Option<FolioId>, create_block: bool) -> Result<Option<Edit>> {
    let value = value.map(|id| id.to_string());
    set_scalar(bytes, OWNED_ID, value.as_deref(), create_block)
}
fn quoted(value: &str) -> Result<String> {
    if value.is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(FrontMatterError::InvalidValue);
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
fn render_reference(r: &Reference<'_>, ending: &str) -> Result<String> {
    plain(&r.alias)?;
    if r.name.is_some() == r.id.is_some() {
        return Err(FrontMatterError::InvalidValue);
    }
    let mut output = format!("  {}:{ending}", r.alias);
    if let Some(name) = &r.name {
        output.push_str(&format!("    name: {}{ending}", quoted(name)?));
    }
    if let Some(id) = r.id {
        output.push_str(&format!("    id: \"{id}\"{ending}"));
    }
    if let Some(pin) = &r.pin {
        output.push_str(&format!("    pin: {}{ending}", quoted(pin)?));
    }
    Ok(output)
}
// Standalone comments and blank lines survive removal of their surrounding key.
fn remove_references(bytes: &[u8], node: &Node) -> Edit {
    let mut replacement = Vec::new();
    let mut pos = node.continuation.start;
    while pos < node.range.end {
        let l = line(bytes, pos, node.range.end);
        if !meaningful(text(&bytes[pos..l.end]).unwrap_or("")) {
            replacement.extend_from_slice(&bytes[pos..l.next]);
        }
        pos = l.next;
    }
    edit(node.range.clone(), replacement)
}
/// Replaces the entire references value, including nested block entries.
pub fn set_references(
    bytes: &[u8],
    values: &[Reference<'_>],
    create_block: bool,
) -> Result<Option<Edit>> {
    let block = locate(bytes)?;
    let ending = block.as_ref().map_or("\n", |b| b.line_ending);
    let mut rendered = String::new();
    let mut seen = Vec::new();
    for r in values {
        if seen.contains(&r.alias.as_ref()) {
            return Err(FrontMatterError::Duplicate);
        }
        seen.push(r.alias.as_ref());
        rendered.push_str(&render_reference(r, ending)?);
    }
    if let Some(block) = &block {
        edit_checks(bytes, block)?;
        if let Some(n) = node(block, OWNED_REFERENCES)? {
            let entries = references(bytes, n)?;
            if values.is_empty() {
                return checked_edit(bytes, remove_references(bytes, n));
            }
            // Flow entry spans end inside the collection; include its closing
            // delimiter and header line when converting to block entries.
            let end = entries
                .last()
                .map_or(n.continuation.start, |e| e.range.end)
                .max(n.continuation.start);
            // Keep the header comment and trailing standalone comments. Entry
            // values are rewritten together into the canonical quoted block form.
            let mut replacement = bytes[n.value.end..n.continuation.start].to_vec();
            replacement.extend_from_slice(rendered.as_bytes());
            return checked_edit(bytes, edit(n.value.start..end, replacement));
        }
    }
    if values.is_empty() {
        return Ok(None);
    }
    // insert supplies the final line ending itself.
    insert(
        bytes,
        block.as_ref(),
        OWNED_REFERENCES,
        &format!("{ending}{}", rendered.trim_end_matches(['\n', '\r'])),
        create_block,
    )
}
// A single entry inside a flow collection cannot contain a block mapping.
fn render_flow_reference(r: &Reference<'_>) -> Result<String> {
    render_reference(r, "\n")?;
    let mut fields = Vec::new();
    if let Some(name) = &r.name {
        fields.push(format!("name: {}", quoted(name)?));
    }
    if let Some(id) = r.id {
        fields.push(format!("id: \"{id}\""));
    }
    if let Some(pin) = &r.pin {
        fields.push(format!("pin: {}", quoted(pin)?));
    }
    Ok(format!("{}: {{{}}}", r.alias, fields.join(", ")))
}
/// Sets or removes one alias, writing quoted block mappings.
// split in the folt-core refactor (#203)
#[allow(clippy::too_many_lines)]
pub fn set_reference(
    bytes: &[u8],
    alias: &str,
    value: Option<&Reference<'_>>,
    create_block: bool,
) -> Result<Option<Edit>> {
    plain(alias)?;
    if value.is_some_and(|r| r.alias != alias) {
        return Err(FrontMatterError::InvalidValue);
    }
    let block = locate(bytes)?;
    if let Some(block) = &block {
        edit_checks(bytes, block)?;
        if let Some(n) = node(block, OWNED_REFERENCES)? {
            let entries = references(bytes, n)?;
            let selected = entries.iter().position(|e| e.reference.alias == alias);
            let rendered = value
                .map(|r| render_reference(r, block.line_ending))
                .transpose()?;
            let collection = text(&bytes[n.value.clone()])?;
            if collection.starts_with('{') {
                // Flow collections must stay flow collections. Only the selected
                // key/value and, on removal, one separator belong to the edit.
                let parts = mapping(collection)?;
                if let Some(index) = selected {
                    let start = entries[index].key_range.start;
                    let end = entries[index].value.end;
                    if let Some(value) = value {
                        return checked_edit(
                            bytes,
                            edit(start..end, render_flow_reference(value)?),
                        );
                    }
                    let commas: Vec<_> = punctuation(&collection[1..collection.len() - 1], ',')
                        .into_iter()
                        .map(|n| n + 1)
                        .collect();
                    let (range, replacement) = if index < commas.len() {
                        let comma = n.value.start + commas[index];
                        (start..comma + 1, bytes[end..comma].to_vec())
                    } else if index > 0 {
                        let comma = n.value.start + commas[index - 1];
                        (comma..end, bytes[comma + 1..start].to_vec())
                    } else {
                        (start..end, Vec::new())
                    };
                    return checked_edit(bytes, edit(range, replacement));
                }
                if let Some(value) = value {
                    let position = n.value.end - 1;
                    let has_separator = parts.last().is_some_and(|(_, old)| {
                        let end = old.as_ptr() as usize - collection.as_ptr() as usize + old.len();
                        !punctuation(&collection[end..collection.len() - 1], ',').is_empty()
                    });
                    let separator = if entries.is_empty() || has_separator {
                        ""
                    } else {
                        ", "
                    };
                    return checked_edit(
                        bytes,
                        edit(
                            position..position,
                            format!("{separator}{}", render_flow_reference(value)?),
                        ),
                    );
                }
                return Ok(None);
            }
            let indent = entries.first().map_or(2, |entry| {
                bytes[entry.range.start..]
                    .iter()
                    .take_while(|b| **b == b' ')
                    .count()
            });
            let rendered = rendered.map(|s| {
                s.split_inclusive(block.line_ending)
                    .map(|line| format!("{}{}", " ".repeat(indent), &line[2..]))
                    .collect::<String>()
            });
            if let Some(index) = selected {
                let entry = &entries[index];
                let mut rendered = rendered.unwrap_or_default();
                let header = line(bytes, entry.range.start, entry.range.end);
                let raw = text(&bytes[entry.range.start..header.end])?;
                let header_value = &raw[colon(raw).ok_or(FrontMatterError::InvalidValue)? + 1..];
                let nested =
                    header_value.trim().is_empty() || header_value.trim_start().starts_with('#');
                let flow_comments = if nested {
                    Vec::new()
                } else {
                    scan_punctuation(text(&bytes[entry.range.clone()])?, '#', true)
                        .into_iter()
                        .map(|offset| entry.range.start + offset)
                        .collect::<Vec<_>>()
                };
                let header_comment = if nested {
                    punctuation(raw, '#').first().copied()
                } else {
                    flow_comments
                        .first()
                        .filter(|pos| **pos < header.end)
                        .map(|pos| pos - entry.range.start)
                };
                if let Some(comment) = header_comment {
                    let comment = &raw[comment..];
                    if value.is_some() {
                        let end = rendered
                            .find(block.line_ending)
                            .ok_or(FrontMatterError::InvalidValue)?;
                        rendered.insert_str(end, &format!(" {comment}"));
                    } else {
                        rendered = format!("{}{comment}{}", " ".repeat(indent), block.line_ending);
                    }
                }
                let mut preserved = Vec::new();
                // Comments and blank lines between fields/entries are document
                // bytes, even when they fall inside the selected entry's span.
                let mut pos = header.next;
                while pos < entry.range.end {
                    let l = line(bytes, pos, entry.range.end);
                    let raw = text(&bytes[pos..l.end])?;
                    if !nested {
                        if let Some(comment) = flow_comments
                            .iter()
                            .find(|offset| **offset >= pos && **offset < l.end)
                        {
                            let spaces = if value.is_some() { indent + 2 } else { indent };
                            preserved.extend_from_slice(
                                format!(
                                    "{}{}{}",
                                    " ".repeat(spaces),
                                    text(&bytes[*comment..l.end])?,
                                    block.line_ending
                                )
                                .as_bytes(),
                            );
                        } else if raw.trim().is_empty() {
                            preserved.extend_from_slice(&bytes[pos..l.next]);
                        }
                    } else if !meaningful(raw) {
                        preserved.extend_from_slice(&bytes[pos..l.next]);
                    } else if let Some(comment) = punctuation(raw, '#').first() {
                        let comment = &raw[*comment..];
                        let key = raw[..colon(raw).ok_or(FrontMatterError::InvalidValue)?].trim();
                        let field =
                            format!("{}{}{key}:", block.line_ending, " ".repeat(indent + 2));
                        if let Some(start) = rendered.find(&field) {
                            let start = start + field.len();
                            let end = start
                                + rendered[start..]
                                    .find(block.line_ending)
                                    .ok_or(FrontMatterError::InvalidValue)?;
                            rendered.insert_str(end, &format!(" {comment}"));
                        } else {
                            // Removed fields keep their original indentation;
                            // removing the alias moves comments to entry level.
                            let spaces = if value.is_some() {
                                raw.len() - raw.trim_start_matches(' ').len()
                            } else {
                                indent
                            };
                            preserved.extend_from_slice(
                                format!("{}{comment}{}", " ".repeat(spaces), block.line_ending)
                                    .as_bytes(),
                            );
                        }
                    }
                    pos = l.next;
                }
                let mut replacement = rendered.into_bytes();
                replacement.extend_from_slice(&preserved);
                return checked_edit(bytes, edit(entry.range.clone(), replacement));
            }
            if let Some(rendered) = rendered {
                let position = entries.last().map_or(n.continuation.start, |e| e.range.end);
                return checked_edit(bytes, edit(position..position, rendered));
            }
            return Ok(None);
        }
    }
    match value {
        Some(value) => set_references(bytes, std::slice::from_ref(value), create_block),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests;
