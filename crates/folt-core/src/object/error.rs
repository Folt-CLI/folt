//! Defines object errors and their diagnostics; encoding and validation live elsewhere.

use std::error::Error;
use std::fmt;
use std::io;

use crate::id::InvalidObjectId;

#[derive(Debug)]
pub enum ObjectError {
    InvalidHash,
    InvalidFrame,
    InvalidObject,
    LimitExceeded,
    Truncated,
    UnknownCodec(u8),
    HashMismatch,
    Io(io::Error),
}

impl fmt::Display for ObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHash => write!(f, "invalid object hash"),
            Self::InvalidFrame => write!(f, "invalid object framing"),
            Self::InvalidObject => write!(f, "invalid canonical object"),
            Self::LimitExceeded => write!(f, "object exceeds size limit"),
            Self::Truncated => write!(f, "object body is truncated"),
            Self::UnknownCodec(codec) => write!(f, "unknown object codec 0x{codec:02x}"),
            Self::HashMismatch => write!(f, "object hash does not match its name"),
            Self::Io(error) => write!(f, "object codec error: {error}"),
        }
    }
}
impl Error for ObjectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InvalidObjectId> for ObjectError {
    fn from(_: InvalidObjectId) -> Self {
        Self::InvalidHash
    }
}
