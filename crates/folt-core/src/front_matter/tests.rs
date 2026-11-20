use std::ops::Range;

use crate::config::limits::SCAN_LIMIT;
use crate::config::names::{OWNED, OWNED_REFERENCES};
use crate::id::FolioId;

use super::edit::references::render_reference;
use super::edit::{checked_edit, edit};
use super::error::Result;
use super::keys::{audit, node};
use super::reference::references;
use super::scalar::text;
use super::*;

mod ambiguous_keys;
mod comments;
mod document;
mod explicit_keys;
mod owned_keys;
mod owners;
mod reference_edits;
mod references;
mod scalars;
mod syntax_tree;

const ID: &str = "12345678-1234-4234-8234-123456789abc";

fn logo() -> Reference<'static> {
    Reference {
        alias: "logo".into(),
        name: Some("logo-image".into()),
        id: None,
        pin: Some("abc123".into()),
    }
}

fn rename(bytes: &[u8], name: &str) -> Vec<u8> {
    apply_edit(
        bytes,
        &locate(bytes)
            .and_then(|block| Block::set_name(block.as_ref(), bytes, Some(name), true))
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
