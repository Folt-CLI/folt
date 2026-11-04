//! Shared fixtures for in-crate unit tests and the `tests/` crate via the
//! `test-support` feature. Temporary storage is removed on drop.
#[cfg(test)]
mod facts;
#[cfg(test)]
mod home;
mod paths;

#[cfg(test)]
pub(crate) use facts::{RelocateFixture, facts, target};
#[cfg(test)]
pub(crate) use home::{TempDir, TempHome};
pub use paths::expected_path;
