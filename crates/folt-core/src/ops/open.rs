use std::path::PathBuf;
use std::str::FromStr;

use crate::config::env::Environment;
use crate::config::names::REGISTRY_FILE;
use crate::home::{Home, HomeError};
use crate::layout::{HOME_FORMAT_VERSION, LayoutSupport, LayoutVersion};
use crate::registry::Registry;

impl Home {
    /// Opens or creates a home directory and validates its contents and format.
    ///
    /// Concurrent first runs are not serialised until the home lock exists.
    pub fn open_or_create(path: PathBuf, environment: &Environment) -> Result<Self, HomeError> {
        let home = Self::create_and_recognize(path.clone(), environment)?;
        let registry = Registry::open_existing(&home)?;
        if let Some(text) = registry.format_version()? {
            let version =
                LayoutVersion::from_str(&text).map_err(|_| HomeError::InvalidFormatVersion {
                    path: path.join(REGISTRY_FILE),
                    found: text,
                })?;
            match version.support_as_home_format() {
                LayoutSupport::Supported => {}
                LayoutSupport::Newer => {
                    return Err(HomeError::NewerFormatVersion {
                        found: version,
                        supported: HOME_FORMAT_VERSION,
                    });
                }
                LayoutSupport::Older => {
                    return Err(HomeError::OlderFormatVersion {
                        found: version,
                        supported: HOME_FORMAT_VERSION,
                    });
                }
            }
        } else if path.join(REGISTRY_FILE).exists() {
            return Err(HomeError::MissingFormatVersion {
                path: path.join(REGISTRY_FILE),
            });
        }

        Ok(home)
    }
}
