use std::{error::Error, fmt, str::FromStr};

use crate::config::limits::HEX_ID_LEN;

/// The SHA-256 name of an object.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObjectId([u8; 32]);

impl ObjectId {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ObjectId {
    type Err = InvalidObjectId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != HEX_ID_LEN
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(InvalidObjectId);
        }
        let mut bytes = [0; 32];
        for (index, chunk) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let digit = |b: u8| {
                if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b - b'a' + 10
                }
            };
            bytes[index] = digit(chunk[0]) * 16 + digit(chunk[1]);
        }
        Ok(Self(bytes))
    }
}

/// An invalid lowercase hexadecimal object id.
#[derive(Debug)]
pub struct InvalidObjectId;

impl fmt::Display for InvalidObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid object hash")
    }
}
impl Error for InvalidObjectId {}
