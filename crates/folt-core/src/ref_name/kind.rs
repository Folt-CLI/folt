use crate::config::names::{HEADS_DIR, PINS_DIR, TAGS_DIR};

use super::{LowerRefError, RefName, parse_pin};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefKind {
    Branch,
    Tag,
    Pin,
}
impl RefKind {
    pub(crate) fn directory(self) -> &'static str {
        match self {
            Self::Branch => HEADS_DIR,
            Self::Tag => TAGS_DIR,
            Self::Pin => PINS_DIR,
        }
    }
    pub(crate) fn name(self, value: &str) -> Result<RefName, LowerRefError> {
        if self == Self::Pin {
            parse_pin(value)?;
            Ok(RefName(value.into()))
        } else {
            RefName::parse(value)
        }
    }
}
