use std::fs;
use std::io;
use std::path::PathBuf;

use crate::config::env::Environment;
use crate::config::settings::{FileSettings, Settings};

use super::{Home, HomeError, home_warnings, private_directory_builder, recognize_home};

impl Home {
    pub(crate) fn create_and_recognize(
        path: PathBuf,
        environment: &Environment,
    ) -> Result<Self, HomeError> {
        let settings = Settings::resolve(environment, FileSettings::default())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| HomeError::CannotCreate {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        match private_directory_builder().create(&path) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(source) => {
                return Err(HomeError::CannotCreate {
                    path: path.clone(),
                    source,
                });
            }
        }

        recognize_home(&path)?;
        let home = Self {
            environment: environment.clone(),
            busy_timeout: settings.busy_timeout,
            warnings: home_warnings(&path)?,
            path: path.clone(),
        };
        Ok(home)
    }
}
