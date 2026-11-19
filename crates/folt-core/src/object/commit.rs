//! Encodes and parses canonical commit bodies; framing and field validation live elsewhere.

use crate::id::ObjectId;

use super::fields::{field, validate_message, validate_value};
use super::{Author, CommitTime, ObjectError, ObjectKind, decode, encode};

/// A canonical commit body. Headers end in LF; a nonempty body is preserved byte for byte.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    pub content: ObjectId,
    pub parents: Vec<ObjectId>,
    pub author: Author,
    pub time: CommitTime,
    pub message: String,
    pub body: Option<String>,
    pub group: Option<String>,
}

impl Commit {
    pub fn encode_body(&self) -> Result<Vec<u8>, ObjectError> {
        validate_message(&self.message)?;
        if let Some(group) = &self.group {
            validate_value(group)?;
        }
        let mut text = format!("content {}\n", self.content);
        for parent in &self.parents {
            text.push_str(&format!("parent {parent}\n"));
        }
        text.push_str(&format!(
            "author {}\ntime {}\nmessage {}\n",
            self.author, self.time, self.message
        ));
        if let Some(group) = &self.group {
            text.push_str(&format!("group {group}\n"));
        }
        if let Some(body) = self.body.as_ref().filter(|body| !body.is_empty()) {
            text.push('\n');
            text.push_str(body);
        }
        Ok(text.into_bytes())
    }

    pub fn parse_body(body: &[u8]) -> Result<Self, ObjectError> {
        let text = std::str::from_utf8(body).map_err(|_| ObjectError::InvalidObject)?;
        let (headers, body_text) = match text.split_once("\n\n") {
            Some((headers, body)) => (headers, Some(body.to_owned())),
            None => (
                text.strip_suffix('\n').ok_or(ObjectError::InvalidObject)?,
                None,
            ),
        };
        let mut lines = headers.split('\n');
        let content = field(lines.next(), "content ")?.parse()?;
        let mut parents = Vec::new();
        let mut line = lines.next();
        while let Some(value) = line.and_then(|line| line.strip_prefix("parent ")) {
            parents.push(value.parse()?);
            line = lines.next();
        }
        let author = field(line, "author ")?.parse()?;
        let time = field(lines.next(), "time ")?.parse()?;
        let message = field(lines.next(), "message ")?.to_owned();
        let group = lines
            .next()
            .map(|line| field(Some(line), "group ").map(str::to_owned))
            .transpose()?;
        if lines.next().is_some() {
            return Err(ObjectError::InvalidObject);
        }
        let commit = Self {
            content,
            parents,
            author,
            time,
            message,
            body: body_text,
            group,
        };
        if commit.encode_body()? != body {
            return Err(ObjectError::InvalidObject);
        }
        Ok(commit)
    }

    pub fn encode(&self) -> Result<Vec<u8>, ObjectError> {
        Ok(encode(ObjectKind::Commit, &self.encode_body()?))
    }

    pub fn parse(framed: &[u8], limit: u64) -> Result<Self, ObjectError> {
        Self::parse_body(&decode(framed, ObjectKind::Commit, limit)?)
    }
}
