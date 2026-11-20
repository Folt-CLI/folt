use std::path::PathBuf;

use crate::config::env::Environment;
use crate::config::names::HOME_DIR_NAME;

use super::HomeError;

/// Resolves the home from an Environment.
pub fn resolve_home(environment: &Environment) -> Result<PathBuf, HomeError> {
    if let Some(value) = environment
        .folt_home
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(HomeError::InvalidFoltHome(value.clone()));
        }
        return Ok(path);
    }

    environment
        .platform_home
        .as_ref()
        .map(|path| path.join(HOME_DIR_NAME))
        .ok_or(HomeError::NoHomeDirectory)
}
