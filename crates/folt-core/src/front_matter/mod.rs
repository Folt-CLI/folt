//! Lossless, bounded front matter inspection and byte-span edits.
//!
//! This is a deliberately small YAML subset, not a general YAML parser. Foreign
//! nodes remain opaque. An edit is valid only for the bytes it was planned on;
//! plan it again after applying another edit. Names are written as double-quoted
//! strings; ids remain unquoted.

mod edit;
mod entries;
mod error;
mod keys;
mod locate;
mod read;
mod reference;
mod scalar;
mod scan;
mod tree;

pub use edit::{Edit, apply_edit};
pub use error::FrontMatterError;
pub use locate::locate;
pub use read::{read_id, read_name};
pub use reference::{Reference, read_references};
pub use tree::{Block, Node};

#[cfg(test)]
mod tests;
