use std::error::Error;
use std::fmt;

use crate::config::formats::OBJECT_ALGORITHM;
use crate::layout::{FOLIO_LAYOUT_VERSION, LayoutSupport, LayoutVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolioFormat {
    pub layout: LayoutVersion,
    pub algorithm: String,
}

/// Checks whether a registry format can be read by this build.
pub fn check_format(format: &FolioFormat) -> Result<(), FormatError> {
    match format.layout.support_as_folio_layout() {
        LayoutSupport::Supported => {}
        LayoutSupport::Newer => {
            return Err(FormatError::Newer {
                found: format.layout,
                supported: FOLIO_LAYOUT_VERSION,
            });
        }
        LayoutSupport::Older => {
            return Err(FormatError::Older {
                found: format.layout,
                supported: FOLIO_LAYOUT_VERSION,
            });
        }
    }
    if format.algorithm != OBJECT_ALGORITHM {
        return Err(FormatError::UnsupportedAlgorithm(format.algorithm.clone()));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatError {
    Newer {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    Older {
        found: LayoutVersion,
        supported: LayoutVersion,
    },
    UnsupportedAlgorithm(String),
}
impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Newer { found, supported } => write!(
                f,
                "Folio layout {found} is newer than supported layout {supported}"
            ),
            Self::Older { found, supported } => write!(
                f,
                "Folio layout {found} is older than supported layout {supported}"
            ),
            Self::UnsupportedAlgorithm(value) => write!(f, "unsupported object algorithm: {value}"),
        }
    }
}
impl Error for FormatError {}
