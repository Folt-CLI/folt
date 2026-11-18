use crate::ops::refusal::Refusal;
use crate::placement::PlacementError;
use crate::placement_path::RegistryError;
use crate::refs::RefError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum RulesError {
    Internal(&'static str),
    Placement(PlacementError),
    Ref(RefError),
    Registry(RegistryError),
    Refused(Refusal),
}

impl From<PlacementError> for RulesError {
    fn from(error: PlacementError) -> Self {
        Self::Placement(error)
    }
}

impl From<RefError> for RulesError {
    fn from(error: RefError) -> Self {
        Self::Ref(error)
    }
}

impl From<RegistryError> for RulesError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<Refusal> for RulesError {
    fn from(error: Refusal) -> Self {
        Self::Refused(error)
    }
}

impl fmt::Display for RulesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(message) => write!(f, "inconsistent placement facts: {message}"),
            Self::Placement(error) => error.fmt(f),
            Self::Ref(error) => error.fmt(f),
            Self::Registry(error) => error.fmt(f),
            Self::Refused(error) => error.fmt(f),
        }
    }
}

impl Error for RulesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Internal(_) => None,
            Self::Placement(error) => Some(error),
            Self::Ref(error) => Some(error),
            Self::Registry(error) => Some(error),
            Self::Refused(error) => Some(error),
        }
    }
}
