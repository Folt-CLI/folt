use crate::folio::name::FolioName;
use crate::front_matter::{self, Block, FrontMatterError};
use crate::ops::rename::RenameError;
use crate::ops::stage;
use crate::registry::Placement;

fn renamed(bytes: &[u8], name: &FolioName) -> Result<Vec<u8>, RenameError> {
    let edit = Block::set_name(
        front_matter::locate(bytes)?.as_ref(),
        bytes,
        Some(name.as_str()),
        false,
    )?;
    Ok(match edit {
        Some(edit) => front_matter::apply_edit(bytes, &edit)?,
        None => bytes.to_vec(),
    })
}

pub(super) fn prepare_replacement(
    row: &Placement,
    base_bytes: &[u8],
    old: &FolioName,
    new: &FolioName,
    retry: bool,
) -> Result<Option<Vec<u8>>, RenameError> {
    let named = match front_matter::read_name(base_bytes) {
        Ok(Some(name)) => {
            // A fresh rename must leave a base that its pending retry accepts.
            if !retry && name != old.as_str() && name != new.as_str() {
                return Err(RenameError::InternalNameMismatch {
                    path: row.path.to_path_buf(),
                    name: name.into_owned(),
                });
            }
            true
        }
        Ok(None) | Err(FrontMatterError::Binary) => false,
        Err(error) => return Err(RenameError::FrontMatter(error)),
    };
    // The pending record identifies the operation. The base must still hold
    // its old name or the new name from a commit completed before record clear.
    if retry {
        let base_name = front_matter::read_name(base_bytes)?;
        if !named
            || !matches!(base_name.as_deref(), Some(name) if name == old.as_str() || name == new.as_str())
        {
            return Err(RenameError::UnknownName(old.clone()));
        }
    }
    let replacement = if named && old != new {
        let replacement = renamed(base_bytes, new)?;
        (replacement != base_bytes).then_some(replacement)
    } else {
        None
    };
    // Preflight both history and the exact replacement prefix before any
    // registry/file writes, even when resuming an own edit or existing stage.
    for content in std::iter::once(base_bytes).chain(replacement.as_deref()) {
        for reference in stage::stage_references(row, content)? {
            crate::registry::reference_name(&reference)?;
        }
    }
    Ok(replacement)
}
