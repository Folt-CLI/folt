//! Hashes canonical objects incrementally or in memory; storage encoding lives elsewhere.

use sha2::{Digest, Sha256};

use crate::id::ObjectId;

use super::ObjectKind;

/// Incrementally hashes a framed object without buffering its body.
pub struct ObjectHasher {
    hasher: Sha256,
}

impl ObjectHasher {
    pub fn new(kind: ObjectKind, length: u64) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(kind.as_bytes());
        hasher.update(b" ");
        hasher.update(length.to_string().as_bytes());
        hasher.update([0]);
        Self { hasher }
    }
    pub fn update(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }
    pub fn finish(self) -> ObjectId {
        ObjectId::from_bytes(self.hasher.finalize().into())
    }
}

pub fn hash_object(kind: ObjectKind, body: &[u8]) -> ObjectId {
    let mut hasher = ObjectHasher::new(kind, body.len() as u64);
    hasher.update(body);
    hasher.finish()
}

pub fn hash_blob(body: &[u8]) -> ObjectId {
    hash_object(ObjectKind::Blob, body)
}
