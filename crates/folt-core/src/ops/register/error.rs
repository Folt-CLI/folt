use std::error::Error;
use std::fmt;
use std::io;

use crate::folio_dir::FolioDirError;
use crate::home::HomeError;
use crate::home::config_file::ConfigError;
use crate::placement_path::RegistryError;

#[derive(Debug)]
pub enum RegisterError {
    Io(io::Error),
    Config(ConfigError),
    Registry(RegistryError),
    Home(HomeError),
    Folio(FolioDirError),
    Placement(crate::placement::PlacementError),
}
impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Config(e) => e.fmt(f),
            Self::Registry(e) => e.fmt(f),
            Self::Home(e) => e.fmt(f),
            Self::Folio(e) => e.fmt(f),
            Self::Placement(e) => e.fmt(f),
        }
    }
}
impl Error for RegisterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Io(e) => e,
            Self::Config(e) => e,
            Self::Registry(e) => e,
            Self::Home(e) => e,
            Self::Folio(e) => e,
            Self::Placement(e) => e,
        })
    }
}
macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for RegisterError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}
from_error!(io::Error, Io);
from_error!(ConfigError, Config);
from_error!(RegistryError, Registry);
from_error!(HomeError, Home);
from_error!(FolioDirError, Folio);
from_error!(crate::placement::PlacementError, Placement);
