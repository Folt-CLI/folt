use super::Refusal;
use crate::object::ObjectError;
use crate::ops::{refusal as placement_refusal, rules_error::RulesError};
use crate::placement::PlacementError;
use crate::placement_path::RegistryError;
use crate::refs::RefError;
use crate::store::StoreError;
use std::{error::Error, fmt};

#[derive(Debug)]
pub enum HistoryError {
    Refused(Refusal),
    Object(ObjectError),
    Store(StoreError),
    Ref(RefError),
    Registry(RegistryError),
    Placement(PlacementError),
    Rules(RulesError),
}

impl From<Refusal> for HistoryError {
    fn from(error: Refusal) -> Self {
        Self::Refused(error)
    }
}
impl From<ObjectError> for HistoryError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}
impl From<StoreError> for HistoryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
impl From<RefError> for HistoryError {
    fn from(error: RefError) -> Self {
        Self::Ref(error)
    }
}
impl From<RegistryError> for HistoryError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}
impl From<PlacementError> for HistoryError {
    fn from(error: PlacementError) -> Self {
        Self::Placement(error)
    }
}
impl From<RulesError> for HistoryError {
    fn from(error: RulesError) -> Self {
        Self::Rules(error)
    }
}
impl From<placement_refusal::Refusal> for HistoryError {
    fn from(error: placement_refusal::Refusal) -> Self {
        Self::Refused(error.into())
    }
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(error) => error.fmt(f),
            Self::Object(error) => error.fmt(f),
            Self::Store(error) => error.fmt(f),
            Self::Ref(error) => error.fmt(f),
            Self::Registry(error) => error.fmt(f),
            Self::Placement(error) => error.fmt(f),
            Self::Rules(error) => error.fmt(f),
        }
    }
}
impl Error for HistoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Refused(error) => Some(error),
            Self::Object(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Ref(error) => Some(error),
            Self::Registry(error) => Some(error),
            Self::Placement(error) => Some(error),
            Self::Rules(error) => Some(error),
        }
    }
}
