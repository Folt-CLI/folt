use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::config::settings::Settings;
use crate::folio::name::FolioName;
use crate::front_matter::Edit;
use crate::home::Home;
use crate::placement_path::{PlacementPath, absolute, path_bytes};
use crate::registry::Registry;

use super::destination::{destination, is_marker, register_destination};
use super::error::RegisterError;
use super::inspect::inspect_entry;
use super::names::resolve_names;
use super::refusal::{RegisterRefusalReason, refuse_name};
use super::report::report;
use super::request::RegisterRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameSource {
    FrontMatter,
    Option,
    FileName { suffix: u32 },
}

/// Exact prefix edit; apply must recompute it against the captured full bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameWrite {
    None,
    Insert(Edit),
    InsertBlock(Edit),
    Replace { old: String, edit: Edit },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterStatus {
    New,
    AlreadyRegistered,
    Refused(RegisterRefusalReason),
    NeedsConfirmation { size: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterEntry {
    pub path: PathBuf,
    pub status: RegisterStatus,
    pub size: u64,
    /// An empty marker to create in the user's directory, with no clobbering.
    pub creates_marker: bool,
    pub name: Option<FolioName>,
    pub name_source: Option<NameSource>,
    pub name_write: NameWrite,
    pub(super) signature: Option<crate::fsx::Signature>,
    pub(super) prefix: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegisterSummary {
    pub new: usize,
    pub already_registered: usize,
    pub refused: usize,
    pub needs_confirmation: usize,
    pub total_bytes: u64,
    pub files_to_write: usize,
    /// Up to ten entries, largest first (path breaks ties).
    pub largest: Vec<(PathBuf, u64)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterNote {
    NeedsConfirmation { size: u64 },
}

/// The dry-run result, also consumed by apply. Request paths remain private so
/// changing display data cannot bypass apply's checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterPlan {
    pub entries: Vec<RegisterEntry>,
    pub notes: Vec<RegisterNote>,
    pub summary: RegisterSummary,
    pub(super) requests: Vec<RegisterRequest>,
    // Resolved paths aligned with original requests, before entry deduplication.
    pub(super) destinations: Vec<PlacementPath>,
}

pub fn plan_register(
    home: &Home,
    paths: impl IntoIterator<Item = impl Into<RegisterRequest>>,
) -> Result<RegisterPlan, RegisterError> {
    let settings = Settings::load(home)?;
    let registry = Registry::open_existing(home)?;
    let home_path = fs::canonicalize(home.path())?;
    let mut entries: Vec<RegisterEntry> = Vec::new();
    let mut requests: Vec<RegisterRequest> = Vec::new();
    let mut seen = HashMap::<PathBuf, usize>::new();
    let mut replay = Vec::new();
    let mut destinations = Vec::new();
    for input in paths {
        let mut request = input.into();
        let absolute = absolute(&request.path)?;
        request.path = absolute.clone();
        replay.push(request.clone());
        let resolved = PlacementPath::resolve(&absolute);
        let path = resolved
            .as_ref()
            .map_or_else(|_| absolute.clone(), PlacementPath::to_path_buf);
        let directory = fs::symlink_metadata(&absolute).is_ok_and(|m| m.is_dir());
        destinations.push(PlacementPath::from_stored(path_bytes(
            register_destination(directory, &path, resolved.is_ok()),
        )));
        if let Some(&index) = seen.get(&path) {
            let previous: &mut RegisterRequest = &mut requests[index];
            if previous.name.is_some() && request.name.is_some() && previous.name != request.name {
                entries[index].status =
                    RegisterStatus::Refused(RegisterRefusalReason::ConflictingNames);
                continue;
            }
            if previous.name.is_none() {
                previous.name = request.name;
            }
            // Preserve directory intent in either request order.
            if directory {
                previous.path = absolute;
            }
            if !matches!(
                entries[index].status,
                RegisterStatus::Refused(RegisterRefusalReason::ConflictingNames)
            ) {
                entries[index] =
                    inspect_entry(&home_path, &registry, &settings, &previous.path, path)?;
            }
            continue;
        }
        seen.insert(path.clone(), entries.len());
        entries.push(inspect_entry(
            &home_path, &registry, &settings, &absolute, path,
        )?);
        requests.push(request);
    }
    resolve_names(&mut entries, &requests, &registry)?;
    Ok(report(
        entries,
        replay,
        destinations,
        settings.batch_total_bytes,
    ))
}

pub(super) fn block_changed(
    previous: &RegisterPlan,
    mut current: RegisterPlan,
    batch_total_bytes: u64,
) -> RegisterPlan {
    let mut previous_by_destination = HashMap::<_, Vec<_>>::new();
    for entry in &previous.entries {
        previous_by_destination
            .entry(PlacementPath::from_stored(path_bytes(destination(entry))))
            .or_default()
            .push(entry);
    }
    for entry in &mut current.entries {
        let destination = PlacementPath::from_stored(path_bytes(destination(entry)));
        let request_moved = previous
            .destinations
            .iter()
            .zip(&current.destinations)
            .any(|(old, new)| new == &destination && old != new);
        if request_moved {
            let path = entry.path.clone();
            refuse_name(entry, RegisterRefusalReason::DestinationChanged { path });
            continue;
        }
        let old = previous_by_destination
            .get(&destination)
            .and_then(|candidates| {
                // Distinct case-variant marker files retain their full path identities.
                // Accept a changed marker spelling only for an unambiguous destination.
                candidates
                    .iter()
                    .copied()
                    .find(|old| old.path == entry.path)
                    .or(match candidates.as_slice() {
                        [old] if is_marker(&old.path) && is_marker(&entry.path) => Some(*old),
                        _ => None,
                    })
            });
        let Some(old) = old else {
            let path = entry.path.clone();
            refuse_name(entry, RegisterRefusalReason::DestinationChanged { path });
            continue;
        };
        if matches!(
            entry.status,
            RegisterStatus::New | RegisterStatus::NeedsConfirmation { .. }
        ) && old.prefix.is_some()
            && old.signature.is_some()
            && (old.signature != entry.signature || old.prefix != entry.prefix)
        {
            refuse_name(entry, RegisterRefusalReason::FileChanged);
        }
    }
    report(
        current.entries,
        current.requests,
        current.destinations,
        batch_total_bytes,
    )
}

pub(super) fn blocked(plan: &RegisterPlan, confirm_large: bool) -> bool {
    plan.summary.refused != 0
        || (!confirm_large && (plan.summary.needs_confirmation != 0 || !plan.notes.is_empty()))
}
