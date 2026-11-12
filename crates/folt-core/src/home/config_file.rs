//! Reading Settings from the home's `config.toml`.

use std::{error::Error, fmt, fs, io, path::PathBuf};

use crate::config::names::{ANONYMOUS_AUTHOR, CONFIG_FILE};
use crate::config::settings::{FileSettings, Settings, SettingsError};
use crate::home::Home;
use crate::object::{Author, ObjectError};

impl FileSettings {
    /// Loads settings, using defaults only when the file is missing.
    pub fn load(home: &Home) -> Result<Self, ConfigError> {
        let path = home.path().join(CONFIG_FILE);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        let metadata = if metadata.is_symlink() {
            fs::metadata(&path).map_err(|source| ConfigError::Read {
                path: path.clone(),
                source,
            })?
        } else {
            metadata
        };
        if !metadata.is_file() {
            return Err(ConfigError::NotAFile { path });
        }
        let text = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let settings: Self = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        Ok(settings)
    }
}

impl Settings {
    /// Loads and resolves settings, then validates the configured author.
    pub fn load(home: &Home) -> Result<Self, ConfigError> {
        let file = FileSettings::load(home)?;
        let settings = Self::resolve(home.environment(), file).map_err(ConfigError::Settings)?;
        let path = home.path().join(CONFIG_FILE);
        settings
            .validated_author()
            .map_err(|source| ConfigError::Invalid { path, source })?;
        Ok(settings)
    }

    /// Returns the configured author, or `anonymous` when no username is set.
    pub fn author(&self) -> Result<Author, ConfigError> {
        self.validated_author().map_err(ConfigError::Author)
    }

    fn validated_author(&self) -> Result<Author, ObjectError> {
        let author = Author::new(
            self.username.as_deref().unwrap_or(ANONYMOUS_AUTHOR),
            self.email.as_deref(),
        )?;
        if self.username.is_some() {
            Ok(author)
        } else {
            Ok(Author::anonymous())
        }
    }
}

/// Errors that can occur while loading settings or building an author.
#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    NotAFile {
        path: PathBuf,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid {
        path: PathBuf,
        source: ObjectError,
    },
    Author(ObjectError),
    Settings(SettingsError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "cannot read config {}: {source}", path.display())
            }
            Self::NotAFile { path } => {
                write!(
                    formatter,
                    "config is not a regular file: {}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "cannot parse config {}: {source}",
                    path.display()
                )
            }
            Self::Invalid { path, source } => {
                write!(
                    formatter,
                    "invalid author in config {}: {source}",
                    path.display()
                )
            }
            Self::Settings(source) => fmt::Display::fmt(source, formatter),
            Self::Author(source) => write!(formatter, "invalid config author: {source}"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Invalid { source, .. } | Self::Author(source) => Some(source),
            Self::Settings(source) => Some(source),
            Self::NotAFile { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests;
