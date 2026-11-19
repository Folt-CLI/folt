use super::edit::{checked_edit, edit, render_reference};
use super::*;

mod ambiguous_keys;
mod comments;
mod document;
mod explicit_keys;
mod owned_keys;
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
