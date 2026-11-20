//! The operations tree orders refusal, facts, working, status, stage, history, rename, checkout, prune.
//! Placement operations follow status and precede stage.

pub mod checkout;
pub mod facts;
pub mod history;
mod open;
pub mod placement;
pub mod prune;
pub mod refusal;
pub mod rename;
pub mod rules_error;
pub mod stage;
pub mod status;
pub mod working;
