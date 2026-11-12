use std::{error::Error, fmt, io, path::PathBuf};

#[derive(Debug)]
pub enum LowerRefError {
    InvalidName(String),
    InvalidRef(PathBuf),
    Conflict { existing: PathBuf },
    Io { path: PathBuf, source: io::Error },
}
impl fmt::Display for LowerRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "invalid ref name: {name}"),
            Self::InvalidRef(path) => write!(f, "invalid ref at {}", path.display()),
            Self::Conflict { existing } => write!(f, "ref conflicts with {}", existing.display()),
            Self::Io { path, source } => write!(f, "ref I/O at {}: {source}", path.display()),
        }
    }
}
impl Error for LowerRefError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
