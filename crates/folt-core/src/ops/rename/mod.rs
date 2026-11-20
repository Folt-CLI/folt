//! Rename the registry first, then the internal main file, then commit on main.
//! Copies keep their bytes and bases. Hold the home lock from planning through
//! apply. After interruption, repeat the same old/new request: only the exact
//! transformation of the old base is accepted as this operation's pending edit.
//! A completed rename clears its pending record; repeating it may return
//! UnknownName because the old registry name no longer exists.

mod apply;
mod error;
mod plan;

pub use apply::apply_rename;
pub use error::RenameError;
pub use plan::{RenamePlan, plan_rename};
