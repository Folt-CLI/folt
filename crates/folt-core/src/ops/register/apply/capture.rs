use std::fs;
use std::io;

use crate::config::settings::Settings;
use crate::folio::name::FolioName;
use crate::front_matter::{self, Block};
use crate::fsx::FileSystem;
use crate::ops::rollback::Rollback;

use super::super::error::RegisterError;
use super::super::plan::{NameWrite, RegisterEntry, RegisterPlan, RegisterStatus, blocked};
use super::super::refusal::{RegisterRefusalReason, io_refusal};
use super::super::report::report;
use super::markers::block_entry;

pub(super) fn capture_entry(
    fs: &dyn FileSystem,
    current: &mut RegisterPlan,
    index: usize,
    entry: &RegisterEntry,
    settings: &Settings,
    confirm_large: bool,
    state: &mut Rollback<'_>,
) -> Result<Option<Vec<u8>>, RegisterError> {
    let content = match fs::read(&entry.path) {
        Ok(content) => content,
        Err(error) => {
            return Ok(block_entry(current, index, io_refusal(&error), settings));
        }
    };
    if let Some(expected) = &entry.signature {
        let metadata = fs::metadata(&entry.path)?;
        if &crate::fsx::signature(&metadata) != expected
            || entry
                .prefix
                .as_ref()
                .is_some_and(|prefix| front_matter::scan_window(&content) != prefix.as_slice())
        {
            return Ok(block_entry(
                current,
                index,
                RegisterRefusalReason::FileChanged,
                settings,
            ));
        }
    }
    let Some(content) = write_name(fs, current, index, settings, state, entry, content)? else {
        return Ok(None);
    };
    // A file may grow even after the metadata recheck. Confirmation
    // also covers the bytes actually captured, before publishing them.
    current.entries[index].size = content.len() as u64;
    if content.len() as u64 >= settings.large_file_bytes {
        current.entries[index].status = RegisterStatus::NeedsConfirmation {
            size: content.len() as u64,
        };
    }
    *current = report(
        current.entries.clone(),
        current.requests.clone(),
        current.destinations.clone(),
        settings.batch_total_bytes,
    );
    if blocked(current, confirm_large) {
        return Ok(None);
    }
    Ok(Some(content))
}

fn write_name(
    fs: &dyn FileSystem,
    current: &mut RegisterPlan,
    index: usize,
    settings: &Settings,
    state: &mut Rollback<'_>,
    entry: &RegisterEntry,
    content: Vec<u8>,
) -> Result<Option<Vec<u8>>, RegisterError> {
    if entry.name_write != NameWrite::None {
        let edit = front_matter::locate(&content)
            .and_then(|block| {
                Block::set_name(
                    block.as_ref(),
                    &content,
                    entry.name.as_ref().map(FolioName::as_str),
                    true,
                )
            })
            .map_err(|error| io::Error::other(format!("{}: {error}", entry.path.display())))?;
        let planned_edit = match &entry.name_write {
            NameWrite::Insert(edit)
            | NameWrite::InsertBlock(edit)
            | NameWrite::Replace { edit, .. } => edit,
            NameWrite::None => {
                return Err(io::Error::other("missing planned folio-name edit").into());
            }
        };
        if edit.as_ref() != Some(planned_edit) {
            return Ok(block_entry(
                current,
                index,
                RegisterRefusalReason::FileChanged,
                settings,
            ));
        }
        let updated = front_matter::apply_edit(&content, planned_edit)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let Some(expected) = entry.signature.as_ref() else {
            return Ok(block_entry(
                current,
                index,
                RegisterRefusalReason::FileChanged,
                settings,
            ));
        };
        let write = crate::fsx::atomic_rewrite(fs, &entry.path, &updated, expected, |signature| {
            state.record_rewrite(entry.path.clone(), content.clone(), signature);
        });
        if let Err(error) = write {
            if error.kind() == io::ErrorKind::InvalidData {
                return Ok(block_entry(
                    current,
                    index,
                    RegisterRefusalReason::FileChanged,
                    settings,
                ));
            }
            return Err(error.into());
        }
        return Ok(Some(updated));
    }
    Ok(Some(content))
}
