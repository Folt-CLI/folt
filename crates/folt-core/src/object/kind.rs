//! Defines canonical object kinds and their byte prefixes; framing lives elsewhere.

/// The kind prefix in a canonical object encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Blob,
    Commit,
    Tag,
}

impl ObjectKind {
    pub(super) fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Blob => b"blob",
            Self::Commit => b"commit",
            Self::Tag => b"tag",
        }
    }
}
