use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::folio::name::{FolioName, FolioNameError, StemError};
use crate::front_matter::FrontMatterError;
use crate::id::FolioId;

use super::plan::{NameWrite, RegisterEntry, RegisterStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameHolder {
    Folio(FolioId),
    Path(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterRefusalReason {
    /// Name the target explicitly instead of its symlink.
    Symlink,
    NotAFile,
    Missing,
    Unreadable,
    InsideHome,
    InsideDirectoryFolio,
    ConflictingNames,
    FileChanged,
    DestinationChanged {
        path: PathBuf,
    },
    /// Renames belong to issue #155.
    NameOnRegisteredPath,
    NameTaken {
        name: FolioName,
        holder: NameHolder,
    },
    InvalidFrontMatterName(FolioNameError),
    FrontMatter(FrontMatterError),
    NameOptionWithFrontMatter,
    NoUsableStem(StemError),
    NameNotWritable {
        reason: String,
    },
}

impl fmt::Display for RegisterRefusalReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NameOnRegisteredPath => {
                f.write_str("--name on a registered path is refused; use rename (#155)")
            }
            Self::NameOptionWithFrontMatter => {
                f.write_str("--name is refused when folio-name already exists; use rename (#155)")
            }
            Self::ConflictingNames => f.write_str("one path requested with different names"),
            Self::DestinationChanged { path } => {
                write!(f, "{}: destination changed since the plan", path.display())
            }
            Self::NoUsableStem(error) => error.fmt(f),
            Self::FrontMatter(error) => error.fmt(f),
            Self::InvalidFrontMatterName(error) => write!(f, "invalid folio-name: {error}"),
            Self::NameNotWritable { reason } => write!(f, "cannot write folio-name: {reason}"),
            Self::NameTaken { name, holder } => write!(f, "name {name} is taken by {holder:?}"),
            other => write!(f, "{other:?}"),
        }
    }
}

pub(super) fn refuse_name(entry: &mut RegisterEntry, reason: RegisterRefusalReason) {
    entry.status = RegisterStatus::Refused(reason);
    entry.name = None;
    entry.name_source = None;
    entry.name_write = NameWrite::None;
}

pub(super) fn io_refusal(error: &io::Error) -> RegisterRefusalReason {
    if error.kind() == io::ErrorKind::NotFound {
        RegisterRefusalReason::Missing
    } else {
        RegisterRefusalReason::Unreadable
    }
}

#[cfg(test)]
mod tests;
