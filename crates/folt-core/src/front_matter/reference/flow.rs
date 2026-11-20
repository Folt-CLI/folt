use super::super::error::{FrontMatterError, Result};
use super::super::scan::{colon, punctuation};

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
pub(in super::super) fn mapping(s: &str) -> Result<Vec<(&str, &str)>> {
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
