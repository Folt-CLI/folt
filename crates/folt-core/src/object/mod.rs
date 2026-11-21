//! Canonical object types, framing, hashing and compression; filesystem storage lives elsewhere.

mod author;
mod commit;
mod compress;
mod error;
mod fields;
mod frame;
mod hash;
mod kind;
mod stored;
mod stream;
mod tag;
mod time;

pub use author::Author;
pub use commit::Commit;
pub use compress::compress;
pub use error::ObjectError;
pub use frame::{decode, encode, peek_kind, read_prefix};
pub use hash::{ObjectHasher, hash_blob, hash_object};
pub use kind::ObjectKind;
pub use stored::open_stored;
pub use stream::{ObjectReader, ObjectWriter};
pub use tag::Tag;
pub use time::CommitTime;

pub(crate) use compress::{
    already_compressed, compression_probe_is_worthwhile, compression_samples_are_worthwhile,
};

#[cfg(test)]
mod tests;
