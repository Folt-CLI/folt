//! Explicit-path registration. Planning reads metadata and a bounded Markdown
//! prefix. Apply atomically writes missing folio-name keys before hashing and
//! ingesting those bytes, and registers the batch in one transaction.
//!
//! Ordinary failures restore rewritten files if still unchanged, and remove this
//! call's markers and Folio directories. Restores that fail before publication
//! report that the file keeps its name; published restores report durability errors.
//! Writes sync a temporary file and its directory, change the inode,
//! and refuse hard links; permissions are copied, but ownership/ACLs/xattrs are not.
//! A crash before the transaction can leave names in files and orphan directories.
//! A fresh plan reads those names on retry; discovery/repair owns the orphans,
//! and retry creates new Folios rather than adopting them.

mod apply;
mod destination;
mod error;
mod inspect;
mod links;
mod names;
mod plan;
mod refusal;
mod report;
mod request;

pub use apply::apply_register;
pub use error::RegisterError;
pub use links::{RegisterLinkOutcome, RegisterLinkResult, apply_register_links};
pub use plan::{
    NameSource, NameWrite, RegisterEntry, RegisterNote, RegisterPlan, RegisterStatus,
    RegisterSummary, plan_register,
};
pub use refusal::{NameHolder, RegisterRefusalReason};
pub use request::RegisterRequest;

#[cfg(test)]
mod tests;
