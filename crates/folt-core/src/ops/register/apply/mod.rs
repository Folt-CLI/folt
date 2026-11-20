use std::path::PathBuf;

use crate::config::settings::Settings;
use crate::folio_dir::Durability;
use crate::fsx::{FileSystem, RealFileSystem};
use crate::home::{Home, HomeLock};
use crate::placement_path::RegistryError;

use super::error::RegisterError;
use super::plan::{RegisterPlan, RegisterStatus, block_changed, blocked, plan_register};

/// Replans under the exclusive home lock. Returns the plan actually applied,
/// or a blocked plan without writing. Does not create an initial history commit.
pub fn apply_register(
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
) -> Result<RegisterPlan, RegisterError> {
    apply_with_hook(&RealFileSystem, home, plan, confirm_large, |_| Ok(()))
}

// Hook payloads are observed by the regression tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum Step<'a> {
    Markers,
    Refreshed(&'a mut RegisterPlan),
    MarkerSynced,
    Folio(Durability),
    Transaction,
    Commit,
}

pub(super) fn apply_with_hook(
    fs: &dyn FileSystem,
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    hook: impl FnMut(Step<'_>) -> Result<(), RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    let lock = home.lock()?;
    apply_locked(fs, home, plan, confirm_large, &lock, hook)
}

pub(super) fn apply_locked(
    fs: &dyn FileSystem,
    home: &Home,
    plan: &RegisterPlan,
    confirm_large: bool,
    lock: &HomeLock,
    mut hook: impl FnMut(Step<'_>) -> Result<(), RegistryError>,
) -> Result<RegisterPlan, RegisterError> {
    let settings = Settings::load(home)?;
    let mut current = block_changed(
        plan,
        plan_register(home, plan.requests.clone())?,
        settings.batch_total_bytes,
    );
    if blocked(&current, confirm_large) {
        return Ok(current);
    }
    let mut state = ApplyState::default();
    let result: Result<bool, RegisterError> = (|| {
        markers::create_markers(fs, &current, &mut state, &mut hook)?;
        current = markers::refresh(home, plan, &current, &settings, &mut hook)?;
        if blocked(&current, confirm_large) {
            return Ok(false);
        }
        let mut rows = Vec::new();
        // Each index identifies a request in the refreshed plan. Display paths
        // may change spelling during replan, so never use them as identities.
        for (index, entry) in current.entries.clone().into_iter().enumerate() {
            if entry.status == RegisterStatus::AlreadyRegistered {
                continue;
            }
            let Some(content) = capture::capture_entry(
                fs,
                &mut current,
                index,
                &entry,
                &settings,
                confirm_large,
                &mut state,
            )?
            else {
                return Ok(false);
            };
            rows.push(publish::publish_entry(
                fs, home, lock, &entry, content, &mut state, &mut hook,
            )?);
        }
        publish::publish_rows(fs, home, lock, &rows, &mut hook)?;
        Ok(true)
    })();
    if matches!(result, Ok(true)) {
        // The empty file used to create an owned marker was only an
        // intermediate state, not an input snapshot for the returned plan.
        for entry in &mut current.entries {
            if state.markers.iter().any(|(path, _)| path == &entry.path) {
                entry.signature = None;
            }
        }
    } else {
        rollback::restore_apply(fs, state, &result)?;
    }
    result.map(|_| current)
}

#[derive(Default)]
struct ApplyState {
    markers: Vec<(PathBuf, crate::fsx::Signature)>,
    directories: Vec<PathBuf>,
    rewritten: Vec<(PathBuf, Vec<u8>, crate::fsx::Signature)>,
}

mod capture;
mod markers;
mod publish;
mod rollback;

#[cfg(test)]
mod tests;
