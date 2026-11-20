use crate::folio_name::FolioName;
use crate::front_matter::FrontMatterError;
use crate::object::ObjectError;
use crate::ops::history::HistoryError;
use crate::ops::rules_error::RulesError;
use crate::placement_path::RegistryError;
use crate::store::StoreError;
use std::{error::Error, fmt, path::PathBuf};

#[derive(Debug)]
pub enum RenameError {
    UnknownName(FolioName),
    InternalChanged(PathBuf),
    InternalNameMismatch { path: PathBuf, name: String },
    PendingRename { old: FolioName, new: FolioName },
    PlanChanged,
    Registry(RegistryError),
    Io(std::io::Error),
    FrontMatter(FrontMatterError),
    Store(StoreError),
    Working(RulesError),
    History(HistoryError),
    Object(ObjectError),
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PendingRename { old, new } => {
                write!(f, "finish pending rename `{old}` to `{new}` first")
            }
            Self::UnknownName(name) => write!(f, "unknown Folio name: {name}"),
            Self::InternalNameMismatch { path, name } => write!(
                f,
                "cannot rename: {} has folio-name `{name}`; the file and the registry disagree; fix the internal file's folio-name first",
                path.display()
            ),
            Self::InternalChanged(path) => write!(
                f,
                "cannot rename: {} has uncommitted edits or a staged change; commit or discard the internal file's changes and unstage it before retrying",
                path.display()
            ),
            Self::PlanChanged => f.write_str("rename state changed; plan the rename again"),
            Self::Registry(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::FrontMatter(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Working(e) => e.fmt(f),
            Self::History(e) => e.fmt(f),
            Self::Object(e) => e.fmt(f),
        }
    }
}
impl Error for RenameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::FrontMatter(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::Working(e) => Some(e),
            Self::History(e) => Some(e),
            Self::Object(e) => Some(e),
            _ => None,
        }
    }
}
macro_rules! from_error {
    ($ty:ty, $variant:ident) => {
        impl From<$ty> for RenameError {
            fn from(error: $ty) -> Self {
                Self::$variant(error)
            }
        }
    };
}
from_error!(RegistryError, Registry);
from_error!(std::io::Error, Io);
from_error!(FrontMatterError, FrontMatter);
from_error!(StoreError, Store);
from_error!(RulesError, Working);
from_error!(HistoryError, History);
from_error!(ObjectError, Object);
