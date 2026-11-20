/// The way a Folio is presented at a placement path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementKind {
    Link,
    Copy,
    Internal,
}

impl PlacementKind {
    /// The default for a user placement is a copy; Internal is never chosen by a user.
    pub const DEFAULT: Self = Self::Copy;

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Copy => "copy",
            Self::Internal => "internal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "link" => Some(Self::Link),
            "copy" => Some(Self::Copy),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }

    /// Parses a user placement kind from an option or setting value.
    pub fn parse_user(value: &str) -> Option<Self> {
        match value {
            "link" => Some(Self::Link),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }
}
