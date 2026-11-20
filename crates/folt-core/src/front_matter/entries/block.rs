use crate::front_matter::error::{FrontMatterError, Result};
use crate::front_matter::scalar::{meaningful, text};
use crate::front_matter::scan::{colon, flow_end, line, punctuation, span, trim_end};

use super::{Entries, Entry};

impl<'a> Entries<'a> {
    pub(super) fn block_entry(&mut self) -> Result<Entry<'a>> {
        let bytes = self.bytes;
        let pos = self.pos;
        let current = line(bytes, pos, self.end);
        let raw = text(&bytes[pos..current.end])?;
        let spaces = raw.len() - raw.trim_start_matches(' ').len();
        if spaces == 0 || *self.indent.get_or_insert(spaces) != spaces {
            return Err(FrontMatterError::Unsupported);
        }
        let clean = &raw[spaces..];
        let c = colon(clean).ok_or(FrontMatterError::InvalidValue)?;
        let key_range = span(bytes, clean[..c].trim());
        let start = pos + spaces + c + 1;
        let start = start
            + bytes[start..current.end]
                .iter()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
        let comment = punctuation(text(&bytes[start..current.end])?, '#')
            .first()
            .map_or(current.end, |i| start + i);
        let mut end = trim_end(bytes, comment, start);
        self.pos = current.next;
        let mut entry_end = self.pos;
        if bytes.get(start) == Some(&b'{') {
            end = start
                + flow_end(text(&bytes[start..self.end])?).ok_or(FrontMatterError::InvalidValue)?;
            let final_line = line(bytes, end, self.end);
            let suffix = text(&bytes[end..final_line.end])?.trim();
            if !suffix.is_empty() && !suffix.starts_with('#') {
                return Err(FrontMatterError::Unsupported);
            }
            self.pos = final_line.next;
            entry_end = self.pos;
        }
        let fields = if start == end {
            let (fields, last) = self.nested_fields(spaces)?;
            entry_end = entry_end.max(last);
            Some(fields)
        } else {
            None
        };
        Ok(Entry {
            range: pos..entry_end,
            key_range,
            value: start..end,
            fields,
        })
    }

    fn nested_fields(&mut self, spaces: usize) -> Result<(Vec<(&'a str, &'a str)>, usize)> {
        let mut fields = Vec::new();
        let mut indent = None;
        let mut end = self.pos;
        while self.pos < self.end {
            let child = line(self.bytes, self.pos, self.end);
            let raw = text(&self.bytes[self.pos..child.end])?;
            if !meaningful(raw) {
                self.pos = child.next;
                continue;
            }
            let spaces_here = raw.len() - raw.trim_start_matches(' ').len();
            if spaces_here <= spaces {
                break;
            }
            if *indent.get_or_insert(spaces_here) != spaces_here {
                return Err(FrontMatterError::Unsupported);
            }
            let clean = raw.trim();
            let c = colon(clean).ok_or(FrontMatterError::InvalidValue)?;
            let value = clean[c + 1..].trim();
            let comment = punctuation(value, '#')
                .first()
                .copied()
                .unwrap_or(value.len());
            fields.push((clean[..c].trim(), value[..comment].trim()));
            self.pos = child.next;
            end = self.pos;
        }
        Ok((fields, end))
    }
}
