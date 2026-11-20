use std::fs::OpenOptions;
use std::io;

use crate::config::settings::Settings;
use crate::fsx::FileSystem;
use crate::home::Home;
use crate::placement_path::RegistryError;

use super::super::error::RegisterError;
use super::super::plan::{RegisterPlan, block_changed, plan_register};
use super::super::refusal::{RegisterRefusalReason, refuse_name};
use super::super::report::report;
use super::{ApplyState, Step};

pub(super) fn create_markers(
    fs: &dyn FileSystem,
    current: &RegisterPlan,
    state: &mut ApplyState,
    hook: &mut impl FnMut(Step<'_>) -> Result<(), RegistryError>,
) -> Result<(), RegisterError> {
    hook(Step::Markers)?;
    for entry in &current.entries {
        if entry.creates_marker {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&entry.path)
            {
                Ok(file) => {
                    let signature = crate::fsx::signature(&file.metadata()?);
                    state.markers.push((entry.path.clone(), signature));
                    crate::fsx::sync_file(&file)?;
                    drop(file);
                    if let Some(parent) = entry.path.parent() {
                        fs.sync_directory(parent)?;
                    }
                    hook(Step::MarkerSynced)?;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

pub(super) fn refresh(
    home: &Home,
    plan: &RegisterPlan,
    current: &RegisterPlan,
    settings: &Settings,
    hook: &mut impl FnMut(Step<'_>) -> Result<(), RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    // An appearing marker must be checked for type, readability and size.
    let mut refreshed = plan_register(home, plan.requests.clone())?;
    hook(Step::Refreshed(&mut refreshed))?;
    Ok(block_changed(
        current,
        refreshed,
        settings.batch_total_bytes,
    ))
}

pub(super) fn block_entry(
    current: &mut RegisterPlan,
    index: usize,
    reason: RegisterRefusalReason,
    settings: &Settings,
) -> Option<Vec<u8>> {
    refuse_name(&mut current.entries[index], reason);
    *current = report(
        current.entries.clone(),
        current.requests.clone(),
        current.destinations.clone(),
        settings.batch_total_bytes,
    );
    None
}
