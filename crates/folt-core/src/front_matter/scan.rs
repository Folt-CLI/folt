#[derive(Clone)]
pub(super) struct Line {
    pub(super) end: usize,
    pub(super) next: usize,
}
pub(super) fn line(bytes: &[u8], start: usize, end: usize) -> Line {
    let next = bytes[start..end]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(end, |n| start + n + 1);
    let mut content = next;
    if content > start && bytes[content - 1] == b'\n' {
        content -= 1;
        if content > start && bytes[content - 1] == b'\r' {
            content -= 1;
        }
    }
    Line { end: content, next }
}
pub(super) fn trim_end(bytes: &[u8], mut end: usize, start: usize) -> usize {
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

// Scan punctuation only outside quotes/comments. This also lets wrapped flow
// mappings stay inside their node even when their lines start at column zero.
pub(super) fn punctuation(s: &str, wanted: char) -> Vec<usize> {
    scan_punctuation(s, wanted, false)
}

pub(super) fn scan_punctuation(s: &str, wanted: char, nested_comments: bool) -> Vec<usize> {
    let mut result = Vec::new();
    let mut quote = None;
    let mut escape = false;
    let mut depth = 0i32;
    let mut comment = false;
    let chars: Vec<_> = s.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (i, c) = chars[index];
        if comment {
            if c == '\n' {
                comment = false;
            }
            index += 1;
            continue;
        }
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if q == '"' && c == '\\' {
                escape = true;
            } else if c == q {
                if q == '\'' && chars.get(index + 1).is_some_and(|(_, c)| *c == '\'') {
                    index += 1;
                } else {
                    quote = None;
                }
            }
        } else if c == '#' && (i == 0 || s.as_bytes()[i - 1].is_ascii_whitespace()) {
            if wanted == '#' && (depth == 0 || nested_comments) {
                result.push(i);
            }
            comment = true;
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else {
            if c == wanted && depth == 0 {
                result.push(i);
            }
            if c == '{' || c == '[' {
                depth += 1;
            }
            if c == '}' || c == ']' {
                depth -= 1;
            }
        }
        index += 1;
    }
    result
}
pub(super) fn colon(s: &str) -> Option<usize> {
    punctuation(s, ':').into_iter().find(|i| {
        s.as_bytes()
            .get(i + 1)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'{' || *b == b'[')
    })
}
pub(super) fn flow_end(s: &str) -> Option<usize> {
    let closing = if s.starts_with('{') { '}' } else { ']' };
    punctuation(&s[1..], closing).first().map(|n| n + 2)
}

/// Locates a borrowed token within its original input without copying it.
pub(super) fn span(bytes: &[u8], token: &str) -> std::ops::Range<usize> {
    let start = token.as_ptr() as usize - bytes.as_ptr() as usize;
    start..start + token.len()
}
