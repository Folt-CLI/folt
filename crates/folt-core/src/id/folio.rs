use std::{error::Error, fmt};

use uuid::Uuid;

/// A canonical, randomly generated Folio identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FolioId(Uuid);

impl FolioId {
    /// Generates a version 4 identifier.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parses only the lowercase hyphenated representation.
    pub fn parse(value: &str) -> Result<Self, InvalidFolioId> {
        let id = Uuid::try_parse(value).map_err(|_| InvalidFolioId(value.into()))?;
        if id.hyphenated().to_string() != value {
            return Err(InvalidFolioId(value.into()));
        }
        Ok(Self(id))
    }

    /// Returns the 16 bytes used by the registry.
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }

    /// Constructs an identifier from registry bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }
}

impl fmt::Display for FolioId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(formatter)
    }
}

/// An invalid lowercase hyphenated Folio id.
#[derive(Debug)]
pub struct InvalidFolioId(pub String);

impl fmt::Display for InvalidFolioId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Folio id: {}", self.0)
    }
}
impl Error for InvalidFolioId {}
