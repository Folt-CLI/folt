//! Home methods that forward to the owning operation functions without adding rules.
//! Each method keeps the owner's arguments; the owner defines every rule and error.

mod checkout;
mod history;
mod placement;
mod prune;
mod rebase;
mod register;
mod rename;
mod stage;
mod status;
