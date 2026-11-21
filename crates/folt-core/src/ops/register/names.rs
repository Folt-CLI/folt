use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::folio::name::FolioName;
use crate::front_matter::{self, Block};
use crate::registry::Registry;

use super::error::RegisterError;
use super::plan::{NameSource, NameWrite, RegisterEntry, RegisterStatus};
use super::refusal::{NameHolder, RegisterRefusalReason, refuse_name};
use super::request::RegisterRequest;

mod files;

use files::names_from_files;

pub(super) fn resolve_names(
    entries: &mut [RegisterEntry],
    requests: &[RegisterRequest],
    registry: &Registry,
) -> Result<(), RegisterError> {
    let prefixes = names_from_files(entries, requests);
    let mut assigned = HashMap::new();
    reserve_names(entries, registry, &mut assigned)?;
    let mut counters = HashMap::new();
    for (index, (entry, request)) in entries.iter_mut().zip(requests).enumerate() {
        if !matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) {
            continue;
        }
        name_from_stem(entry, request, registry, &mut assigned, &mut counters)?;
        // An unusable stem refuses the entry before planning its edit.
        if matches!(entry.status, RegisterStatus::Refused(_)) {
            continue;
        }
        plan_name_write(entry, prefixes.get(&index))?;
    }
    Ok(())
}

fn reserve_names(
    entries: &mut [RegisterEntry],
    registry: &Registry,
    assigned: &mut HashMap<FolioName, PathBuf>,
) -> Result<(), RegisterError> {
    // Explicit options first, then front matter, preserving request order in each group.
    for option_pass in [true, false] {
        for entry in entries.iter_mut() {
            if entry.name.is_none()
                || matches!(entry.name_source, Some(NameSource::Option)) != option_pass
            {
                continue;
            }
            let Some(name) = entry.name.clone() else {
                continue;
            };
            if let Some(holder) = holder(registry, assigned, &name)? {
                refuse_name(entry, RegisterRefusalReason::NameTaken { name, holder });
            } else {
                assigned.insert(name, entry.path.clone());
            }
        }
    }
    Ok(())
}

fn name_from_stem(
    entry: &mut RegisterEntry,
    request: &RegisterRequest,
    registry: &Registry,
    assigned: &mut HashMap<FolioName, PathBuf>,
    counters: &mut HashMap<FolioName, u32>,
) -> Result<(), RegisterError> {
    if entry.name.is_none() {
        let directory = fs::symlink_metadata(&request.path).is_ok_and(|m| m.is_dir());
        let stem = if directory {
            entry.path.parent().and_then(Path::file_name)
        } else {
            request.path.file_stem()
        };
        let base = match FolioName::from_stem(stem.unwrap_or_default()) {
            Ok(name) => name,
            Err(error) => {
                refuse_name(entry, RegisterRefusalReason::NoUsableStem(error));
                return Ok(());
            }
        };
        let counter = counters.entry(base.clone()).or_default();
        loop {
            let name = if *counter == 0 {
                base.clone()
            } else {
                base.with_suffix(*counter)
            };
            let suffix = *counter;
            *counter = counter
                .checked_add(1)
                .ok_or_else(|| io::Error::other("name suffix exhausted"))?;
            if holder(registry, assigned, &name)?.is_none() {
                assigned.insert(name.clone(), entry.path.clone());
                entry.name = Some(name);
                entry.name_source = Some(NameSource::FileName { suffix });
                break;
            }
        }
    }
    Ok(())
}

fn plan_name_write(
    entry: &mut RegisterEntry,
    prefix: Option<&Vec<u8>>,
) -> Result<(), RegisterError> {
    if entry.name_source == Some(NameSource::FrontMatter) {
        return Ok(());
    }
    if let Some(bytes) = prefix {
        let name = entry.name.as_ref().ok_or_else(|| {
            io::Error::other(format!(
                "missing resolved folio-name: {}",
                entry.path.display()
            ))
        })?;
        let edit = match front_matter::locate(bytes)
            .and_then(|block| Block::set_name(block.as_ref(), bytes, Some(name.as_str()), true))
        {
            Ok(Some(edit)) => edit,
            Ok(None) => return Ok(()),
            Err(error) => {
                refuse_name(entry, RegisterRefusalReason::FrontMatter(error));
                return Ok(());
            }
        };
        entry.name_write = if front_matter::locate(bytes)
            .map_err(|e| io::Error::other(e.to_string()))?
            .is_some()
        {
            NameWrite::Insert(edit)
        } else {
            NameWrite::InsertBlock(edit)
        };
        if !entry.creates_marker {
            let meta = fs::metadata(&entry.path)?;
            let reason = if meta.permissions().readonly() {
                Some("read-only file".to_owned())
            } else {
                crate::placement_fs::require_single_link(&entry.path)
                    .err()
                    .map(|e| e.to_string())
            };
            if let Some(reason) = reason {
                refuse_name(entry, RegisterRefusalReason::NameNotWritable { reason });
            }
        }
    }
    Ok(())
}

fn holder(
    registry: &Registry,
    assigned: &HashMap<FolioName, PathBuf>,
    name: &FolioName,
) -> Result<Option<NameHolder>, RegisterError> {
    // Exactly one indexed lookup for each candidate, including local collisions.
    let registered = registry.find_by_name(name)?;
    Ok(registered
        .map(NameHolder::Folio)
        .or_else(|| assigned.get(name).cloned().map(NameHolder::Path)))
}
