//! Encodes and parses canonical tag bodies; framing and field validation live elsewhere.

use crate::id::ObjectId;

use super::fields::{field, validate_value};
use super::{Author, CommitTime, ObjectError, ObjectKind, decode, encode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tag {
    pub target: ObjectId,
    pub name: String,
    pub tagger: Author,
    pub time: CommitTime,
    pub message: String,
}

impl Tag {
    pub fn encode_body(&self) -> Result<Vec<u8>, ObjectError> {
        crate::ref_name::RefName::parse(&self.name).map_err(|_| ObjectError::InvalidObject)?;
        validate_value(&self.message)?;
        Ok(format!(
            "target {}\nname {}\ntagger {}\ntime {}\nmessage {}\n",
            self.target, self.name, self.tagger, self.time, self.message
        )
        .into_bytes())
    }

    pub fn parse_body(body: &[u8]) -> Result<Self, ObjectError> {
        let text = std::str::from_utf8(body).map_err(|_| ObjectError::InvalidObject)?;
        let mut lines = text.split_terminator('\n');
        let tag = Self {
            target: field(lines.next(), "target ")?.parse()?,
            name: field(lines.next(), "name ")?.into(),
            tagger: field(lines.next(), "tagger ")?.parse()?,
            time: field(lines.next(), "time ")?.parse()?,
            message: field(lines.next(), "message ")?.into(),
        };
        if lines.next().is_some() || tag.encode_body()? != body {
            return Err(ObjectError::InvalidObject);
        }
        Ok(tag)
    }

    pub fn encode(&self) -> Result<Vec<u8>, ObjectError> {
        Ok(encode(ObjectKind::Tag, &self.encode_body()?))
    }
    pub fn parse(framed: &[u8], limit: u64) -> Result<Self, ObjectError> {
        Self::parse_body(&decode(framed, ObjectKind::Tag, limit)?)
    }
}
