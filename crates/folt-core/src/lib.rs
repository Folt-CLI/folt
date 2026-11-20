#![cfg_attr(test, allow(clippy::unreachable))]
#![cfg_attr(test, allow(clippy::disallowed_methods))]

pub mod config;
pub mod folio;
pub mod folio_dir;
pub mod folio_name;
pub mod front_matter;
mod fsx;
pub mod home;
pub mod id;
pub mod layout;
pub mod object;
pub mod ops;
pub mod placement;
pub mod placement_fs;
pub mod placement_path;
pub mod ref_name;
pub mod refs;
pub mod registry;
pub mod store;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

/// Returns the version of `folt`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
