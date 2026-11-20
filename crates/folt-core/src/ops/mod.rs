//! The operations tree orders refusal, facts, working, status, step_hook, placement, then register; each module imports only earlier names or rollback, which imports only fsx.

mod step_hook;

pub mod facts;
mod open;
pub mod placement;
pub mod refusal;
pub mod register;
pub mod rules_error;
pub mod status;
pub mod working;

pub(crate) mod rollback;
