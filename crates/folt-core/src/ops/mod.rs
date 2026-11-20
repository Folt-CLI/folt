//! The operations tree orders refusal, facts, working, status, step_hook, stage, placement, history, register, rename, checkout, prune.
//! Each module imports only earlier names or rollback, which imports only fsx.

mod step_hook;

pub mod checkout;
pub mod facts;
pub mod history;
mod open;
pub mod placement;
pub mod prune;
pub mod refusal;
pub mod register;
pub mod rename;
pub mod rules_error;
pub mod stage;
pub mod status;
pub mod working;

pub(crate) mod rollback;
