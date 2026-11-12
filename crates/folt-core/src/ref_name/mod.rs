//! Portable ref names and ref kinds, depending only on config and id.

mod error;
mod kind;
mod name;

pub use error::LowerRefError;
pub use kind::RefKind;
pub use name::{RefName, parse_pin, pin_name};

#[cfg(test)]
mod tests;
